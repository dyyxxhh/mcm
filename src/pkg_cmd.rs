use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::app::App;
use crate::cli::{MakeFormat, PkgCommand};
use crate::confirmation::{require_confirmation, OperationKind};
use crate::dyyl_host;
use crate::i18n;
use crate::mcm_package::{
    lock_to_dyyl, new_lock, new_step, parse_mcm_lock, LockStep, McmLock, StepPermission,
};
use crate::share_client::{
    install_command_snippet, parse_package_json_to_value, resolve_slug, ShareClient,
};

impl App {
    pub(crate) fn pkg(&self, command: PkgCommand) -> Result<()> {
        match command {
            PkgCommand::Info { path } => self.pkg_info(&path),
            PkgCommand::Install {
                target,
                server,
                yes,
            } => self.pkg_install_with_server(&target, server.as_deref(), yes),
            PkgCommand::Download {
                target,
                server,
                output,
                yes,
            }
            | PkgCommand::Dl {
                target,
                server,
                output,
                yes,
            } => self.pkg_download_with_server(&target, server.as_deref(), output.as_deref(), yes),
            PkgCommand::Make { yes: _, format } => self.pkg_make(format),
            PkgCommand::Share {
                target,
                server,
                yes,
            } => self.pkg_share(&target, &server, yes),
            PkgCommand::List { server, mine } => self.pkg_list_with_server(server.as_deref(), mine),
            PkgCommand::Update {
                slug,
                file,
                server,
                yes,
            } => self.pkg_update(&slug, &file, &server, yes),
            PkgCommand::Delete { slug, server, yes } => self.pkg_delete(&slug, &server, yes),
            PkgCommand::Auth { command } => crate::pkg_auth::pkg_auth_impl(self, command),
        }
    }

    pub(crate) fn top_install(&self, target: Option<String>, yes: bool) -> Result<()> {
        let resolved = match target {
            Some(target) => {
                if target.starts_with("mc") && crate::mc_target::parse_mc_target(&target).is_ok() {
                    bail!("{}", i18n::top_install_smart_target_error(self.lang));
                }
                let lower = target.to_ascii_lowercase();
                let is_modpack = lower.ends_with(".mcm")
                    || lower.ends_with(".mrpack")
                    || lower.ends_with(".zip");
                if !is_modpack && !target.starts_with("http") {
                    bail!("{}", i18n::top_install_accepts_only(self.lang));
                }
                target
            }
            None => {
                let auto = find_single_mcm(Path::new("."), self.lang)?;
                auto.to_string_lossy().into_owned()
            }
        };
        self.pkg_install(&resolved, yes)
    }

    pub(crate) fn do_file(&self, file: Option<PathBuf>, yes: bool) -> Result<()> {
        let path = match file {
            Some(p) => p,
            None => find_single_mcm(Path::new("."), self.lang)?,
        };
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext == "dyyl" {
            // dyyl source: build to a temp .mcm, then execute.
            // When dyyl is on PATH this runs the real NDJSON host protocol;
            // otherwise the built-in text parser is used.
            let tmp = tempfile::tempdir().context("create temp dir for build")?;
            let out = tmp.path().join("build.mcm");
            self.build_dyyl(&path, Some(&out))?;
            let text = fs::read_to_string(&out)
                .with_context(|| format!("read built lock: {}", out.display()))?;
            let lock = parse_mcm_lock(&text)?;
            return self.do_lock(&lock, yes);
        }
        let text = fs::read_to_string(&path)
            .with_context(|| i18n::read_file_error(self.lang, &path.display().to_string()))?;
        let lock = parse_mcm_lock(&text)?;
        self.do_lock(&lock, yes)
    }

