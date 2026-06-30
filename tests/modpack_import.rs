//! Integration tests for standard modpack format import/export.
//!
//! Covers:
//! - `.mrpack` import: mods from modrinth.index.json + overrides copied with path safety.
//! - CurseForge `.zip` import: manifest.json + overrides; unresolvable mods → warnings.
//! - Path traversal (`../evil`, absolute, backslash) rejected; no partial install.
//! - Oversized archive (declared size > limit) rejected.
//! - `pkg make --format mrpack` export → round-trip import reproduces mod set.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use mcm::parse_mcm_lock;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

struct TestHome {
    #[allow(dead_code)]
    root: TempDir,
    config: PathBuf,
    state: PathBuf,
    mods: PathBuf,
}

impl TestHome {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        let config = root.path().join("config");
        let state = root.path().join("state");
        let mods = root.path().join("mods");
        fs::create_dir_all(&mods).expect("mods dir");
        Self {
            root,
            config,
            state,
            mods,
        }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("mcm").expect("mcm binary");
        cmd.args([
            "--config-dir",
            self.config.to_str().unwrap(),
            "--state-dir",
            self.state.to_str().unwrap(),
            "--provider",
            "mock",
        ]);
        cmd
    }

    fn profile(&self) {
        self.cmd()
            .args([
                "mods",
                "add",
                "dev",
                "--mods-dir",
                self.mods.to_str().unwrap(),
                "--mc-version",
                "1.20.1",
                "--loader",
                "fabric",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("added profile dev"));
    }

    fn profile_named(&self, name: &str, mods_subdir: &str) {
        let mods_dir = self.root.path().join(mods_subdir);
        fs::create_dir_all(&mods_dir).expect("mods dir");
        self.cmd()
            .args([
                "mods",
                "add",
                name,
                "--mods-dir",
                mods_dir.to_str().unwrap(),
                "--mc-version",
                "1.20.1",
                "--loader",
                "fabric",
            ])
            .assert()
            .success();
        self.cmd().args(["mods", "use", name]).assert().success();
    }
}

/// CRC-32 (ISO 3309 / zlib polynomial) for the hand-rolled stored-zip builder.
/// The `zip` crate is a private dependency, not a dev-dependency, so integration
/// tests build minimal valid ZIP archives byte-for-byte.
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    crc ^ 0xFFFF_FFFF
}

/// Build a minimal valid stored ZIP archive (method=0, no compression).
fn build_stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    build_stored_zip_with_sizes(entries, &[])
}

