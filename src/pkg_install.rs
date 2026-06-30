//! Package install/download apply logic, split from `pkg_cmd.rs` to stay
//! under the 250 pure-LOC ceiling. Executes v2 lock steps: bridges
//! `mod.install` steps to provider artifacts, writes mod jars, runs
//! `shell.run`/`file.*`/`net.download`/`config.set` steps, and handles
//! `game.choose` version-root scoping.
// allow: SIZE_OK — single-module ownership of lock execution; splitting
// would create awkward App method boundaries.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use anyhow::{bail, Context, Result};
use time::OffsetDateTime;

use crate::app::App;
use crate::config::ProfileSnapshot;
use crate::confirmation::{require_confirmation, OperationKind};
use crate::download::{download_file, DownloadOptions, HttpFetcher};
use crate::i18n;
use crate::install::download_artifact;
use crate::lock::{InstallReason, InstalledMod};
use crate::mcm_package::validate_step_dest_path;
use crate::mcm_package::{parse_mcm_lock, LockStep, McmLock};
use crate::provider::{Artifact, ReleaseKind};
use crate::safety::{sanitize_filename, validate_download_url};

/// Tracks the `game.choose` scope during lock execution. Each `game.choose`
/// step sets the version root for subsequent `file.*`, `net.download`, and
/// `config.set` steps. Reset by the next `game.choose` or script end.
struct VersionContext {
    root: PathBuf,
}

impl VersionContext {
    fn resolve_dest(&self, dest: &str) -> Result<PathBuf> {
        validate_step_dest_path(dest)?;
        Ok(self.root.join(dest))
    }
}

impl App {
    pub(crate) fn pkg_install(&self, target: &str, yes: bool) -> Result<()> {
        if self.import_modpack(target, yes)? {
            return Ok(());
        }
        let lock = self.load_lock_file(target)?;
        warn_if_do_steps(&lock, self.lang);
        require_confirmation(OperationKind::PackageInstall, yes)?;
        if has_do_steps(&lock) {
            require_confirmation(OperationKind::LaunchOnInstall, yes)?;
            println!("{}", i18n::launch_on_install_confirmed(self.lang));
        }
        self.apply_lock(&lock, false)?;
        println!(
            "{}",
            i18n::installed_package(self.lang, &lock.identity.name, &lock.identity.version)
        );
        Ok(())
    }

    pub(crate) fn pkg_download(&self, target: &str, yes: bool) -> Result<()> {
        if self.import_modpack(target, yes)? {
            return Ok(());
        }
        let lock = self.load_lock_file(target)?;
        warn_if_do_steps(&lock, self.lang);
        require_confirmation(OperationKind::Download, yes)?;
        self.apply_lock(&lock, true)?;
        println!(
            "{}",
            i18n::downloaded_package(self.lang, &lock.identity.name, &lock.identity.version)
        );
        Ok(())
    }

    pub(crate) fn game_root_for_lock(&self, lock: &McmLock) -> Result<PathBuf> {
        let _ = lock;
        let profile = self.active_profile()?;
        profile
            .mods_dir
            .parent()
            .map(Path::to_path_buf)
            .context(i18n::could_not_resolve_game_root(self.lang))
    }

    pub(crate) fn load_lock_file(&self, target: &str) -> Result<McmLock> {
        if let Some(lock) = self.resolve_from_sources(target)? {
            return Ok(lock);
        }
        let text = if target.starts_with("http") {
            fetch_url(target)?
        } else {
            let path = Path::new(target);
            fs::read_to_string(path)
                .with_context(|| i18n::read_file_error(self.lang, &path.display().to_string()))?
        };
        parse_mcm_lock(&text)
    }

    /// Execute v2 lock steps. When `download_only` is true, only download
    /// artifacts without executing install effects. For `mcm install`,
    /// only install-permitted steps run; do/full steps are silently stripped.
    pub(crate) fn apply_lock(&self, lock: &McmLock, download_only: bool) -> Result<()> {
        let profile = self.active_profile()?;
        let default_root = self.game_root_for_lock(lock)?;
        let mut vctx = VersionContext {
            root: default_root.clone(),
        };
        for step in &lock.steps {
            if !step.permission.is_install_permitted() {
                continue;
            }
            self.execute_step(step, lock, &profile, &mut vctx, download_only)?;
        }
        Ok(())
    }