    /// `mcm build <in.dyyl> [-o <out.mcm>]`: run dyyl with host protocol
    /// and write v2 JSON lock.
    ///
    /// When the `dyyl` interpreter is on PATH, mcm spawns it with
    /// `--host-json` and collects the full NDJSON command stream — the
    /// real host protocol. When dyyl is absent, mcm falls back to the
    /// built-in text parser so builds keep working in minimal envs.
    pub(crate) fn build_dyyl(&self, input: &Path, output: Option<&Path>) -> Result<()> {
        let out_path = output.map(PathBuf::from).unwrap_or_else(|| {
            let mut p = input.to_path_buf();
            p.set_extension("mcm");
            p
        });

        let lock = if dyyl_host::dyyl_available() {
            // Real host protocol: spawn dyyl --host-json, collect commands.
            let commands = dyyl_host::run_dyyl_host(input)?;
            dyyl_host::commands_to_lock(&commands)?
        } else {
            // Fallback: built-in text parser (no dyyl binary).
            let text = fs::read_to_string(input)
                .with_context(|| i18n::read_file_error(self.lang, &input.display().to_string()))?;
            parse_dyyl_to_lock(&text)?
        };

        let json = serde_json::to_string_pretty(&lock)?;
        crate::util::atomic_write(&out_path, json.as_bytes())?;
        println!(
            "{}",
            i18n::build_success(self.lang, &out_path.display().to_string())
        );
        Ok(())
    }

    /// `mcm make <out.dyyl>`: export current instance state as dyyl source.
    pub(crate) fn make_dyyl(&self, output: &Path) -> Result<()> {
        let profile = self.active_profile()?;
        let lock = self.load_lock(&profile)?;
        // Build a minimal v2 lock from the current instance state.
        let mcm_lock = instance_state_to_lock(&profile, &lock)?;
        let dyyl_text = lock_to_dyyl(&mcm_lock);
        crate::util::atomic_write(output, dyyl_text.as_bytes())?;
        println!(
            "{}",
            i18n::make_success(self.lang, &output.display().to_string())
        );
        Ok(())
    }

    fn pkg_make(&self, format: MakeFormat) -> Result<()> {
        match format {
            MakeFormat::Mcm => {
                // `mcm pkg make` without format flag: output v2 lock JSON.
                let profile = self.active_profile()?;
                let lock = self.load_lock(&profile)?;
                let mcm_lock = instance_state_to_lock(&profile, &lock)?;
                println!("{}", serde_json::to_string_pretty(&mcm_lock)?);
                Ok(())
            }
            MakeFormat::Mrpack => {
                let profile = self.active_profile()?;
                let name = format!("{}.mrpack", profile.name);
                let output = Path::new(&name);
                self.export_mrpack(output)
            }
            MakeFormat::Curseforge => Err(anyhow::anyhow!(
                i18n::curseforge_export_not_implemented(self.lang)
            )),
        }
    }

    fn pkg_share(&self, target: &str, server: &str, yes: bool) -> Result<()> {
        require_confirmation(OperationKind::PackageInstall, yes)?;
        let lock = self.load_lock_file(target)?;
        let content = serde_json::to_value(&lock)?;
        let slug = lock.identity.name.clone();
        let version = lock.identity.version.clone();

        let client = ShareClient::new(server)?;
        let token = self.get_or_login_token(&client)?;
        let published_slug = client.publish(&token, &slug, &version, &content)?;
        println!("{}", i18n::package_published(self.lang, &published_slug));
        let cmd = install_command_snippet(&published_slug);
        println!();
        println!("Install command:");
        println!("  {cmd}");
        Ok(())
    }

    fn pkg_update(&self, slug: &str, file: &str, server: &str, yes: bool) -> Result<()> {
        require_confirmation(OperationKind::PackageInstall, yes)?;
        let lock = self.load_lock_file(file)?;
        let content = parse_package_json_to_value(&fs::read_to_string(file)?)?;
        let version = lock.identity.version.clone();

        let client = ShareClient::new(server)?;
        let token = self.get_or_login_token(&client)?;
        client.update(&token, slug, &version, &content)?;
        println!("{}", i18n::package_updated(self.lang, slug));
        let cmd = install_command_snippet(slug);
        println!();
        println!("Install command:");
        println!("  {cmd}");
        Ok(())
    }

    fn pkg_delete(&self, slug: &str, server: &str, yes: bool) -> Result<()> {
        require_confirmation(OperationKind::Download, yes)?;
        let client = ShareClient::new(server)?;
        let token = self.get_or_login_token(&client)?;
        client.delete(&token, slug)?;
        println!("{}", i18n::package_deleted(self.lang, slug));
        Ok(())
    }

