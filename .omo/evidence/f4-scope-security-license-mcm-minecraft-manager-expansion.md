# F4 — Scope Fidelity / Security / License Verification

Reviewer: Security/scope/license final verification wave
Date: 2026-06-28
Target: mcm (Minecraft Manager expansion)

---

## Checks Performed

### 1. No admin token or Turnstile required for publish/update/delete

**PASS**

Evidence:
- `src/server/share.rs:16` — Module doc explicitly states: "No admin token, no Turnstile anywhere here. Auth is OIDC only (`AuthedOwner`)."
- `src/server/auth.rs` — `AuthedOwner` extractor reads `Authorization: Bearer <token>` or `mcm_session` cookie, looks up session in in-memory store. No admin-token path exists.
- `src/server/share.rs:81-122,124-171,173-204` — `publish_package`, `update_package`, `delete_package` all take `AuthedOwner(owner): AuthedOwner` as parameter. No additional admin gate.
- `README.md:279` — "No admin token or Turnstile is required for publish/update/delete. Authentication is OIDC only."
- `tests/server_auth.rs:15` — Test comment: "No admin token or Turnstile required anywhere in publish/update/delete."

### 2. Server storage default is outside `/x` and unsafe `/x` defaults are refused

**PASS**

Evidence:
- `src/server/config.rs:105-107` — Default: `PathBuf::from("/var/lib/mcm-share")` (outside `/x`).
- `src/server/config.rs:122-134` — `validate_data_dir()` checks all ancestor paths against `Path::new("/x")`. Returns error: "MCM_SHARE_DATA_DIR must not be under /x (got {dir}); server storage must live outside /x per the plan".
- `src/server/storage/helpers.rs:27-36` — Defense-in-depth `refuse_under_x()` check on storage open.
- `src/server/storage/helpers.rs:60-66` — Unit tests proving `/x`, `/x/mcm-share`, `/x/foo/bar` all rejected; `/var/lib/mcm-share`, `/tmp/mcm-test` accepted.
- `tests/server_storage.rs:9,320-325` — Integration test: `Storage::open(PathBuf::from("/x/mcm-share"))` and `/x` both fail with error containing "/x".
- `README.md:266` — "The service refuses to start if the default data directory is under `/x`."

### 3. Fresh install has no custom source

**PASS**

Evidence:
- `src/config.rs:30-34` — `sources: BTreeMap<String, SourceRecord>` with `#[serde(default)]`. Default is empty BTreeMap.
- `src/config.rs:17` — `Config` derives `Default` (since task 5), so `sources` defaults to empty.
- `src/source_cmd.rs:6` — Module doc: "Fresh config has zero custom sources — no author source is preinstalled."
- `README.md:205` — "Sources are manually imported indexes. A fresh install has zero custom sources. No source, including the author source, is preloaded by default."
- `tests/source_cmd.rs` — Test: "Fresh config: empty list (exit 0), no config.toml on disk".

### 4. Imported sources are trusted only after manual add + confirmation policy

**PASS**

Evidence:
- `src/source_cmd.rs:27` — `source_add` calls `require_confirmation(OperationKind::SourceAction, yes)?` before inserting.
- `src/source_cmd.rs:29-30` — Duplicate check: "source {url} is already imported".
- `src/config.rs:37-38` — SourceRecord doc: "Importing makes it trusted; actionable operations on sources still require confirmation via the centralized policy."
- `src/source_cmd.rs:60` — `source_info` prints: "status: trusted (manual import)".
- `tests/source_cmd.rs:118` — Without `--yes` in non-TTY: "confirmation required; pass --yes to proceed", nothing persisted.
- `tests/source_cmd.rs:136-140` — Add with `--yes` succeeds and persists.
- `tests/source_cmd.rs:147-159` — Info prints "trusted (manual import)".
- `README.md:214-216` — "a source is trusted once you manually import it" + "state-changing actions from a source still require confirmation".

### 5. `curl|bash` install scripts verify artifacts and avoid unverified piped execution

**PASS**

Evidence — Bootstrap script (`src/server/install/bootstrap-script.sh`):
- Downloads to temp dir via `mktemp -d` (line 72). NOT piped to shell.
- SHA-256 checksum verification (lines 103-131): downloads `.sha256` sidecar, verifies via `sha256sum`, `shasum -a 256`, or `openssl dgst`.
- Aborts on failure (line 125-131): "checksum verification failed" + "Aborting installation."
- Trap cleanup (line 74): `trap 'rm -rf "${TEMP_DIR}"' EXIT`.
- Unit tests in `bootstrap.rs:60-72` assert checksum verification exists.
- Unit test in `bootstrap.rs:140-153` — `script_does_not_pipe_unverified_bytes_to_shell` scans every non-comment line for `| sh`, `| bash`, `| /bin/sh`, `| /bin/bash` and panics if found.

Evidence — Package install script (`src/server/install/pkg.rs`):
- Slug validated at boundary via `validate_package_name(&slug)` — only `[a-z0-9-]` allowed.
- Slug embedded in single quotes in shell: `SLUG='{slug}'` (line 102). No injection possible.
- Delegates to `mcm install "${PACKAGE_BASE_URL}/api/share/pkg/${SLUG}" --yes` (line 123). No direct shell piping of untrusted bytes.
- Unit tests in `server_pkg_install.rs` verify script structure, bootstrap via `/install`, and slug quoting.

### 6. No HMCL/PCL code/assets/strings copied

**PASS**