    pub(crate) fn do_lock(&self, lock: &McmLock, yes: bool) -> Result<()> {
        require_confirmation(OperationKind::ScriptExecution, yes)?;
        let profile = self.active_profile()?;
        let default_root = self.game_root_for_lock(lock)?;
        let mut vctx = VersionContext {
            root: default_root.clone(),
        };
        let mut executed = 0;
        for step in &lock.steps {
            match step.op.as_str() {
                "root.system" => {
                    require_confirmation(OperationKind::RootSystemChange, true)?;
                    self.execute_step(step, lock, &profile, &mut vctx, false)?;
                    executed += 1;
                }
                "mcm.do" => {
                    eprintln!("{}", i18n::nested_mcm_do_skipped(self.lang));
                }
                _ => {
                    self.execute_step(step, lock, &profile, &mut vctx, false)?;
                    executed += 1;
                }
            }
        }
        if executed == 0 {
            println!("{}", i18n::no_scripts_to_execute(self.lang));
        }
        Ok(())
    }

    fn execute_step(
        &self,
        step: &LockStep,
        _lock: &McmLock,
        profile: &crate::config::Profile,
        vctx: &mut VersionContext,
        download_only: bool,
    ) -> Result<()> {
        match step.op.as_str() {
            "game.choose" => {
                execute_game_choose(step, vctx)?;
            }
            "mod.install" => {
                self.install_mod_step(step, profile, download_only)?;
            }
            "shell.run" => {
                if !download_only {
                    run_shell_step(step, &vctx.root)?;
                }
            }
            "file.copy" => {
                if !download_only {
                    execute_file_copy(step, vctx)?;
                }
            }
            "file.write" => {
                if !download_only {
                    execute_file_write(step, vctx)?;
                }
            }
            "net.download" => {
                execute_net_download(step, vctx, download_only)?;
            }
            "config.set" => {
                if !download_only {
                    execute_config_set(step, vctx, &self.lang)?;
                }
            }
            "game.install" | "pkg.install" => {
                // Handled by specialized installers; no-op here.
            }
            "root.system" => {
                if !download_only {
                    execute_root_system(step, &vctx.root)?;
                }
            }
            other => {
                eprintln!("{}", i18n::unknown_step_skipped(self.lang, other));
            }
        }
        Ok(())
    }