    fn pkg_list_with_server(&self, server: Option<&str>, mine: bool) -> Result<()> {
        match server {
            Some(url) => {
                let client = ShareClient::new(url)?;
                if mine {
                    let token = self.get_or_login_token(&client)?;
                    let packages = client.list_mine(&token)?;
                    print_package_list(&packages);
                } else {
                    let packages = client.list()?;
                    print_package_list(&packages);
                }
                Ok(())
            }
            None => self.pkg_list(),
        }
    }

    fn pkg_download_with_server(
        &self,
        target: &str,
        server: Option<&str>,
        output: Option<&Path>,
        yes: bool,
    ) -> Result<()> {
        if let Some(url) = server {
            let slug = resolve_slug(target);
            let client = ShareClient::new(url)?;
            match output {
                Some(path) => {
                    let json = client.download_to_file(slug, path)?;
                    let lock = parse_mcm_lock(&json)?;
                    println!(
                        "{}",
                        i18n::downloaded_package(
                            self.lang,
                            &lock.identity.name,
                            &lock.identity.version
                        )
                    );
                }
                None => {
                    let json = client.download(slug)?;
                    let lock = parse_mcm_lock(&json)?;
                    println!(
                        "{}",
                        i18n::downloaded_package(
                            self.lang,
                            &lock.identity.name,
                            &lock.identity.version
                        )
                    );
                    println!("{json}");
                }
            }
            Ok(())
        } else {
            self.pkg_download(target, yes)
        }
    }

    fn pkg_install_with_server(&self, target: &str, server: Option<&str>, yes: bool) -> Result<()> {
        if let Some(url) = server {
            let slug = resolve_slug(target);
            let client = ShareClient::new(url)?;
            let json = client.download(slug)?;
            let lock = parse_mcm_lock(&json)?;
            require_confirmation(OperationKind::PackageInstall, yes)?;
            self.apply_lock(&lock, false)?;
            println!(
                "{}",
                i18n::installed_package(self.lang, &lock.identity.name, &lock.identity.version)
            );
            Ok(())
        } else {
            self.pkg_install(target, yes)
        }
    }

    fn pkg_list(&self) -> Result<()> {
        let config = self.load_config()?;
        let mut names: BTreeSet<String> = BTreeSet::new();
        for profile in config.profiles.values() {
            if let Ok(lock) = self.load_lock(profile) {
                for m in lock.installed.values() {
                    names.insert(format!("{} {}", m.logical_id, m.version));
                }
            }
        }
        for name in &names {
            println!("{name}");
        }
        Ok(())
    }

    fn get_or_login_token(&self, client: &ShareClient) -> Result<String> {
        client.oidc_login(self.lang)
    }
}

fn print_package_list(packages: &[crate::share_client::PackageEntry]) {
    if packages.is_empty() {
        println!("No packages found.");
        return;
    }
    for pkg in packages {
        let desc = pkg.description.as_deref().unwrap_or("(no description)");
        let owner = pkg.owner.as_deref().unwrap_or("unknown");
        let name_display = if pkg.name == pkg.slug {
            String::new()
        } else {
            format!(" ({})", pkg.name)
        };
        println!(
            "{}{} v{} by {} - {}",
            pkg.slug, name_display, pkg.version, owner, desc
        );
    }
}

fn find_single_mcm(dir: &Path, lang: crate::i18n::Lang) -> Result<PathBuf> {
    let mut entries: Vec<PathBuf> = Vec::new();
    for entry in
        fs::read_dir(dir).with_context(|| i18n::read_dir_error(lang, &dir.display().to_string()))?
    {
        let path = entry?.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext == "mcm" || ext == "dyyl" {
            entries.push(path);
        }
    }
    if entries.is_empty() {
        bail!("{}", i18n::no_mcm_file_found(lang));
    }
    entries.sort();
    Ok(entries.remove(0))
}