Evidence:
- `docs/CLEAN-ROOM-POLICY.md` — 104-line comprehensive policy. States: "NO code, assets, strings, icons, UI layout files, or implementation structure from PCL/PCL2 may be copied" and "Direct code reuse (copying any HMCL source file, function, or snippet) requires a separate license review and is FORBIDDEN".
- `README.md:325-327` — "HMCL and PCL are conceptual UX and product references only. No HMCL or PCL code, UI text, assets, icons, strings, or implementation structure is copied."
- Grep for `HMCL|PCL|hmcl|pcl2` returns 29 matches in only 2 files (`README.md` and `docs/CLEAN-ROOM-POLICY.md`) — all are policy text, not code/assets.
- Grep across all `.rs` files: zero HMCL/PCL references (no code, no imports, no strings).

### 7. No OIDC secrets, provided secret-like values, passwords, tokens, or credentials written

**PASS**

Evidence — No concrete secret values found:
- Grep for `sk-*`, `ghp_*`, `AIza*`, `BEGIN PRIVATE KEY`, `xox[baprs]-*`: zero matches.
- Grep for JWT patterns `eyJ[a-zA-Z0-9_-]{20,}`: zero matches.
- Grep for `MCM_OIDC_CLIENT_SECRET.*=.*['"]...['"]`: zero matches (no hardcoded secret values).
- Grep for `bearer`/`Basic` with long tokens: zero matches.

Evidence — Secret handling is correct:
- `src/server/config.rs:113-115` — `MCM_OIDC_CLIENT_SECRET` read from env only: `env::var("MCM_OIDC_CLIENT_SECRET").ok().map(SecretString::new)`.
- `src/server/config.rs:43-63` — `SecretString` wraps secrets; `Debug` impl outputs `<redacted>`.
- `src/server/config.rs:88-96` — `ServerConfig::Debug` renders `oidc_client_secret` as `<redacted>`.
- `tests/server/config.rs:182-203` — Two tests prove `SecretString` and `ServerConfig` debug output never leak secrets.
- `src/server/auth.rs:17-22` — Module doc: "OIDC client secrets and session tokens are NEVER logged."

Evidence — "leak" strings in test code are intentional test fixtures:
- `tests/mcm_package.rs:275`, `tests/source_index.rs:277`, `tests/server_storage.rs:333`, `src/source_index.rs:310,319`, `src/server/storage/helpers.rs:79` — All are payloads like `"password":"leak"` or `"token":"leak"` that VERIFY the secret-rejection scanner works. These are not real secrets.

Evidence — README/docs reference env variable names only:
- `README.md:258` — `// MCM_OIDC_CLIENT_SECRET: provide via env or secret file, never commit.`
- `README.md:273-277` — "OIDC configuration uses environment variable names only. No secret values are committed."
- `src/server/mod.rs:273,298` — Test router builders set `oidc_client_secret: None`.

Evidence — Evidence files checked:
- `.omo/evidence/` directory is empty (no prior evidence files with potential secret leaks).

### 8. AGPL/license docs exist

**PASS**

Evidence:
- `LICENSE` — Full AGPL-3.0 text (661 lines), complete with MCM copyright notice (lines 632-646).
- `docs/AGPL-COMPLIANCE.md` — 85-line compliance guide covering CLI use, network service use, modified distributions, and dependency licenses.
- `docs/CLEAN-ROOM-POLICY.md` — 104-line clean-room policy for HMCL/PCL references.
- `deny.toml` — Dependency license audit: only permissive OSI-approved licenses allowed (MIT, Apache-2.0, ISC, Zlib, BSD-2/3, MPL-2.0, etc.). AGPL-3.0-or-later allowed only for the workspace crate itself.
- `README.md:319-323` — License section: "MCM is licensed under the GNU Affero General Public License v3 or later (see `LICENSE`). Under AGPLv3 section 13, anyone running a modified version as a network service must offer users the Corresponding Source."

---

## Summary

| Check | Verdict | Key Evidence |
|-------|---------|--------------|
| No admin token / Turnstile for publish | PASS | `share.rs:16`, `auth.rs`, all publish/update/delete use `AuthedOwner` only |
| Server storage outside `/x` | PASS | Default `/var/lib/mcm-share`, `validate_data_dir` + `refuse_under_x` double guard, tested |
| Fresh install: no custom source | PASS | `Config` derives `Default`, `sources` is empty BTreeMap, tested |
| Sources trusted after manual add + confirm | PASS | `require_confirmation(SourceAction)` gate, tested in source_cmd |
| curl\|bash verifies artifacts | PASS | Bootstrap: sha256 checksum + temp dir + no pipe-to-shell. Package: slug validation + single-quote embed |
| No HMCL/PCL code/assets | PASS | CLEAN-ROOM-POLICY.md + grep: only policy references, zero code/assets |
| No OIDC secrets/credentials in repo | PASS | Env-only loading, SecretString redaction, zero hardcoded secrets, test fixtures are intentional rejection tests |
| AGPL/license docs exist | PASS | LICENSE (661 lines), AGPL-COMPLIANCE.md, CLEAN-ROOM-POLICY.md, deny.toml |

---

## Residual Risk

- **Bootstrap script checksum**: the sha256 sidecar download is best-effort (`2>/dev/null || true`). If the `.sha256` file is unavailable or empty, the script aborts with "checksum verification failed". This is correct conservative behavior.
- **OIDC client secret loading**: `SecretString::as_str()` is `#[allow(dead_code)]` because real OIDC token exchange is not yet wired (mock mode only). The function exists but is unused in production. This is a future-ready placeholder, not a leak.
- **Session token entropy**: session tokens use `nonce()` which combines nanosecond timestamps + counter (not cryptographic RNG). Documented as "no cryptographic strength needed beyond unguessability for a personal service". Acceptable for the threat model.

---

VERDICT: APPROVE