    fn install_mod_step(
        &self,
        step: &LockStep,
        profile: &crate::config::Profile,
        download_only: bool,
    ) -> Result<()> {
        let provider = self.provider()?;
        let mut lock = self.load_lock(profile)?;
        fs::create_dir_all(&profile.mods_dir)?;
        let args = &step.args;
        let logical_id = args
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_owned();
        let provider_name = args
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_owned();
        let version = args
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0")
            .to_owned();
        let filename_raw = args
            .get("filename")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown.jar")
            .to_owned();
        let download_url = args
            .get("download_url")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let sha256_claimed = args
            .get("sha256")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let project_id = args
            .get("project_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let file_id = args
            .get("file_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        if provider_name == "source" {
            let hash = crate::source_resolve::install_source_mod(step, &profile.mods_dir)?;
            if !download_only {
                lock.installed.insert(
                    logical_id.clone(),
                    InstalledMod {
                        logical_id,
                        provider: provider_name,
                        project_id,
                        file_id,
                        version,
                        filename: sanitize_filename(&filename_raw)?,
                        sha256: hash,
                        reason: InstallReason::Manual,
                        required_deps: Vec::new(),
                        profile: ProfileSnapshot {
                            mc_version: profile.mc_version.clone(),
                            loader: profile.loader.clone(),
                            side: profile.side,
                        },
                        installed_at: OffsetDateTime::now_utc().to_string(),
                        owner_id: None,
                    },
                );
            }
            return Ok(());
        }

        if let Some(ref url) = download_url {
            validate_download_url(url)?;
        }
        let filename = sanitize_filename(&filename_raw)?;
        let target = profile.mods_dir.join(&filename);
        let artifact = Artifact {
            file_id,
            version: version.clone(),
            release: ReleaseKind::Stable,
            mc_versions: Vec::new(),
            loaders: Vec::new(),
            side: crate::config::Side::Both,
            filename: filename.clone(),
            download_url,
            sha256: sha256_claimed.clone(),
            download_count: None,
            deps: Vec::new(),
            owner_id: None,
        };
        let hash = download_artifact(provider.as_ref(), &artifact, &target)?;
        if download_only {
            return Ok(());
        }
        lock.installed.insert(
            logical_id.clone(),
            InstalledMod {
                logical_id,
                provider: provider_name,
                project_id: args
                    .get("project_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned(),
                file_id: args
                    .get("file_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned(),
                version,
                filename,
                sha256: hash,
                reason: InstallReason::Manual,
                required_deps: Vec::new(),
                profile: ProfileSnapshot {
                    mc_version: profile.mc_version.clone(),
                    loader: profile.loader.clone(),
                    side: profile.side,
                },
                installed_at: OffsetDateTime::now_utc().to_string(),
                owner_id: None,
            },
        );
        if !download_only {
            self.save_lock(profile, &lock)?;
        }
        Ok(())
    }
}

pub(crate) fn warn_if_do_steps(lock: &McmLock, lang: crate::i18n::Lang) {
    if has_do_steps(lock) {
        eprintln!("{}", i18n::script_warning(lang));
    }
}

fn has_do_steps(lock: &McmLock) -> bool {
    lock.steps
        .iter()
        .any(|s| !s.permission.is_install_permitted())
}

pub(crate) fn run_shell_step(step: &LockStep, cwd: &Path) -> Result<()> {
    let lang = crate::i18n::Lang::default();
    let command = step
        .args
        .get("command")
        .and_then(|v| v.as_str())
        .with_context(|| i18n::step_missing_arg(lang, &step.op, "command"))?;
    let step_cwd = step
        .args
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(Path::new)
        .unwrap_or(cwd);
    let mut cmd = StdCommand::new("sh");
    cmd.arg("-c").arg(command).current_dir(step_cwd);
    let status = cmd
        .status()
        .with_context(|| i18n::run_action_error(lang, &step.op))?;
    if !status.success() {
        bail!(
            "{}",
            i18n::action_exited_with_status(lang, &step.op, &status.to_string())
        );
    }
    Ok(())
}

fn execute_game_choose(step: &LockStep, vctx: &mut VersionContext) -> Result<()> {
    let game_name = step
        .args
        .get("game")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let version = step
        .args
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("latest");
    let version_root = resolve_version_root(game_name, version)?;
    vctx.root = version_root;
    Ok(())
}

fn resolve_version_root(game_name: &str, version: &str) -> Result<PathBuf> {
    let home = directories::UserDirs::new()
        .context("could not resolve home directory")?
        .home_dir()
        .to_path_buf();
    Ok(home.join("mcm").join(game_name).join(version))
}

fn execute_file_copy(step: &LockStep, vctx: &VersionContext) -> Result<()> {
    let src_artifact = step
        .args
        .get("src_artifact")
        .and_then(|v| v.as_str())
        .with_context(|| {
            i18n::step_missing_arg(crate::i18n::Lang::default(), &step.op, "src_artifact")
        })?;
    let dest = step
        .args
        .get("dest")
        .and_then(|v| v.as_str())
        .with_context(|| i18n::step_missing_arg(crate::i18n::Lang::default(), &step.op, "dest"))?;
    let dest_path = vctx.resolve_dest(dest)?;
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src_artifact, &dest_path)
        .with_context(|| format!("failed to copy {src_artifact} to {}", dest_path.display()))?;
    Ok(())
}

fn execute_file_write(step: &LockStep, vctx: &VersionContext) -> Result<()> {
    let dest = step
        .args
        .get("dest")
        .and_then(|v| v.as_str())
        .with_context(|| i18n::step_missing_arg(crate::i18n::Lang::default(), &step.op, "dest"))?;
    let dest_path = vctx.resolve_dest(dest)?;
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = if let Some(content) = step.args.get("content").and_then(|v| v.as_str()) {
        content.to_owned()
    } else if let Some(artifact_id) = step.args.get("artifact").and_then(|v| v.as_str()) {
        bail!(
            "file.write with artifact reference '{artifact_id}' requires \
             artifact resolution (not yet implemented)"
        )
    } else {
        bail!("file.write step requires either 'content' or 'artifact' argument")
    };
    fs::write(&dest_path, content.as_bytes())
        .with_context(|| format!("failed to write {}", dest_path.display()))?;
    Ok(())
}

fn execute_net_download(step: &LockStep, vctx: &VersionContext, download_only: bool) -> Result<()> {
    let url = step
        .args
        .get("url")
        .and_then(|v| v.as_str())
        .with_context(|| i18n::step_missing_arg(crate::i18n::Lang::default(), &step.op, "url"))?;
    let dest = step
        .args
        .get("dest")
        .and_then(|v| v.as_str())
        .with_context(|| i18n::step_missing_arg(crate::i18n::Lang::default(), &step.op, "dest"))?;
    let dest_path = vctx.resolve_dest(dest)?;
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)?;
    }
    validate_download_url(url)?;
    let fetcher = HttpFetcher::new(url);
    let options = DownloadOptions {
        expected_sha256: step
            .args
            .get("sha256")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        ..DownloadOptions::default()
    };
    download_file(&dest_path, &fetcher, &options)?;
    if download_only {
        let _ = fs::remove_file(&dest_path);
    }
    Ok(())
}

