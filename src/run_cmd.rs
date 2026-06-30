//! `run` command implementation on [`App`].
//!
//! Implements `mcm run [--dry-run]` — the game launch command.
//! The dry-run mode prints the assembled launch command without executing it.
//! Real launch spawns Java and propagates the exit code.

use anyhow::{bail, Context, Result};

use crate::app::App;
use crate::i18n;
use crate::launch::build_launch_command;

impl App {
    pub(crate) fn run_cmd(&self, dry_run: bool) -> Result<()> {
        let mut config = self.load_config()?;

        let default_game = config
            .default_game
            .as_deref()
            .map(|s| s.to_owned())
            .or_else(|| config.games.keys().next().map(|s| s.to_owned()))
            .with_context(|| i18n::no_default_game_and_no_games(self.lang))?;

        // Clone so we don't hold an immutable borrow of `config` while
        // `build_launch_command` mutably borrows `config.launch_auth`
        // (for in-place token refresh).
        let game = config
            .games
            .get(&default_game)
            .with_context(|| i18n::default_game_does_not_exist(self.lang, &default_game))?
            .clone();
        let global_root = config.global.root_dir.clone();

        let was_online = matches!(
            config.launch_auth.mode,
            crate::auth::LaunchAuthMode::Online
        );
        let prev_token = config
            .launch_auth
            .online
            .as_ref()
            .map(|a| a.access_token.clone());

        let command = build_launch_command(
            &default_game,
            &game,
            &global_root,
            &mut config.launch_auth,
        )?;

        // If a Microsoft token was refreshed in-place during launch, persist
        // the updated OnlineAccount so subsequent launches skip the refresh.
        if was_online {
            let new_token = config
                .launch_auth
                .online
                .as_ref()
                .map(|a| a.access_token.clone());
            if new_token.as_deref() != prev_token.as_deref() {
                self.save_config(&config).context("persist refreshed Microsoft token")?;
            }
        }

        if dry_run {
            println!("{}", command.render());
            Ok(())
        } else {
            self.spawn_game(&command)
        }
    }