/// Like `build_stored_zip` but allows overriding the uncompressed size declared
/// in the central directory for specific entries (by index). This is used to
/// simulate zip-bomb-like archives where the central directory claims large
/// sizes but the actual data is small.
fn build_stored_zip_with_sizes(
    entries: &[(&str, &[u8])],
    fake_sizes: &[(usize, u32)], // (entry_index, fake_uncompressed_size)
) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let mut offset: u32 = 0;
    for (i, (name, data)) in entries.iter().enumerate() {
        let crc = crc32(data);
        let name_bytes = name.as_bytes();
        let name_len = u16::try_from(name_bytes.len()).unwrap();
        let data_len = u32::try_from(data.len()).unwrap();

        // Local file header (always uses real data_len).
        buf.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
        buf.extend_from_slice(&20u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // method = stored
        buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
        buf.extend_from_slice(&0u16.to_le_bytes()); // mod date
        buf.extend_from_slice(&crc.to_le_bytes());
        buf.extend_from_slice(&data_len.to_le_bytes()); // compressed size
        buf.extend_from_slice(&data_len.to_le_bytes()); // uncompressed size
        buf.extend_from_slice(&name_len.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // extra len
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(data);

        // Central directory header (may use fake size).
        let cd_size = fake_sizes
            .iter()
            .find(|(idx, _)| *idx == i)
            .map(|(_, s)| *s)
            .unwrap_or(data_len);
        central.extend_from_slice(&[0x50, 0x4B, 0x01, 0x02]);
        central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central.extend_from_slice(&0u16.to_le_bytes()); // flags
        central.extend_from_slice(&0u16.to_le_bytes()); // method
        central.extend_from_slice(&0u16.to_le_bytes()); // mod time
        central.extend_from_slice(&0u16.to_le_bytes()); // mod date
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&cd_size.to_le_bytes()); // compressed size
        central.extend_from_slice(&cd_size.to_le_bytes()); // uncompressed size
        central.extend_from_slice(&name_len.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra len
        central.extend_from_slice(&0u16.to_le_bytes()); // comment len
        central.extend_from_slice(&0u16.to_le_bytes()); // disk number
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name_bytes);

        offset += 30 + u32::from(name_len) + data_len;
    }
    let cd_offset = u32::try_from(buf.len()).unwrap();
    buf.extend_from_slice(&central);
    let cd_size = u32::try_from(central.len()).unwrap();
    let entry_count = u16::try_from(entries.len()).unwrap();

    // End of central directory record.
    buf.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06]);
    buf.extend_from_slice(&0u16.to_le_bytes()); // disk number
    buf.extend_from_slice(&0u16.to_le_bytes()); // disk with CD
    buf.extend_from_slice(&entry_count.to_le_bytes()); // entries on this disk
    buf.extend_from_slice(&entry_count.to_le_bytes()); // total entries
    buf.extend_from_slice(&cd_size.to_le_bytes());
    buf.extend_from_slice(&cd_offset.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // comment len
    buf
}

fn write_file(dir: &Path, name: &str, data: &[u8]) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, data).expect("write file");
    path
}

/// Build a minimal valid .mrpack zip for import testing.
/// The mod file entry has empty downloads (so import reads from overrides)
/// and a sha512 hash matching the embedded mock jar bytes.
fn build_mrpack(mod_jar_bytes: &[u8], config_content: &str) -> Vec<u8> {
    use sha2::{Digest, Sha512};
    let hash = hex::encode(Sha512::digest(mod_jar_bytes));
    let index = format!(
        r#"{{"format":1,"game":"minecraft","versionId":"1.0.0","dependencies":{{"minecraft":"1.20.1","fabric-loader":"0.15.0"}},"files":[{{"path":"mods/testmod.jar","hashes":{{"sha512":"{hash}"}},"downloads":[],"fileSize":{size},"mcm":{{"logical_id":"testmod","provider":"mock","project_id":"testmod","file_id":"testmod-file","version":"1.0.0","sha256":"abc"}}}}]}}"#,
        hash = hash,
        size = mod_jar_bytes.len(),
    );
    build_stored_zip(&[
        ("modrinth.index.json", index.as_bytes()),
        ("overrides/mods/testmod.jar", mod_jar_bytes),
        ("overrides/config/sodium.toml", config_content.as_bytes()),
    ])
}

/// Build a minimal CurseForge manifest zip for import testing.
fn build_curseforge_zip(overrides_content: &str) -> Vec<u8> {
    let manifest = r#"{"minecraft":{"version":"1.20.1","modLoaders":[{"id":"fabric-0.15.0","primary":true}]},"manifestType":"minecraftModpack","manifestVersion":1,"name":"cf-test","version":"1.0.0","author":"test","files":[{"projectID":123456,"fileID":789012,"required":true}]}"#;
    build_stored_zip(&[
        ("manifest.json", manifest.as_bytes()),
        ("overrides/config/options.txt", overrides_content.as_bytes()),
    ])
}

// ---------------------------------------------------------------------------
// .mrpack import: mods + overrides
// ---------------------------------------------------------------------------