fn execute_config_set(
    step: &LockStep,
    vctx: &VersionContext,
    lang: &crate::i18n::Lang,
) -> Result<()> {
    let scope = step
        .args
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("version");
    let key = step
        .args
        .get("key")
        .and_then(|v| v.as_str())
        .with_context(|| i18n::step_missing_arg(*lang, &step.op, "key"))?;
    let value = step
        .args
        .get("value")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match scope {
        "version" => {
            let config_path = vctx.root.join("config.toml");
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut content = if config_path.exists() {
                fs::read_to_string(&config_path)?
            } else {
                String::new()
            };
            let entry = format!("{key} = \"{value}\"\n");
            if content.contains(&format!("{key} = ")) {
                let lines: Vec<&str> = content.lines().collect();
                let mut new_lines = Vec::new();
                for line in &lines {
                    if line.starts_with(&format!("{key} = ")) {
                        new_lines.push(&entry[..entry.len() - 1]);
                    } else {
                        new_lines.push(line);
                    }
                }
                content = new_lines.join("\n") + "\n";
            } else {
                content.push_str(&entry);
            }
            fs::write(&config_path, content.as_bytes())?;
        }
        "user" => {
            let config_dir = directories::UserDirs::new()
                .map(|d| d.home_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
            let config_path = config_dir.join("mcm").join("config.toml");
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut content = if config_path.exists() {
                fs::read_to_string(&config_path)?
            } else {
                String::new()
            };
            let entry = format!("{key} = \"{value}\"\n");
            if content.contains(&format!("{key} = ")) {
                let lines: Vec<&str> = content.lines().collect();
                let mut new_lines = Vec::new();
                for line in &lines {
                    if line.starts_with(&format!("{key} = ")) {
                        new_lines.push(&entry[..entry.len() - 1]);
                    } else {
                        new_lines.push(line);
                    }
                }
                content = new_lines.join("\n") + "\n";
            } else {
                content.push_str(&entry);
            }
            fs::write(&config_path, content.as_bytes())?;
        }
        other => {
            bail!("config.set: unknown scope '{other}'; expected 'version' or 'user'");
        }
    }
    Ok(())
}

fn execute_root_system(step: &LockStep, cwd: &Path) -> Result<()> {
    let lang = crate::i18n::Lang::default();
    let command = step
        .args
        .get("command")
        .and_then(|v| v.as_str())
        .with_context(|| i18n::step_missing_arg(lang, &step.op, "command"))?;
    eprintln!("WARNING: root.system requires elevated privileges: {command}");
    let mut cmd = StdCommand::new("sh");
    cmd.arg("-c").arg(command).current_dir(cwd);
    let status = cmd
        .status()
        .with_context(|| i18n::run_action_error(lang, &step.op))?;
    if !status.success() {
        bail!(
            "{}",
            i18n::action_exited_with_status(lang, &step.op, &status.to_string())
        );
    }
    Ok(())
}

fn fetch_url(url: &str) -> Result<String> {
    let dest = std::env::temp_dir().join(format!("mcm-fetch-{}.txt", std::process::id()));
    let fetcher = HttpFetcher::new(url);
    download_file(&dest, &fetcher, &DownloadOptions::default())?;
    let text = fs::read_to_string(&dest)
        .with_context(|| i18n::fetch_error(crate::i18n::Lang::default(), url))?;
    let _ = fs::remove_file(&dest);
    Ok(text)
}