    /// Spawn the Java process and wait for it to exit.
    fn spawn_game(&self, cmd: &crate::launch::LaunchCommand) -> Result<()> {
        use std::process::Command;

        let mut builder = Command::new(&cmd.java_path);
        builder.args(&cmd.jvm_args);
        if !cmd.classpath.is_empty() {
            let cp_string = cmd
                .classpath
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(":");
            builder.arg("-cp").arg(&cp_string);
        }
        let mut child = builder
            .arg(&cmd.main_class)
            .args(&cmd.game_args)
            .current_dir(&cmd.game_dir)
            .spawn()
            .with_context(|| {
                format!(
                    "failed to start Java at {}; check that the path is correct and Java is installed",
                    cmd.java_path.display()
                )
            })?;

        let status = child.wait().with_context(|| "wait for Java process")?;
        let code = status.code().unwrap_or(1);
        if code != 0 {
            bail!("game exited with code {code}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::app::App;
    use crate::auth::{LaunchAuthMode, OnlineAccount};
    use crate::cli::ProviderChoice;
    use crate::config::LaunchAuthConfig;
    use crate::game_model::GameConfig;
    use crate::i18n::Lang;
    use crate::launch::build_launch_command;

    fn create_fake_java_with_arg_log(dir: &std::path::Path, arg_log: &std::path::Path) {
        let java_path = dir.join("java");
        std::fs::write(
            &java_path,
            format!(
                "#!/bin/bash\n\
                 for arg in \"$@\"; do\n\
                 \techo \"$arg\" >> \"{}\"\n\
                 done\n",
                arg_log.display()
            ),
        )
        .expect("write fake java");
        make_executable(&java_path);
    }

    fn create_fake_java_exit_code(dir: &std::path::Path, exit_code: u8) {
        let java_path = dir.join("java");
        std::fs::write(&java_path, format!("#!/bin/bash\nexit {exit_code}\n"))
            .expect("write fake java");
        make_executable(&java_path);
    }

    fn create_fake_java_cwd_logger(dir: &std::path::Path, cwd_log: &std::path::Path) {
        let java_path = dir.join("java");
        std::fs::write(
            &java_path,
            format!("#!/bin/bash\npwd >> \"{}\"\n", cwd_log.display()),
        )
        .expect("write fake java");
        make_executable(&java_path);
    }

    #[cfg(unix)]
    fn make_executable(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        let p = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(path, p).expect("chmod");
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &std::path::Path) {}

    fn build_fixture_game(
        tmp: &tempfile::TempDir,
        name: &str,
        mc_version: &str,
        loader: Option<(&str, &str)>,
    ) -> (
        crate::game_model::GameRecord,
        std::path::PathBuf,
        std::path::PathBuf,
    ) {
        let root = tmp.path().join(name);
        let resolved_version_id = match loader {
            Some((ln, lv)) => format!("{mc_version}-{ln}-{lv}"),
            None => mc_version.to_owned(),
        };
        let version_dir = root.join("versions").join(&resolved_version_id);
        std::fs::create_dir_all(&version_dir).expect("create version dir");

        let managed_dir = tmp
            .path()
            .join("runtimes")
            .join("java")
            .join("java21")
            .join("bin");
        std::fs::create_dir_all(&managed_dir).expect("create managed java dir");
        let java_path = managed_dir.join("java");
        std::fs::write(&java_path, b"mock java").expect("write mock java");
        std::fs::write(managed_dir.join("java.version"), "21\n").expect("write marker");

        let vj = serde_json::json!({
            "id": resolved_version_id,
            "mainClass": "net.minecraft.client.main.Main",
            "assets": "12",
            "assetIndex": {
                "id": "12",
                "sha1": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "size": 456789,
                "totalSize": 1234567,
                "url": "https://launchermeta.mojang.com/v1/packages/12/index.json"
            },
            "libraries": [
                {
                    "name": format!("net.minecraft:client:{mc_version}"),
                    "downloads": {
                        "artifact": {
                            "path": format!("net/minecraft/client/{mc_version}/client-{mc_version}.jar"),
                            "sha1": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
                            "size": 25000000,
                            "url": "https://libraries.minecraft.net/test.jar"
                        }
                    }
                },
                {
                    "name": "org.lwjgl:lwjgl:3.3.3",
                    "rules": [{"action": "allow", "os": {"name": "linux"}}],
                    "downloads": {
                        "artifact": {
                            "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar",
                            "sha1": "dddddddddddddddddddddddddddddddddddddddd",
                            "size": 800000,
                            "url": "https://libraries.minecraft.net/lwjgl.jar"
                        }
                    }
                }
            ],
            "arguments": {
                "jvm": [
                    "-Djava.library.path=${natives_directory}",
                    "-Dminecraft.launcher.brand=${launcher_name}",
                    "-Dminecraft.launcher.version=${launcher_version}",
                    "-cp", "${classpath}"
                ],
                "game": [
                    "--username", "${auth_player_name}",
                    "--uuid", "${auth_uuid}",
                    "--accessToken", "${auth_access_token}",
                    "--userType", "${auth_user_type}",
                    "--version", "${version_name}",
                    "--gameDir", "${game_directory}",
                    "--assetsDir", "${assets_root}",
                    "--versionType", "${version_type}"
                ]
            }
        });
        let json_path = version_dir.join(format!("{resolved_version_id}.json"));
        std::fs::write(&json_path, serde_json::to_string_pretty(&vj).unwrap())
            .expect("write version json");

        let game_jar = version_dir.join(format!("{resolved_version_id}.jar"));
        std::fs::write(&game_jar, b"mock game jar").expect("write game jar");

        if let Some((loader_name, loader_ver)) = loader {
            let loader_jar = version_dir.join(format!("{loader_name}-{loader_ver}.jar"));
            std::fs::write(&loader_jar, b"mock loader jar").expect("write loader jar");
        }

        let game = crate::game_model::GameRecord {
            name: name.to_owned(),
            root_dir: root,
            mc_version: Some(mc_version.to_owned()),
            loader: loader.map(|(n, _)| n.to_owned()),
            loader_version: loader.map(|(_, v)| v.to_owned()),
            resolved_version_id: Some(resolved_version_id),
            version_config: GameConfig::default(),
        };
        (game, tmp.path().to_path_buf(), java_path)
    }

    fn managed_dir(tmp: &tempfile::TempDir) -> std::path::PathBuf {
        tmp.path()
            .join("runtimes")
            .join("java")
            .join("java21")
            .join("bin")
    }

    fn setup_app(
        tmp: &tempfile::TempDir,
        game: &crate::game_model::GameRecord,
        default_game: Option<&str>,
        global_root: &std::path::Path,
        auth: Option<LaunchAuthConfig>,
    ) -> App {
        let config_dir = tmp.path().join("config");
        let state_dir = tmp.path().join("state");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        std::fs::create_dir_all(&state_dir).expect("create state dir");

        let config = crate::config::Config {
            default_game: default_game.map(String::from),
            games: [(game.name.clone(), game.clone())].into(),
            global: crate::game_model::GlobalConfig {
                root_dir: global_root.to_path_buf(),
            },
            launch_auth: auth.unwrap_or_default(),
            ..Default::default()
        };
        let toml = toml::to_string_pretty(&config).expect("serialize config");
        std::fs::write(config_dir.join("config.toml"), toml).expect("write config");

        App {
            config_dir,
            state_dir,
            provider_choice: ProviderChoice::Mock,
            lang: Lang::default(),
        }
    }

    #[test]
    fn dry_run_via_render_contains_java_path_jvm_main_class_and_auth() {
        let tmp = tempfile::tempdir().expect("tmp");
        let (game, global_root, _) = build_fixture_game(&tmp, "dev", "1.21.1", None);
        let mut auth = LaunchAuthConfig::default();
        let cmd = build_launch_command("dev", &game, &global_root, &mut auth)
            .expect("build launch command");

        let rendered = cmd.render();

        assert!(rendered.contains("/java"), "java path: {rendered}");
        assert!(
            rendered.contains("-Djava.library.path="),
            "natives path: {rendered}"
        );
        assert!(
            rendered.contains("net.minecraft.client.main.Main"),
            "main class: {rendered}"
        );
        assert!(rendered.contains("--username"), "auth username: {rendered}");
        assert!(rendered.contains("--uuid"), "auth uuid: {rendered}");
        assert!(rendered.contains("--gameDir"), "game dir: {rendered}");
        assert!(rendered.contains("--assetsDir"), "assets dir: {rendered}");
    }

    #[test]
    fn dry_run_with_fabric_loader_includes_loader_jar_in_classpath() {
        let tmp = tempfile::tempdir().expect("tmp");
        let (game, global_root, _) =
            build_fixture_game(&tmp, "dev", "1.21.1", Some(("fabric", "0.16.0")));
        let mut auth = LaunchAuthConfig::default();
        let cmd = build_launch_command("dev", &game, &global_root, &mut auth)
            .expect("build launch command");

        assert!(
            cmd.classpath
                .iter()
                .any(|p| p.to_string_lossy().contains("fabric-0.16.0.jar")),
            "classpath should contain fabric loader jar"
        );
        assert_eq!(cmd.loader.as_deref(), Some("fabric"));
        assert_eq!(cmd.loader_version.as_deref(), Some("0.16.0"));
    }

    #[test]
    fn spawn_game_succeeds_and_records_argv() {
        let tmp = tempfile::tempdir().expect("tmp");
        let arg_log = tmp.path().join("argv.log");

        let (game, global_root, _) = build_fixture_game(&tmp, "dev", "1.21.1", None);
        let mut auth = LaunchAuthConfig::default();
        let cmd = build_launch_command("dev", &game, &global_root, &mut auth)
            .expect("build launch command");

        create_fake_java_with_arg_log(&managed_dir(&tmp), &arg_log);

        let app = setup_app(&tmp, &game, Some("dev"), &global_root, None);
        let result = app.spawn_game(&cmd);
        assert!(result.is_ok(), "spawn should succeed: {result:?}");

        let logged_args = std::fs::read_to_string(&arg_log).expect("read arg log");
        let lines: Vec<&str> = logged_args.lines().collect();

        assert!(!lines.is_empty(), "fake java should have recorded args");
        assert!(
            lines.iter().any(|l| l.starts_with("-Djava.library.path=")),
            "argv should include natives path: {lines:?}"
        );
        assert!(
            lines.contains(&"net.minecraft.client.main.Main"),
            "argv should include main class: {lines:?}"
        );
        assert!(
            lines.contains(&"--username"),
            "argv should include --username: {lines:?}"
        );
    }

    #[test]
    fn spawn_game_propagates_nonzero_exit() {
        let tmp = tempfile::tempdir().expect("tmp");

        let (game, global_root, _) = build_fixture_game(&tmp, "dev", "1.21.1", None);
        let mut auth = LaunchAuthConfig::default();
        let cmd = build_launch_command("dev", &game, &global_root, &mut auth)
            .expect("build launch command");

        create_fake_java_exit_code(&managed_dir(&tmp), 42);

        let app = setup_app(&tmp, &game, Some("dev"), &global_root, None);
        let result = app.spawn_game(&cmd);

        assert!(result.is_err(), "spawn should fail with nonzero exit");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("42"),
            "error should mention exit code 42: {err_msg}"
        );
    }

    #[test]
    fn spawn_game_sets_working_directory_to_game_dir() {
        let tmp = tempfile::tempdir().expect("tmp");
        let cwd_log = tmp.path().join("cwd.log");

        let (game, global_root, _) = build_fixture_game(&tmp, "dev", "1.21.1", None);
        let mut auth = LaunchAuthConfig::default();
        let cmd = build_launch_command("dev", &game, &global_root, &mut auth)
            .expect("build launch command");

        create_fake_java_cwd_logger(&managed_dir(&tmp), &cwd_log);

        let app = setup_app(&tmp, &game, Some("dev"), &global_root, None);
        app.spawn_game(&cmd).expect("spawn should succeed");

        let logged_cwd = std::fs::read_to_string(&cwd_log).expect("read cwd log");
        let logged_cwd = logged_cwd.trim();
        let expected = game.root_dir.to_string_lossy();
        assert_eq!(
            logged_cwd,
            expected.as_ref(),
            "process cwd should be game root_dir"
        );
    }

    #[test]
    fn run_cmd_errors_when_no_games_and_no_default() {
        let tmp = tempfile::tempdir().expect("tmp");
        let config_dir = tmp.path().join("config");
        let state_dir = tmp.path().join("state");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        std::fs::create_dir_all(&state_dir).expect("create state dir");

        let config = crate::config::Config::default();
        let toml = toml::to_string_pretty(&config).expect("serialize config");
        std::fs::write(config_dir.join("config.toml"), toml).expect("write config");

        let app = App {
            config_dir,
            state_dir,
            provider_choice: ProviderChoice::Mock,
            lang: Lang::default(),
        };

        let result = app.run_cmd(true);
        assert!(result.is_err(), "should error when no games configured");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("no default game") || err_msg.contains("no games"),
            "error should mention missing games: {err_msg}"
        );
    }

    #[test]
    fn run_cmd_errors_when_default_game_not_in_games() {
        let tmp = tempfile::tempdir().expect("tmp");
        let config_dir = tmp.path().join("config");
        let state_dir = tmp.path().join("state");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        std::fs::create_dir_all(&state_dir).expect("create state dir");

        let config = crate::config::Config {
            default_game: Some("nonexistent".into()),
            ..Default::default()
        };
        let toml = toml::to_string_pretty(&config).expect("serialize config");
        std::fs::write(config_dir.join("config.toml"), toml).expect("write config");

        let app = App {
            config_dir,
            state_dir,
            provider_choice: ProviderChoice::Mock,
            lang: Lang::default(),
        };

        let result = app.run_cmd(true);
        assert!(result.is_err(), "should error when default game not found");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("nonexistent"),
            "error should mention the game name: {err_msg}"
        );
    }

    #[test]
    fn dry_run_with_online_auth_errors_without_refresh_token() {
        let tmp = tempfile::tempdir().expect("tmp");
        let (game, global_root, _) = build_fixture_game(&tmp, "dev", "1.21.1", None);
        let mut auth = LaunchAuthConfig {
            mode: LaunchAuthMode::Online,
            online: Some(OnlineAccount {
                username: "OnlineUser".into(),
                uuid: "deadbeef-dead-beef-dead-beefdeadbeef".into(),
                access_token: "real-token".into(),
                user_type: "microsoft".into(),
                ..Default::default()
            }),
        };
        let err = build_launch_command("dev", &game, &global_root, &mut auth).unwrap_err();
        assert!(
            err.to_string().contains("refresh token"),
            "error should mention refresh token: {err}"
        );
    }
}