/// Parse dyyl source text and build a v2 lock.
///
/// This is a simplified parser that extracts `mcm.*` commands from dyyl
/// source. The full implementation would spawn dyyl with `--host-json`
/// and collect the streaming protocol events.
fn parse_dyyl_to_lock(text: &str) -> Result<McmLock> {
    let mut lock = new_lock("dyyl-build", "1.0.0");
    let mut steps: Vec<LockStep> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        // Skip comments and empty lines.
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        // Extract mcm.* command calls: `mcm.game.choose("dev", "1.20.1");`
        if let Some(cmd) = trimmed.strip_suffix(';') {
            let cmd = cmd.trim();
            if let Some(args_str) = cmd.strip_prefix("mcm.") {
                // Parse `op(args)` format.
                if let Some(paren_start) = args_str.find('(') {
                    let op = &args_str[..paren_start];
                    let args_inner = &args_str[paren_start + 1..].trim_end_matches(')');
                    let (permission, step_op) = classify_mcm_op(op);
                    let args = parse_dyyl_args(args_inner);
                    let step = new_step(&step_op, permission, args);
                    steps.push(step);
                }
            }
        }
    }
    lock.steps = steps;
    Ok(lock)
}

/// Classify a dyyl mcm command into step op and permission.
fn classify_mcm_op(dyyl_op: &str) -> (StepPermission, String) {
    match dyyl_op {
        "game.choose" | "game.install" | "mod.install" | "pkg.install" | "file.copy"
        | "file.write" | "net.download" | "config.set" => {
            (StepPermission::Install, dyyl_op.to_owned())
        }
        "shell.run" | "do" => (StepPermission::Do, dyyl_op.to_owned()),
        "root.system" => (StepPermission::Full, dyyl_op.to_owned()),
        _ => (StepPermission::Install, format!("mcm.{dyyl_op}")),
    }
}

/// Parse dyyl function arguments into a serde_json::Value.
fn parse_dyyl_args(args_str: &str) -> serde_json::Value {
    if args_str.is_empty() {
        return serde_json::Value::Null;
    }
    // Simple key:value or positional arg parsing.
    let mut map = serde_json::Map::new();
    let parts: Vec<&str> = args_str.split(',').collect();
    for (i, part) in parts.iter().enumerate() {
        let part = part.trim();
        if let Some((key, val)) = part.split_once(':') {
            let key = key.trim().to_owned();
            let val = parse_dyyl_value(val.trim());
            map.insert(key, val);
        } else {
            map.insert(i.to_string(), parse_dyyl_value(part));
        }
    }
    serde_json::Value::Object(map)
}

fn parse_dyyl_value(s: &str) -> serde_json::Value {
    if s.starts_with('"') && s.ends_with('"') {
        serde_json::Value::String(s[1..s.len() - 1].to_owned())
    } else {
        serde_json::Value::String(s.to_owned())
    }
}

/// Convert the current instance state (profile + lock) to a v2 McmLock.
fn instance_state_to_lock(
    profile: &crate::config::Profile,
    lock: &crate::lock::LockState,
) -> Result<McmLock> {
    let mut mcm_lock = new_lock(&profile.name, "1.0.0");
    mcm_lock.game = Some(crate::mcm_package::LockGame {
        game: None,
        version: Some(profile.mc_version.clone()),
        loader: Some(profile.loader.clone()),
    });
    mcm_lock.identity.description = Some(format!("Exported from game instance: {}", profile.name));

    // Add a game.choose step.
    mcm_lock.steps.push(new_step(
        "game.choose",
        StepPermission::Install,
        serde_json::json!({
            "game": profile.name,
            "version": profile.mc_version,
        }),
    ));

    // Add mod.install steps for each installed mod.
    for m in lock.installed.values() {
        mcm_lock.steps.push(new_step(
            "mod.install",
            StepPermission::Install,
            serde_json::json!({
                "id": m.logical_id,
                "provider": m.provider,
                "project_id": m.project_id,
                "file_id": m.file_id,
                "version": m.version,
                "filename": m.filename,
                "sha256": m.sha256,
            }),
        ));
    }

    Ok(mcm_lock)
}