#[test]
fn mrpack_import_installs_mods_and_overrides() {
    let home = TestHome::new();
    home.profile();
    let game_root = home.mods.parent().unwrap().to_path_buf();

    // Mock provider returns: "mock mcm jar\nid={file_id}\nversion={version}\n"
    let mod_bytes = b"mock mcm jar\nid=testmod-file\nversion=1.0.0\n";
    let zip_bytes = build_mrpack(mod_bytes, "# sodium config\n");
    let zip_path = write_file(home.root.path(), "test.mrpack", &zip_bytes);

    home.cmd()
        .args(["pkg", "install", zip_path.to_str().unwrap(), "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("imported modpack"));

    // Mod jar written to game root mods dir.
    assert!(game_root.join("mods/testmod.jar").exists());
    // Override config file copied.
    assert!(game_root.join("config/sodium.toml").exists());
    // Lock entry recorded.
    let lock_text = fs::read_to_string(home.state.join("dev.lock.json")).expect("lock file");
    assert!(lock_text.contains("testmod"));
}

#[test]
fn mrpack_import_without_yes_bails() {
    let home = TestHome::new();
    home.profile();
    let mod_bytes = b"mock mcm jar\nid=testmod-file\nversion=1.0.0\n";
    let zip_bytes = build_mrpack(mod_bytes, "# config\n");
    let zip_path = write_file(home.root.path(), "test.mrpack", &zip_bytes);

    home.cmd()
        .args(["pkg", "install", zip_path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "confirmation required; pass --yes to proceed",
        ));
}

#[test]
fn mrpack_import_hash_mismatch_rejected() {
    let home = TestHome::new();
    home.profile();
    let mod_bytes = b"mock mcm jar\nid=testmod-file\nversion=1.0.0\n";
    // Build mrpack with a WRONG hash.
    let index = r#"{"format":1,"game":"minecraft","versionId":"1.0.0","dependencies":{},"files":[{"path":"mods/testmod.jar","hashes":{"sha512":"deadbeef"},"downloads":[],"fileSize":10,"mcm":{"logical_id":"testmod","provider":"mock","project_id":"testmod","file_id":"testmod-file","version":"1.0.0","sha256":"abc"}}]}"#;
    let zip_bytes = build_stored_zip(&[
        ("modrinth.index.json", index.as_bytes()),
        ("overrides/mods/testmod.jar", mod_bytes),
    ]);
    let zip_path = write_file(home.root.path(), "bad.mrpack", &zip_bytes);

    home.cmd()
        .args(["pkg", "install", zip_path.to_str().unwrap(), "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("hash mismatch"));
}

// ---------------------------------------------------------------------------
// CurseForge manifest import
// ---------------------------------------------------------------------------

#[test]
fn curseforge_import_installs_overrides_and_warns_on_unresolvable_mods() {
    let home = TestHome::new();
    home.profile();
    let game_root = home.mods.parent().unwrap().to_path_buf();
    let zip_bytes = build_curseforge_zip("# options\n");
    let zip_path = write_file(home.root.path(), "cf.zip", &zip_bytes);

    home.cmd()
        .args(["pkg", "install", zip_path.to_str().unwrap(), "--yes"])
        .assert()
        .success()
        .stderr(predicate::str::contains("warning")); // unresolvable mod warning

    // Override copied.
    assert!(game_root.join("config/options.txt").exists());
}

// ---------------------------------------------------------------------------
// Path traversal rejection — no partial install
// ---------------------------------------------------------------------------

#[test]
fn path_traversal_entry_rejected_no_partial_install() {
    let home = TestHome::new();
    home.profile();
    let game_root = home.mods.parent().unwrap().to_path_buf();
    let index =
        r#"{"format":1,"game":"minecraft","versionId":"1.0.0","dependencies":{},"files":[]}"#;
    let zip_bytes = build_stored_zip(&[
        ("modrinth.index.json", index.as_bytes()),
        ("overrides/../evil.txt", b"evil"),
        ("overrides/config/safe.toml", b"safe"),
    ]);
    let zip_path = write_file(home.root.path(), "evil.mrpack", &zip_bytes);

    home.cmd()
        .args(["pkg", "install", zip_path.to_str().unwrap(), "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("traversal").or(predicate::str::contains("asset path")));

    // No partial install: neither evil.txt nor safe.toml should exist.
    assert!(!game_root.join("evil.txt").exists());
    assert!(!game_root.join("config/safe.toml").exists());
}

#[test]
fn absolute_path_entry_rejected() {
    let home = TestHome::new();
    home.profile();
    let index =
        r#"{"format":1,"game":"minecraft","versionId":"1.0.0","dependencies":{},"files":[]}"#;
    let zip_bytes = build_stored_zip(&[
        ("modrinth.index.json", index.as_bytes()),
        ("overrides//etc/passwd", b"bad"),
    ]);
    let zip_path = write_file(home.root.path(), "abs.mrpack", &zip_bytes);

    home.cmd()
        .args(["pkg", "install", zip_path.to_str().unwrap(), "--yes"])
        .assert()
        .failure();
}

#[test]
fn backslash_path_entry_rejected() {
    let home = TestHome::new();
    home.profile();
    let index =
        r#"{"format":1,"game":"minecraft","versionId":"1.0.0","dependencies":{},"files":[]}"#;
    let zip_bytes = build_stored_zip(&[
        ("modrinth.index.json", index.as_bytes()),
        ("overrides/config\\evil.toml", b"bad"),
    ]);
    let zip_path = write_file(home.root.path(), "bs.mrpack", &zip_bytes);

    home.cmd()
        .args(["pkg", "install", zip_path.to_str().unwrap(), "--yes"])
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// Oversized archive rejection
// ---------------------------------------------------------------------------

#[test]
fn oversized_archive_rejected() {
    let home = TestHome::new();
    home.profile();
    let index =
        r#"{"format":1,"game":"minecraft","versionId":"1.0.0","dependencies":{},"files":[]}"#;
    // Declare a 300MB entry in the central directory but with tiny actual data.
    let zip_bytes = build_stored_zip_with_sizes(
        &[
            ("modrinth.index.json", index.as_bytes()),
            ("overrides/big.dat", b"x"),
        ],
        &[(1, 300_000_000)], // entry index 1 claims 300MB
    );
    let zip_path = write_file(home.root.path(), "huge.mrpack", &zip_bytes);

    home.cmd()
        .args(["pkg", "install", zip_path.to_str().unwrap(), "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("exceeds").or(predicate::str::contains("size")));
}

// ---------------------------------------------------------------------------
// Secret field rejection in modpack JSON
// ---------------------------------------------------------------------------

#[test]
fn secret_field_in_mrpack_index_rejected() {
    let home = TestHome::new();
    home.profile();
    let index = r#"{"format":1,"game":"minecraft","versionId":"1.0.0","dependencies":{},"files":[],"api_key":"stolen"}"#;
    let zip_bytes = build_stored_zip(&[("modrinth.index.json", index.as_bytes())]);
    let zip_path = write_file(home.root.path(), "secret.mrpack", &zip_bytes);

    home.cmd()
        .args(["pkg", "install", zip_path.to_str().unwrap(), "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("secret"));
}

// ---------------------------------------------------------------------------
// Non-modpack zip falls through to .mcm error
// ---------------------------------------------------------------------------

#[test]
fn non_modpack_zip_rejected() {
    let home = TestHome::new();
    home.profile();
    let zip_bytes = build_stored_zip(&[("random.txt", b"not a modpack")]);
    let zip_path = write_file(home.root.path(), "random.zip", &zip_bytes);

    home.cmd()
        .args(["pkg", "install", zip_path.to_str().unwrap(), "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("modpack").or(predicate::str::contains("not a modpack")));
}

// ---------------------------------------------------------------------------
// Export: pkg make --format mrpack
// ---------------------------------------------------------------------------

#[test]
fn pkg_make_mrpack_writes_valid_mrpack() {
    let home = TestHome::new();
    home.profile();
    // Install a mod first so the lock has an entry.
    let mcm_json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "seed", "version": "1.0.0"},
        "permissions": {"install": true},
        "game": {"version": "1.20.1", "loader": "fabric"},
        "steps": [
            {
                "op": "mod.install",
                "permission": "install",
                "args": {
                    "id": "rootmod",
                    "provider": "mock",
                    "project_id": "rootmod",
                    "file_id": "rootmod-file",
                    "version": "1.0.0",
                    "filename": "rootmod-1.0.0.jar",
                    "download_url": "https://cdn.modrinth.com/mock/rootmod"
                }
            }
        ],
        "artifacts": [],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let mcm_path = write_file(home.root.path(), "seed.mcm", mcm_json.as_bytes());
    home.cmd()
        .args(["pkg", "install", mcm_path.to_str().unwrap(), "--yes"])
        .assert()
        .success();

    // Export to mrpack format.
    let cwd = home.root.path();
    let mut cmd = Command::cargo_bin("mcm").expect("mcm binary");
    cmd.current_dir(cwd).args([
        "--config-dir",
        home.config.to_str().unwrap(),
        "--state-dir",
        home.state.to_str().unwrap(),
        "--provider",
        "mock",
        "pkg",
        "make",
        "--format",
        "mrpack",
        "--yes",
    ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("wrote").or(predicate::str::contains(".mrpack")));

    // The .mrpack file should exist.
    let mrpack_path = cwd.join("dev.mrpack");
    assert!(mrpack_path.exists(), "dev.mrpack should exist after export");
}

// ---------------------------------------------------------------------------
// Export: pkg make --format curseforge
// ---------------------------------------------------------------------------

#[test]
fn pkg_make_curseforge_writes_valid_zip() {
    let home = TestHome::new();
    home.profile();
    let mcm_json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "seed", "version": "1.0.0"},
        "permissions": {"install": true},
        "game": {"version": "1.20.1", "loader": "fabric"},
        "steps": [
            {
                "op": "mod.install",
                "permission": "install",
                "args": {
                    "id": "rootmod",
                    "provider": "curseforge",
                    "project_id": "12345",
                    "file_id": "67890",
                    "version": "1.0.0",
                    "filename": "rootmod-1.0.0.jar",
                    "download_url": "https://edge.forgecdn.net/mock/rootmod"
                }
            }
        ],
        "artifacts": [],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let mcm_path = write_file(home.root.path(), "seed.mcm", mcm_json.as_bytes());
    home.cmd()
        .args(["pkg", "install", mcm_path.to_str().unwrap(), "--yes"])
        .assert()
        .success();

    let cwd = home.root.path();
    let mut cmd = Command::cargo_bin("mcm").expect("mcm binary");
    cmd.current_dir(cwd).args([
        "--config-dir", home.config.to_str().unwrap(),
        "--state-dir", home.state.to_str().unwrap(),
        "--provider", "mock",
        "pkg", "make", "--format", "curseforge", "--yes",
    ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("wrote").or(predicate::str::contains(".zip")));

    // The .zip file should exist.
    let zip_path = cwd.join("dev.zip");
    assert!(zip_path.exists(), "dev.zip should exist after curseforge export");

    // Verify manifest.json is present in the zip with correct structure.
    let zip_bytes = fs::read(&zip_path).expect("read zip");
    let cursor = std::io::Cursor::new(zip_bytes.as_slice());
    let mut archive = zip::ZipArchive::new(cursor).expect("open zip");
    let manifest_idx = (0..archive.len())
        .find(|i| {
            archive.by_index(*i).map(|e| e.name() == "manifest.json").unwrap_or(false)
        })
        .expect("manifest.json not found in zip");
    use std::io::Read;
    let mut manifest_str = String::new();
    archive.by_index(manifest_idx).unwrap().read_to_string(&mut manifest_str).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_str).expect("parse manifest");
    assert_eq!(manifest["manifestType"], "minecraftModpack");
    assert_eq!(manifest["manifestVersion"], 1);
    assert_eq!(manifest["minecraft"]["version"], "1.20.1");
    assert_eq!(manifest["minecraft"]["modLoaders"][0]["id"], "fabric");
    // The curseforge-sourced mod should appear in the files list.
    let files = manifest["files"].as_array().expect("files array");
    assert!(!files.is_empty(), "files list should contain the installed mod");
    assert_eq!(files[0]["projectID"], 12345);
    assert_eq!(files[0]["fileID"], 67890);
}

#[test]
fn pkg_make_mrpack_round_trip_import() {
    let home = TestHome::new();
    home.profile();

    // Install a mod so the lock has an entry and the jar is on disk.
    let mcm_json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "seed", "version": "1.0.0"},
        "permissions": {"install": true},
        "game": {"version": "1.20.1", "loader": "fabric"},
        "steps": [
            {
                "op": "mod.install",
                "permission": "install",
                "args": {
                    "id": "rootmod",
                    "provider": "mock",
                    "project_id": "rootmod",
                    "file_id": "rootmod-file",
                    "version": "1.0.0",
                    "filename": "rootmod-1.0.0.jar",
                    "download_url": "https://cdn.modrinth.com/mock/rootmod"
                }
            }
        ],
        "artifacts": [],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let mcm_path = write_file(home.root.path(), "seed.mcm", mcm_json.as_bytes());
    home.cmd()
        .args(["pkg", "install", mcm_path.to_str().unwrap(), "--yes"])
        .assert()
        .success();

    // Export to mrpack.
    let cwd = home.root.path();
    let mut cmd = Command::cargo_bin("mcm").expect("mcm binary");
    cmd.current_dir(cwd).args([
        "--config-dir",
        home.config.to_str().unwrap(),
        "--state-dir",
        home.state.to_str().unwrap(),
        "--provider",
        "mock",
        "pkg",
        "make",
        "--format",
        "mrpack",
        "--yes",
    ]);
    cmd.assert().success();
    let mrpack_path = cwd.join("dev.mrpack");
    assert!(mrpack_path.exists());

    // Create a fresh profile and import the mrpack.
    home.profile_named("target", "target_mods");

    home.cmd()
        .args(["pkg", "install", mrpack_path.to_str().unwrap(), "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("imported modpack"));

    // The mod jar should exist in the new profile's mods dir.
    assert!(
        home.root
            .path()
            .join("target_mods/rootmod-1.0.0.jar")
            .exists(),
        "mod jar should be installed in target profile"
    );

    // Lock entry should exist.
    let lock_text = fs::read_to_string(home.state.join("target.lock.json")).expect("lock file");
    assert!(lock_text.contains("rootmod"));
}

// ---------------------------------------------------------------------------
// Top-level install dispatches to modpack import for .mrpack
// ---------------------------------------------------------------------------

#[test]
fn top_install_dispatches_to_mrpack_import() {
    let home = TestHome::new();
    home.profile();
    let mod_bytes = b"mock mcm jar\nid=testmod-file\nversion=1.0.0\n";
    let zip_bytes = build_mrpack(mod_bytes, "# config\n");
    let zip_path = write_file(home.root.path(), "test.mrpack", &zip_bytes);

    home.cmd()
        .args(["install", zip_path.to_str().unwrap(), "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("imported modpack"));
}

// ---------------------------------------------------------------------------
// pkg make default format is still mcm (regression)
// ---------------------------------------------------------------------------

#[test]
fn pkg_make_default_format_is_mcm() {
    let home = TestHome::new();
    home.profile();
    let output = home
        .cmd()
        .args(["pkg", "make"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = String::from_utf8(output).expect("utf8");
    parse_mcm_lock(&json).expect("make output should parse as .mcm");
}
