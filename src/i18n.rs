//! Bilingual (English/Chinese) internationalization module.
//!
//! All user-facing output strings are routed through this module. Technical
//! terms (mod, modpack, etc.) are never translated.

/// Language enum for bilingual support.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    #[default]
    En,
    Zh,
}

impl Lang {
    pub fn from_input(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "en" | "english" => Some(Self::En),
            "zh" | "chinese" | "中文" => Some(Self::Zh),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Zh => "zh",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Zh => "中文",
        }
    }
}

/// Translation macro for simple static strings.
macro_rules! t {
    ($lang:expr, $en:expr, $zh:expr) => {
        match $lang {
            Lang::En => $en,
            Lang::Zh => $zh,
        }
    };
}

// ---------------------------------------------------------------------------
// Error messages
// ---------------------------------------------------------------------------

pub fn error_prefix(lang: Lang) -> &'static str {
    t!(lang, "Error: ", "错误: ")
}

pub fn warning_prefix(lang: Lang) -> &'static str {
    t!(lang, "Warning: ", "警告: ")
}

pub fn success_prefix(lang: Lang) -> &'static str {
    t!(lang, "Success: ", "成功: ")
}

// ---------------------------------------------------------------------------
// Profile commands
// ---------------------------------------------------------------------------

pub fn added_profile(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("added profile {name}"),
        Lang::Zh => format!("已添加配置 {name}"),
    }
}

pub fn active_profile(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("active profile {name}"),
        Lang::Zh => format!("当前配置 {name}"),
    }
}

pub fn unknown_profile(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("unknown profile {name}"),
        Lang::Zh => format!("未知配置 {name}"),
    }
}

pub fn no_active_profile(lang: Lang) -> &'static str {
    t!(
        lang,
        "no active profile; run profile add or profile use",
        "没有活动配置; 运行 profile add 或 profile use"
    )
}

pub fn active_profile_missing(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("active profile {name} is missing"),
        Lang::Zh => format!("活动配置 {name} 不存在"),
    }
}

// ---------------------------------------------------------------------------
// Game commands
// ---------------------------------------------------------------------------

pub fn no_default_game(lang: Lang) -> &'static str {
    t!(lang, "no default game", "没有默认游戏")
}

pub fn unknown_game(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("unknown game {name}"),
        Lang::Zh => format!("未知游戏 {name}"),
    }
}

pub fn game_already_exists(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("game {name} already exists"),
        Lang::Zh => format!("游戏 {name} 已存在"),
    }
}

pub fn default_game(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("default game {name}"),
        Lang::Zh => format!("默认游戏 {name}"),
    }
}

pub fn renamed_game(lang: Lang, old: &str, new: &str) -> String {
    match lang {
        Lang::En => format!("renamed game {old} -> {new}"),
        Lang::Zh => format!("已重命名游戏 {old} -> {new}"),
    }
}

pub fn game_removed_mid_rename(lang: Lang) -> &'static str {
    t!(lang, "game removed mid-rename", "游戏在重命名过程中被移除")
}

// ---------------------------------------------------------------------------
// Game install commands
// ---------------------------------------------------------------------------

pub fn installed_game(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("installed game {name}"),
        Lang::Zh => format!("已安装游戏 {name}"),
    }
}

pub fn removed_game_record(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("removed game record: {name}"),
        Lang::Zh => format!("已移除游戏记录: {name}"),
    }
}

pub fn default_game_cleared(lang: Lang) -> &'static str {
    t!(lang, "default game cleared", "默认游戏已清除")
}

pub fn dry_run(lang: Lang) -> &'static str {
    t!(lang, "dry run", "模拟运行")
}

pub fn deleted_path(lang: Lang, path: &str) -> String {
    match lang {
        Lang::En => format!("deleted {path}"),
        Lang::Zh => format!("已删除 {path}"),
    }
}

// ---------------------------------------------------------------------------
// Source commands
// ---------------------------------------------------------------------------

pub fn source_already_imported(lang: Lang, url: &str) -> String {
    match lang {
        Lang::En => format!("source {url} is already imported"),
        Lang::Zh => format!("源 {url} 已导入"),
    }
}

pub fn added_source(lang: Lang, url: &str) -> String {
    match lang {
        Lang::En => format!("added source {url}"),
        Lang::Zh => format!("已添加源 {url}"),
    }
}

pub fn removed_source(lang: Lang, url: &str) -> String {
    match lang {
        Lang::En => format!("removed source {url}"),
        Lang::Zh => format!("已移除源 {url}"),
    }
}

pub fn unknown_source(lang: Lang, url: &str) -> String {
    match lang {
        Lang::En => format!("unknown source {url}"),
        Lang::Zh => format!("未知源 {url}"),
    }
}

pub fn source_status_trusted(lang: Lang) -> &'static str {
    t!(lang, "trusted (manual import)", "已信任 (手动导入)")
}

pub fn source_index_unavailable(lang: Lang, error: &str) -> String {
    match lang {
        Lang::En => format!("index: unavailable ({error})"),
        Lang::Zh => format!("索引: 不可用 ({error})"),
    }
}

// ---------------------------------------------------------------------------
// Package commands
// ---------------------------------------------------------------------------

pub fn no_scripts_to_execute(lang: Lang) -> &'static str {
    t!(lang, "no scripts to execute", "没有可执行的脚本")
}

pub fn oidc_publish_not_implemented(lang: Lang) -> &'static str {
    t!(
        lang,
        "OIDC publish flow not implemented yet",
        "OIDC 发布流程尚未实现"
    )
}

pub fn no_mcm_file_found(lang: Lang) -> &'static str {
    t!(
        lang,
        "no .mcm file found in current directory",
        "当前目录未找到 .mcm 文件"
    )
}

pub fn top_install_smart_target_error(lang: Lang) -> &'static str {
    t!(
        lang,
        "top-level install does not accept Minecraft smart targets; use `game install` instead",
        "顶层安装不接受 Minecraft 智能目标; 请使用 `game install`"
    )
}

pub fn top_install_accepts_only(lang: Lang) -> &'static str {
    t!(
        lang,
        "top-level install accepts only a `.mcm` / `.mrpack` / `.zip` file path or URL; raw mod names are not supported (use `mods install`)",
        "顶层安装仅接受 `.mcm` / `.mrpack` / `.zip` 文件路径或 URL; 不支持原始模组名称 (使用 `mods install`)"
    )
}

pub fn installed_package(lang: Lang, name: &str, version: &str) -> String {
    match lang {
        Lang::En => format!("installed package {name} {version}"),
        Lang::Zh => format!("已安装包 {name} {version}"),
    }
}

pub fn downloaded_package(lang: Lang, name: &str, version: &str) -> String {
    match lang {
        Lang::En => format!("downloaded package {name} {version}"),
        Lang::Zh => format!("已下载包 {name} {version}"),
    }
}

pub fn launch_on_install_confirmed(lang: Lang) -> &'static str {
    t!(lang, "launch-on-install confirmed", "安装时启动已确认")
}

pub fn script_warning(lang: Lang) -> &'static str {
    t!(
        lang,
        "WARNING: this package contains scripts that will be executed. Review them carefully.",
        "警告: 此包包含将要执行的脚本。请仔细审查。"
    )
}

pub fn curseforge_export_not_implemented(lang: Lang) -> &'static str {
    t!(
        lang,
        "curseforge export is not implemented yet",
        "CurseForge 导出尚未实现"
    )
}

pub fn duplicate_asset_path(lang: Lang, path: &str) -> String {
    match lang {
        Lang::En => format!("duplicate asset path in package: {path}"),
        Lang::Zh => format!("包中存在重复的资源路径: {path}"),
    }
}

pub fn unknown_step_skipped(lang: Lang, step: &str) -> String {
    match lang {
        Lang::En => format!("unknown step '{step}' skipped"),
        Lang::Zh => format!("未知步骤 '{step}' 已跳过"),
    }
}

pub fn nested_mcm_do_skipped(lang: Lang) -> &'static str {
    t!(
        lang,
        "nested mcm.do step skipped (requires dyyl host)",
        "嵌套的 mcm.do 步骤已跳过 (需要 dyyl 主机)"
    )
}

pub fn step_missing_arg(lang: Lang, step: &str, arg: &str) -> String {
    match lang {
        Lang::En => format!("step '{step}' is missing required argument '{arg}'"),
        Lang::Zh => format!("步骤 '{step}' 缺少必需参数 '{arg}'"),
    }
}

pub fn build_success(lang: Lang, path: &str) -> String {
    match lang {
        Lang::En => format!("built v2 lock: {path}"),
        Lang::Zh => format!("已构建 v2 锁: {path}"),
    }
}

pub fn make_success(lang: Lang, path: &str) -> String {
    match lang {
        Lang::En => format!("exported dyyl source: {path}"),
        Lang::Zh => format!("已导出 dyyl 源: {path}"),
    }
}

// ---------------------------------------------------------------------------
// Lifecycle commands
// ---------------------------------------------------------------------------

pub fn install_requires_query_or_file(lang: Lang) -> &'static str {
    t!(
        lang,
        "install requires a query or --file <PATH>",
        "安装需要查询或 --file <PATH>"
    )
}

pub fn installation_cancelled(lang: Lang) -> &'static str {
    t!(lang, "installation cancelled", "安装已取消")
}

pub fn removed_mod(lang: Lang, id: &str) -> String {
    match lang {
        Lang::En => format!("removed {id}"),
        Lang::Zh => format!("已移除 {id}"),
    }
}

pub fn mod_not_installed(lang: Lang, id: &str) -> String {
    match lang {
        Lang::En => format!("{id} is not installed"),
        Lang::Zh => format!("{id} 未安装"),
    }
}

pub fn mod_is_automatic(lang: Lang, id: &str) -> String {
    match lang {
        Lang::En => format!("{id} is automatic; use autoremove when no roots require it"),
        Lang::Zh => format!("{id} 是自动安装的; 当没有根依赖需要时使用 autoremove"),
    }
}

pub fn confirmation_required_pass_yes(lang: Lang) -> &'static str {
    t!(
        lang,
        "confirmation required; pass --yes to apply",
        "需要确认; 传入 --yes 以应用"
    )
}

pub fn nothing_to_autoremove(lang: Lang) -> &'static str {
    t!(lang, "nothing to autoremove", "没有可自动移除的内容")
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

pub fn selected_from_search(lang: Lang, id: &str, query: &str) -> String {
    match lang {
        Lang::En => format!("selected {id} from search result {query}"),
        Lang::Zh => format!("从搜索结果 {query} 中选择了 {id}"),
    }
}

pub fn mod_not_found_by_search(lang: Lang, query: &str) -> String {
    match lang {
        Lang::En => format!("mod {query} not found by search"),
        Lang::Zh => format!("搜索未找到模组 {query}"),
    }
}

pub fn candidates_label(lang: Lang) -> &'static str {
    t!(lang, "candidates:", "可用版本:")
}

pub fn selected_label(lang: Lang) -> &'static str {
    t!(lang, "selected:", "已选择:")
}

pub fn required_deps_label(lang: Lang) -> &'static str {
    t!(lang, "required deps:", "必需依赖:")
}

pub fn optional_deps_label(lang: Lang) -> &'static str {
    t!(lang, "optional deps:", "可选依赖:")
}

pub fn missing_file(lang: Lang, id: &str, filename: &str) -> String {
    match lang {
        Lang::En => format!("missing: {id} ({filename})"),
        Lang::Zh => format!("缺失: {id} ({filename})"),
    }
}

pub fn changed_file(lang: Lang, id: &str, filename: &str) -> String {
    match lang {
        Lang::En => format!("changed: {id} ({filename})"),
        Lang::Zh => format!("已更改: {id} ({filename})"),
    }
}

pub fn ok_status(lang: Lang, id: &str) -> String {
    match lang {
        Lang::En => format!("ok: {id}"),
        Lang::Zh => format!("正常: {id}"),
    }
}

pub fn untracked_file(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("untracked: {name}"),
        Lang::Zh => format!("未跟踪: {name}"),
    }
}

// ---------------------------------------------------------------------------
// Install
// ---------------------------------------------------------------------------

pub fn install_plan_item(lang: Lang, id: &str, version: &str, reason: &str) -> String {
    match lang {
        Lang::En => format!("install {id} {version} {reason}"),
        Lang::Zh => format!("安装 {id} {version} {reason}"),
    }
}

pub fn warning_message(lang: Lang, msg: &str) -> String {
    match lang {
        Lang::En => format!("warning: {msg}"),
        Lang::Zh => format!("警告: {msg}"),
    }
}

pub fn no_stable_compatible_artifact(lang: Lang, id: &str) -> String {
    match lang {
        Lang::En => format!("no stable compatible artifact for {id}"),
        Lang::Zh => format!("没有适用于 {id} 的稳定兼容版本"),
    }
}

pub fn optional_dependency_not_installed(lang: Lang, id: &str) -> String {
    match lang {
        Lang::En => format!("optional dependency {id} not installed"),
        Lang::Zh => format!("可选依赖 {id} 未安装"),
    }
}

pub fn embedded_dependency_not_installed(lang: Lang, id: &str) -> String {
    match lang {
        Lang::En => format!("embedded dependency {id} not installed"),
        Lang::Zh => format!("内嵌依赖 {id} 未安装"),
    }
}

pub fn incompatible_dependency_not_installed(lang: Lang, id: &str) -> String {
    match lang {
        Lang::En => format!("incompatible dependency {id} not installed"),
        Lang::Zh => format!("不兼容依赖 {id} 未安装"),
    }
}

pub fn unknown_dependency_not_installed(lang: Lang, id: &str) -> String {
    match lang {
        Lang::En => format!("unknown dependency {id} not installed"),
        Lang::Zh => format!("未知依赖 {id} 未安装"),
    }
}

// ---------------------------------------------------------------------------
// Run command
// ---------------------------------------------------------------------------

pub fn no_default_game_and_no_games(lang: Lang) -> &'static str {
    t!(
        lang,
        "no default game and no games configured; run `mcm game install <name> <target>` to install a game, then `mcm game default <name>` to set it as default",
        "没有默认游戏且未配置游戏; 运行 `mcm game install <name> <target>` 安装游戏, 然后 `mcm game default <name>` 设置为默认"
    )
}

pub fn default_game_does_not_exist(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("default game {name} does not exist; run `mcm game default <name>` to set a different default"),
        Lang::Zh => format!("默认游戏 {name} 不存在; 运行 `mcm game default <name>` 设置其他默认游戏"),
    }
}

pub fn real_launch_not_implemented(lang: Lang) -> &'static str {
    t!(
        lang,
        "real launch is not implemented yet; use --dry-run to preview the launch command",
        "实际启动尚未实现; 使用 --dry-run 预览启动命令"
    )
}

// ---------------------------------------------------------------------------
// Upgrade commands
// ---------------------------------------------------------------------------

pub fn no_games_configured(lang: Lang) -> &'static str {
    t!(
        lang,
        "no games configured; run game install first",
        "未配置游戏; 请先运行 game install"
    )
}

pub fn all_games_up_to_date(lang: Lang) -> &'static str {
    t!(lang, "all games already up to date", "所有游戏已是最新版本")
}

pub fn upgrade_plan_for(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("upgrade plan for {name}:"),
        Lang::Zh => format!("{name} 的升级计划:"),
    }
}

pub fn already_up_to_date(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("{name}: already up to date"),
        Lang::Zh => format!("{name}: 已是最新版本"),
    }
}

pub fn upgraded(lang: Lang, msg: &str) -> String {
    match lang {
        Lang::En => format!("upgraded {msg}"),
        Lang::Zh => format!("已升级 {msg}"),
    }
}

pub fn skipped(lang: Lang, msg: &str) -> String {
    match lang {
        Lang::En => format!("skipped: {msg}"),
        Lang::Zh => format!("已跳过: {msg}"),
    }
}

// ---------------------------------------------------------------------------
// Runtime commands
// ---------------------------------------------------------------------------

pub fn java_runtime_already_available(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("Java runtime already available for game {name}"),
        Lang::Zh => format!("游戏 {name} 的 Java 运行时已可用"),
    }
}

pub fn installing_managed_java(lang: Lang, version: &str, name: &str) -> String {
    match lang {
        Lang::En => format!("installing managed Java {version} for game {name}..."),
        Lang::Zh => format!("正在为游戏 {name} 安装托管 Java {version}..."),
    }
}

pub fn installed_managed_java(lang: Lang, version: &str) -> String {
    match lang {
        Lang::En => format!("installed managed Java {version}"),
        Lang::Zh => format!("已安装托管 Java {version}"),
    }
}

pub fn system_java_not_implemented(lang: Lang) -> &'static str {
    t!(
        lang,
        "system-wide Java runtime install is not implemented yet",
        "系统级 Java 运行时安装尚未实现"
    )
}

// ---------------------------------------------------------------------------
// Modpack import
// ---------------------------------------------------------------------------

pub fn imported_modpack(lang: Lang) -> &'static str {
    t!(lang, "imported modpack", "已导入模组包")
}

pub fn not_a_modpack(lang: Lang, target: &str) -> String {
    match lang {
        Lang::En => {
            format!("not a modpack: {target} (no modrinth.index.json or manifest.json at zip root)")
        }
        Lang::Zh => {
            format!("不是模组包: {target} (zip 根目录没有 modrinth.index.json 或 manifest.json)")
        }
    }
}

pub fn archive_too_many_entries(lang: Lang, count: usize, limit: usize) -> String {
    match lang {
        Lang::En => format!("archive has {count} entries, exceeds limit of {limit}"),
        Lang::Zh => format!("压缩包有 {count} 个条目, 超过限制 {limit}"),
    }
}

pub fn archive_too_large(lang: Lang, limit: u64) -> String {
    match lang {
        Lang::En => format!("archive total size exceeds {limit} bytes (zip-bomb protection)"),
        Lang::Zh => format!("压缩包总大小超过 {limit} 字节 (zip-bomb 防护)"),
    }
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

pub fn server_listening(lang: Lang, bind: &str, mode: &str) -> String {
    match lang {
        Lang::En => format!("mcm serve listening on {bind} mode={mode}"),
        Lang::Zh => format!("mcm 服务监听于 {bind} 模式={mode}"),
    }
}

// ---------------------------------------------------------------------------
// Confirmation prompts
// ---------------------------------------------------------------------------

pub fn proceed_with_install(lang: Lang) -> &'static str {
    t!(lang, "Proceed with install? [y/N]", "继续安装? [y/N]")
}

pub fn proceed_with_download(lang: Lang) -> &'static str {
    t!(lang, "Proceed with download? [y/N]", "继续下载? [y/N]")
}

pub fn proceed_with_removal(lang: Lang) -> &'static str {
    t!(lang, "Proceed with removal? [y/N]", "继续移除? [y/N]")
}

pub fn proceed_with_version_removal(lang: Lang) -> &'static str {
    t!(
        lang,
        "Proceed with version removal? [y/N]",
        "继续版本移除? [y/N]"
    )
}

pub fn proceed_with_package_install(lang: Lang) -> &'static str {
    t!(
        lang,
        "Proceed with package install? [y/N]",
        "继续包安装? [y/N]"
    )
}

pub fn proceed_with_runtime_install(lang: Lang) -> &'static str {
    t!(
        lang,
        "Proceed with runtime install? [y/N]",
        "继续运行时安装? [y/N]"
    )
}

pub fn proceed_with_source_action(lang: Lang) -> &'static str {
    t!(
        lang,
        "Proceed with source action? [y/N]",
        "继续源操作? [y/N]"
    )
}

pub fn proceed_with_script_execution(lang: Lang) -> &'static str {
    t!(
        lang,
        "Proceed with script execution? [y/N]",
        "继续脚本执行? [y/N]"
    )
}

pub fn proceed_with_launch_on_install(lang: Lang) -> &'static str {
    t!(
        lang,
        "Proceed with launch-on-install? [y/N]",
        "继续安装时启动? [y/N]"
    )
}

pub fn proceed(lang: Lang) -> &'static str {
    t!(lang, "Proceed? [y/N]", "继续? [y/N]")
}

pub fn confirmation_declined(lang: Lang) -> &'static str {
    t!(lang, "confirmation declined", "确认已拒绝")
}

pub fn confirmation_required_non_tty(lang: Lang) -> &'static str {
    t!(
        lang,
        "confirmation required; pass --yes to proceed",
        "需要确认; 传入 --yes 以继续"
    )
}

pub fn confirmation_required_non_bypassable(lang: Lang) -> &'static str {
    t!(
        lang,
        "confirmation required; this operation is non-bypassable and must be confirmed interactively",
        "需要确认; 此操作不可跳过, 必须交互式确认"
    )
}

// ---------------------------------------------------------------------------
// MC-critical warnings
// ---------------------------------------------------------------------------

pub fn autoremove_warning(lang: Lang) -> &'static str {
    t!(
        lang,
        "WARNING: autoremove is MC-critical. Removing apparently unused mods/resources may break worlds/saves or modded structures.",
        "警告: 自动移除是 MC 关键操作。移除明显未使用的模组/资源可能会损坏世界/存档或模组结构。"
    )
}

pub fn world_overwrite_warning(lang: Lang) -> &'static str {
    t!(
        lang,
        "WARNING: world overwrite is MC-critical. This will replace existing worlds/saves or modded structures.",
        "警告: 世界覆盖是 MC 关键操作。这将替换现有世界/存档或模组结构。"
    )
}

pub fn world_delete_warning(lang: Lang) -> &'static str {
    t!(
        lang,
        "WARNING: world deletion is MC-critical. This permanently removes worlds/saves or modded structures.",
        "警告: 世界删除是 MC 关键操作。这将永久移除世界/存档或模组结构。"
    )
}

pub fn autoremove_typed_prompt(lang: Lang) -> &'static str {
    t!(
        lang,
        "autoremove is MC-critical. Type 'yes' to proceed, anything else to cancel",
        "自动移除是 MC 关键操作。输入 'yes' 继续, 其他任何内容取消"
    )
}

pub fn world_overwrite_typed_prompt(lang: Lang) -> &'static str {
    t!(
        lang,
        "world overwrite is MC-critical. Type 'yes' to proceed, anything else to cancel",
        "世界覆盖是 MC 关键操作。输入 'yes' 继续, 其他任何内容取消"
    )
}

pub fn world_delete_typed_prompt(lang: Lang) -> &'static str {
    t!(
        lang,
        "world deletion is MC-critical. Type 'yes' to proceed, anything else to cancel",
        "世界删除是 MC 关键操作。输入 'yes' 继续, 其他任何内容取消"
    )
}

pub fn root_system_typed_prompt(lang: Lang) -> &'static str {
    t!(
        lang,
        "this action modifies root/system state. Type 'yes' to proceed, anything else to cancel",
        "此操作修改 root/系统状态。输入 'yes' 继续, 其他任何内容取消"
    )
}

pub fn default_typed_prompt(lang: Lang) -> &'static str {
    t!(
        lang,
        "Type 'yes' to proceed, anything else to cancel",
        "输入 'yes' 继续, 其他任何内容取消"
    )
}

// ---------------------------------------------------------------------------
// Root escalation
// ---------------------------------------------------------------------------

pub fn root_privileges_required_for(lang: Lang, action: &str) -> String {
    match lang {
        Lang::En => format!("root privileges are required for: {action}"),
        Lang::Zh => format!("需要 root 权限: {action}"),
    }
}

pub fn rerun_with(lang: Lang, cmd: &str) -> String {
    match lang {
        Lang::En => format!("re-run with: {cmd}"),
        Lang::Zh => format!("使用以下命令重新运行: {cmd}"),
    }
}

pub fn root_privileges_required(lang: Lang) -> &'static str {
    t!(
        lang,
        "root privileges required; see suggestion above",
        "需要 root 权限; 请参阅上述建议"
    )
}

pub fn root_privileges_required_pass_yes(lang: Lang) -> &'static str {
    t!(
        lang,
        "root privileges required; pass --yes or run interactively",
        "需要 root 权限; 传入 --yes 或交互式运行"
    )
}

pub fn root_privileges_required_action(lang: Lang) -> &'static str {
    t!(
        lang,
        "root privileges are required for this action.",
        "此操作需要 root 权限。"
    )
}

// ---------------------------------------------------------------------------
// Safety
// ---------------------------------------------------------------------------

pub fn unsafe_filename(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("unsafe artifact filename {name:?}"),
        Lang::Zh => format!("不安全的制品文件名 {name:?}"),
    }
}

pub fn download_url_must_use_https(lang: Lang, url: &str) -> String {
    match lang {
        Lang::En => format!("download URL must use https: {url}"),
        Lang::Zh => format!("下载 URL 必须使用 https: {url}"),
    }
}

pub fn download_url_no_credentials(lang: Lang, url: &str) -> String {
    match lang {
        Lang::En => format!("download URL must not contain credentials: {url}"),
        Lang::Zh => format!("下载 URL 不能包含凭据: {url}"),
    }
}

pub fn download_url_no_host(lang: Lang, url: &str) -> String {
    match lang {
        Lang::En => format!("download URL has no host: {url}"),
        Lang::Zh => format!("下载 URL 没有主机: {url}"),
    }
}

pub fn download_url_host_private(lang: Lang, host: &str) -> String {
    match lang {
        Lang::En => format!("download URL host is private or loopback: {host}"),
        Lang::Zh => format!("下载 URL 主机是私有或回环地址: {host}"),
    }
}

pub fn download_url_host_not_in_allowlist(lang: Lang, host: &str) -> String {
    match lang {
        Lang::En => format!("download URL host {host} is not in allowlist"),
        Lang::Zh => format!("下载 URL 主机 {host} 不在白名单中"),
    }
}

pub fn invalid_download_url(lang: Lang, url: &str) -> String {
    match lang {
        Lang::En => format!("invalid download URL {url}"),
        Lang::Zh => format!("无效的下载 URL {url}"),
    }
}

// ---------------------------------------------------------------------------
// Misc
// ---------------------------------------------------------------------------

pub fn config_not_found(lang: Lang) -> &'static str {
    t!(lang, "config not found", "未找到配置")
}

pub fn lock_file_corrupted(lang: Lang) -> &'static str {
    t!(lang, "lock file corrupted", "锁文件损坏")
}

pub fn no_default_game_set(lang: Lang) -> &'static str {
    t!(lang, "no default game set", "未设置默认游戏")
}

pub fn game_not_found(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("game '{name}' not found"),
        Lang::Zh => format!("游戏 '{name}' 未找到"),
    }
}

pub fn source_exists(lang: Lang, url: &str) -> String {
    match lang {
        Lang::En => format!("source '{url}' already exists"),
        Lang::Zh => format!("源 '{url}' 已存在"),
    }
}

pub fn source_not_found(lang: Lang, url: &str) -> String {
    match lang {
        Lang::En => format!("source '{url}' not found"),
        Lang::Zh => format!("源 '{url}' 未找到"),
    }
}

pub fn package_not_found(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("package '{name}' not found"),
        Lang::Zh => format!("包 '{name}' 未找到"),
    }
}

pub fn invalid_package_name(lang: Lang) -> &'static str {
    t!(lang, "invalid package name", "无效的包名")
}

pub fn path_must_be_absolute(lang: Lang) -> &'static str {
    t!(lang, "path must be absolute", "路径必须是绝对路径")
}

pub fn cannot_install_under_x(lang: Lang) -> &'static str {
    t!(lang, "cannot install under /x", "无法安装到 /x 下")
}

pub fn downloading(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("Downloading {name}..."),
        Lang::Zh => format!("正在下载 {name}..."),
    }
}

pub fn installing(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("Installing {name}..."),
        Lang::Zh => format!("正在安装 {name}..."),
    }
}

pub fn removing(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("Removing {name}..."),
        Lang::Zh => format!("正在移除 {name}..."),
    }
}

pub fn upgrading(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("Upgrading {name}..."),
        Lang::Zh => format!("正在升级 {name}..."),
    }
}

pub fn done(lang: Lang) -> &'static str {
    t!(lang, "Done.", "完成。")
}

pub fn cancelled(lang: Lang) -> &'static str {
    t!(lang, "Cancelled.", "已取消。")
}

pub fn abort(lang: Lang) -> &'static str {
    t!(lang, "Abort.", "中止。")
}

pub fn yes_no(lang: Lang) -> &'static str {
    t!(lang, "Yes/No", "是/否")
}

pub fn installed_n_mods(lang: Lang, count: usize) -> String {
    match lang {
        Lang::En => format!("installed {count} mods"),
        Lang::Zh => format!("已安装 {count} 个模组"),
    }
}

pub fn removed_n_mods(lang: Lang, count: usize) -> String {
    match lang {
        Lang::En => format!("removed {count} mods"),
        Lang::Zh => format!("已移除 {count} 个模组"),
    }
}

pub fn no_mods_to_remove(lang: Lang) -> &'static str {
    t!(lang, "no mods to remove", "没有可移除的模组")
}

pub fn profile_not_found(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("profile '{name}' not found"),
        Lang::Zh => format!("配置 '{name}' 未找到"),
    }
}

pub fn version_not_found(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("version '{name}' not found"),
        Lang::Zh => format!("版本 '{name}' 未找到"),
    }
}

pub fn loader_not_supported(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("loader '{name}' not supported"),
        Lang::Zh => format!("加载器 '{name}' 不支持"),
    }
}

pub fn java_not_found(lang: Lang) -> &'static str {
    t!(lang, "Java not found", "未找到 Java")
}

pub fn minecraft_not_found(lang: Lang) -> &'static str {
    t!(lang, "Minecraft not found", "未找到 Minecraft")
}

pub fn server_mode(lang: Lang, mode: &str) -> String {
    match lang {
        Lang::En => format!("Server mode: {mode}"),
        Lang::Zh => format!("服务模式: {mode}"),
    }
}

pub fn listening_on(lang: Lang, addr: &str) -> String {
    match lang {
        Lang::En => format!("Listening on {addr}"),
        Lang::Zh => format!("监听于 {addr}"),
    }
}

pub fn package_published(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("Package published: {name}"),
        Lang::Zh => format!("包已发布: {name}"),
    }
}

pub fn package_updated(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("Package updated: {name}"),
        Lang::Zh => format!("包已更新: {name}"),
    }
}

pub fn package_deleted(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("Package deleted: {name}"),
        Lang::Zh => format!("包已删除: {name}"),
    }
}

pub fn language_set(lang: Lang) -> String {
    match lang {
        Lang::En => "Language set to English".to_owned(),
        Lang::Zh => "语言已设置为中文".to_owned(),
    }
}

pub fn current_language(lang: Lang) -> String {
    match lang {
        Lang::En => format!("Current language: {}", lang.display_name()),
        Lang::Zh => format!("当前语言: {}", lang.display_name()),
    }
}

pub fn unknown_language(lang: Lang, input: &str) -> String {
    match lang {
        Lang::En => format!("Unknown language: {input} (use 'en' or 'zh')"),
        Lang::Zh => format!("未知语言: {input} (使用 'en' 或 'zh')"),
    }
}

pub fn could_not_resolve_project_dirs(lang: Lang) -> &'static str {
    t!(lang, "could not resolve project dirs", "无法解析项目目录")
}

pub fn build_tokio_runtime(lang: Lang) -> &'static str {
    t!(
        lang,
        "build tokio runtime for serve",
        "为服务构建 tokio 运行时"
    )
}

pub fn config_not_implemented_yet(lang: Lang) -> &'static str {
    t!(lang, "config is not implemented yet", "config 尚未实现")
}

pub fn read_dir_error(lang: Lang, dir: &str) -> String {
    match lang {
        Lang::En => format!("read dir {dir}"),
        Lang::Zh => format!("读取目录 {dir}"),
    }
}

pub fn read_file_error(lang: Lang, path: &str) -> String {
    match lang {
        Lang::En => format!("read {path}"),
        Lang::Zh => format!("读取 {path}"),
    }
}

pub fn write_file_error(lang: Lang, path: &str) -> String {
    match lang {
        Lang::En => format!("write {path}"),
        Lang::Zh => format!("写入 {path}"),
    }
}

pub fn parse_file_error(lang: Lang, path: &str) -> String {
    match lang {
        Lang::En => format!("parse {path}"),
        Lang::Zh => format!("解析 {path}"),
    }
}

pub fn create_dir_error(lang: Lang, path: &str) -> String {
    match lang {
        Lang::En => format!("create {path}"),
        Lang::Zh => format!("创建 {path}"),
    }
}

pub fn remove_error(lang: Lang, path: &str) -> String {
    match lang {
        Lang::En => format!("remove {path}"),
        Lang::Zh => format!("移除 {path}"),
    }
}

pub fn run_action_error(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("run action {name}"),
        Lang::Zh => format!("运行动作 {name}"),
    }
}

pub fn action_exited_with_status(lang: Lang, name: &str, status: &str) -> String {
    match lang {
        Lang::En => format!("action {name} exited with status {status}"),
        Lang::Zh => format!("动作 {name} 以状态 {status} 退出"),
    }
}

pub fn fetch_error(lang: Lang, url: &str) -> String {
    match lang {
        Lang::En => format!("read fetched {url}"),
        Lang::Zh => format!("读取已获取的 {url}"),
    }
}

pub fn source_unavailable(lang: Lang, url: &str, error: &str) -> String {
    match lang {
        Lang::En => format!("warning: source {url} unavailable: {error}"),
        Lang::Zh => format!("警告: 源 {url} 不可用: {error}"),
    }
}

pub fn integrity_check_failed(lang: Lang, url: &str, msg: &str) -> String {
    match lang {
        Lang::En => format!("integrity check failed for {url}: {msg}"),
        Lang::Zh => format!("完整性检查失败 {url}: {msg}"),
    }
}

pub fn source_mod_no_download_url(lang: Lang, id: &str) -> String {
    match lang {
        Lang::En => format!("source mod {id} has no download URL"),
        Lang::Zh => format!("源模组 {id} 没有下载 URL"),
    }
}

pub fn could_not_resolve_game_root(lang: Lang) -> &'static str {
    t!(
        lang,
        "could not resolve game root from active profile mods_dir",
        "无法从活动配置的 mods_dir 解析游戏根目录"
    )
}

pub fn not_implemented_yet(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("{name} is not implemented yet"),
        Lang::Zh => format!("{name} 尚未实现"),
    }
}

pub fn install_plan(lang: Lang, path: &str) -> String {
    match lang {
        Lang::En => format!("install plan: managed runtime at {path}"),
        Lang::Zh => format!("安装计划: 托管运行时位于 {path}"),
    }
}

pub fn run_install_command(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("run `mcm game runtime install {name} --yes` to install"),
        Lang::Zh => format!("运行 `mcm game runtime install {name} --yes` 安装"),
    }
}

pub fn status_found(lang: Lang) -> &'static str {
    t!(lang, "status: found", "状态: 已找到")
}

pub fn status_not_found(lang: Lang) -> &'static str {
    t!(lang, "status: not found", "状态: 未找到")
}

pub fn status_error(lang: Lang, error: &str) -> String {
    match lang {
        Lang::En => format!("status: error: {error}"),
        Lang::Zh => format!("状态: 错误: {error}"),
    }
}

pub fn java_required(lang: Lang, version: &str) -> String {
    match lang {
        Lang::En => format!("java required: {version}"),
        Lang::Zh => format!("需要 Java: {version}"),
    }
}

pub fn java_required_unknown(lang: Lang) -> &'static str {
    t!(
        lang,
        "java required: (unknown - no mc_version)",
        "需要 Java: (未知 - 没有 mc_version)"
    )
}

pub fn java_version(lang: Lang, version: &str) -> String {
    match lang {
        Lang::En => format!("java version: {version}"),
        Lang::Zh => format!("Java 版本: {version}"),
    }
}

pub fn java_path(lang: Lang, path: &str) -> String {
    match lang {
        Lang::En => format!("java path: {path}"),
        Lang::Zh => format!("Java 路径: {path}"),
    }
}

pub fn java_source(lang: Lang, source: &str) -> String {
    match lang {
        Lang::En => format!("java source: {source}"),
        Lang::Zh => format!("Java 来源: {source}"),
    }
}

pub fn user_config_source(lang: Lang, path: &str) -> String {
    match lang {
        Lang::En => format!("user config ({path})"),
        Lang::Zh => format!("用户配置 ({path})"),
    }
}

pub fn managed_source(lang: Lang, path: &str) -> String {
    match lang {
        Lang::En => format!("managed ({path})"),
        Lang::Zh => format!("托管 ({path})"),
    }
}

pub fn system_path_source(lang: Lang) -> &'static str {
    t!(lang, "system PATH", "系统 PATH")
}

// ---------------------------------------------------------------------------
// Version manifest
// ---------------------------------------------------------------------------

pub fn unknown_mc_version(lang: Lang, version: &str) -> String {
    match lang {
        Lang::En => format!("unknown MC version {version}"),
        Lang::Zh => format!("未知 MC 版本 {version}"),
    }
}

pub fn unknown_mc_version_for_java(lang: Lang, version: &str) -> String {
    match lang {
        Lang::En => format!("unknown MC version {version} for Java requirement"),
        Lang::Zh => format!("未知 MC 版本 {version} (用于 Java 要求)"),
    }
}

pub fn game_no_mc_version(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("game {name} has no mc_version set"),
        Lang::Zh => format!("游戏 {name} 未设置 mc_version"),
    }
}

pub fn game_no_mc_version_for_upgrade(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("game {name} has no mc_version"),
        Lang::Zh => format!("游戏 {name} 没有 mc_version"),
    }
}

pub fn game_no_loader(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("game {name} has no loader"),
        Lang::Zh => format!("游戏 {name} 没有加载器"),
    }
}

pub fn configured_java_wrong_version(
    lang: Lang,
    path: &str,
    actual: &str,
    mc_version: &str,
    required: &str,
) -> String {
    match lang {
        Lang::En => format!("configured java at {path} is version {actual}, but {mc_version} requires Java {required}"),
        Lang::Zh => format!("配置的 java 在 {path} 版本为 {actual}, 但 {mc_version} 需要 Java {required}"),
    }
}

// ---------------------------------------------------------------------------
// Misc messages
// ---------------------------------------------------------------------------

pub fn project_has_no_candidates(lang: Lang) -> &'static str {
    t!(lang, "project has no candidates", "项目没有可用版本")
}

pub fn missing_download_url(lang: Lang) -> &'static str {
    t!(lang, "missing download URL", "缺少下载 URL")
}

pub fn no_compatible_artifact(lang: Lang, id: &str) -> String {
    match lang {
        Lang::En => format!("no compatible artifact available for {id}"),
        Lang::Zh => format!("没有适用于 {id} 的兼容版本"),
    }
}

pub fn not_found_by_provider(lang: Lang, id: &str) -> String {
    match lang {
        Lang::En => format!("{id}: not found by provider"),
        Lang::Zh => format!("{id}: 未被提供者找到"),
    }
}

pub fn no_compatible_artifact_available(lang: Lang, id: &str) -> String {
    match lang {
        Lang::En => format!("{id}: no compatible artifact available"),
        Lang::Zh => format!("{id}: 没有可用的兼容版本"),
    }
}

pub fn owner_mismatch(lang: Lang, id: &str, installed: &str, available: &str) -> String {
    match lang {
        Lang::En => format!("{id}: owner mismatch — installed by {installed}, remote owned by {available}; refusing upgrade"),
        Lang::Zh => format!("{id}: 所有者不匹配 — 由 {installed} 安装, 远程由 {available} 拒绝升级"),
    }
}

pub fn required_dependency_not_satisfied(lang: Lang, file_id: &str, dep_id: &str) -> String {
    match lang {
        Lang::En => format!("{file_id}: required dependency {dep_id} not satisfied — not installed and not in upgrade plan; refusing upgrade"),
        Lang::Zh => format!("{file_id}: 必需依赖 {dep_id} 未满足 — 未安装且不在升级计划中; 拒绝升级"),
    }
}

pub fn incompatible_dependency_installed(lang: Lang, file_id: &str, dep_id: &str) -> String {
    match lang {
        Lang::En => {
            format!("{file_id}: incompatible dependency {dep_id} is installed; refusing upgrade")
        }
        Lang::Zh => format!("{file_id}: 不兼容依赖 {dep_id} 已安装; 拒绝升级"),
    }
}

pub fn unsafe_dependency_installed(lang: Lang, file_id: &str, kind: &str, dep_id: &str) -> String {
    match lang {
        Lang::En => format!("{file_id}: {kind} dependency {dep_id} is installed — upgrade may be unsafe; refusing upgrade"),
        Lang::Zh => format!("{file_id}: {kind} 依赖 {dep_id} 已安装 — 升级可能不安全; 拒绝升级"),
    }
}

pub fn project_has_no_candidates_for(lang: Lang, id: &str) -> String {
    match lang {
        Lang::En => format!("project {id} has no candidates"),
        Lang::Zh => format!("项目 {id} 没有可用版本"),
    }
}

// ---------------------------------------------------------------------------
// Pkg auth commands
// ---------------------------------------------------------------------------

pub fn auth_login_print_url(lang: Lang, url: &str) -> String {
    match lang {
        Lang::En => format!("Open this URL in your browser to authenticate:\n{url}"),
        Lang::Zh => format!("请在浏览器中打开以下链接进行认证:\n{url}"),
    }
}

pub fn auth_login_polling(lang: Lang) -> &'static str {
    t!(
        lang,
        "Waiting for browser authentication...",
        "等待浏览器认证..."
    )
}

pub fn auth_login_success(lang: Lang, owner: &str) -> String {
    match lang {
        Lang::En => format!("Logged in as {owner}"),
        Lang::Zh => format!("已登录为 {owner}"),
    }
}

pub fn auth_login_expired(lang: Lang) -> &'static str {
    t!(
        lang,
        "Login expired — please try again",
        "登录已过期 — 请重试"
    )
}

pub fn auth_login_denied(lang: Lang, reason: &str) -> String {
    match lang {
        Lang::En => format!("Login failed: {reason}"),
        Lang::Zh => format!("登录失败: {reason}"),
    }
}

pub fn auth_login_network_error(lang: Lang, error: &str) -> String {
    match lang {
        Lang::En => format!("Network error: {error}"),
        Lang::Zh => format!("网络错误: {error}"),
    }
}

pub fn auth_status_authenticated(lang: Lang, owner: &str) -> String {
    match lang {
        Lang::En => format!("Authenticated as {owner}"),
        Lang::Zh => format!("已认证为 {owner}"),
    }
}

pub fn auth_status_not_authenticated(lang: Lang) -> &'static str {
    t!(
        lang,
        "Not authenticated. Run `mcm pkg auth login --server <url>` to log in.",
        "未认证。运行 `mcm pkg auth login --server <url>` 进行登录。"
    )
}

pub fn auth_logout_success(lang: Lang) -> &'static str {
    t!(lang, "Logged out successfully", "已成功退出登录")
}

pub fn auth_logout_error(lang: Lang, error: &str) -> String {
    match lang {
        Lang::En => format!("Logout failed: {error}"),
        Lang::Zh => format!("退出登录失败: {error}"),
    }
}

// ---------------------------------------------------------------------------
// Share client OIDC strings
// ---------------------------------------------------------------------------

pub fn oidc_start_error(lang: Lang) -> &'static str {
    t!(lang, "failed to reach OIDC server", "无法连接 OIDC 服务器")
}

pub fn oidc_start_parse_error(lang: Lang) -> &'static str {
    t!(
        lang,
        "failed to parse OIDC start response",
        "解析 OIDC 启动响应失败"
    )
}

pub fn oidc_missing_auth_url(lang: Lang) -> &'static str {
    t!(
        lang,
        "missing auth_url in OIDC start response",
        "OIDC 启动响应中缺少 auth_url"
    )
}

pub fn oidc_missing_login_id(lang: Lang) -> &'static str {
    t!(
        lang,
        "missing login_id in OIDC start response",
        "OIDC 启动响应中缺少 login_id"
    )
}

pub fn oidc_open_browser(lang: Lang) -> &'static str {
    t!(
        lang,
        "Open this URL in your browser to authenticate:",
        "请在浏览器中打开以下链接进行认证:"
    )
}

pub fn oidc_login_timeout(lang: Lang) -> &'static str {
    t!(lang, "Login timed out", "登录超时")
}

pub fn oidc_missing_token(lang: Lang) -> &'static str {
    t!(
        lang,
        "missing token in OIDC complete response",
        "OIDC 完成响应中缺少 token"
    )
}

pub fn oidc_login_success(lang: Lang, owner: &str) -> String {
    match lang {
        Lang::En => format!("Logged in as {owner}"),
        Lang::Zh => format!("已登录为 {owner}"),
    }
}

pub fn oidc_login_expired(lang: Lang) -> &'static str {
    t!(
        lang,
        "Login expired — please try again",
        "登录已过期 — 请重试"
    )
}

pub fn oidc_login_denied(lang: Lang, reason: &str) -> String {
    match lang {
        Lang::En => format!("Login failed: {reason}"),
        Lang::Zh => format!("登录失败: {reason}"),
    }
}

pub fn user_config_invalid_source_weight_key(lang: Lang, key: &str) -> String {
    match lang {
        Lang::En => format!("invalid source weight key: {key} (expected source.weight.<provider>)"),
        Lang::Zh => format!("无效的源权重键: {key} (预期 source.weight.<provider>)"),
    }
}

pub fn user_config_invalid_number(lang: Lang, value: &str) -> String {
    match lang {
        Lang::En => format!("invalid number: {value}"),
        Lang::Zh => format!("无效数字: {value}"),
    }
}

pub fn user_config_weight_must_be_positive(lang: Lang, weight: f64) -> String {
    match lang {
        Lang::En => format!("source weight must be positive, got {weight}"),
        Lang::Zh => format!("源权重必须为正数，当前为 {weight}"),
    }
}

pub fn user_config_source_weight_set(lang: Lang, provider: &str, weight: f64) -> String {
    match lang {
        Lang::En => format!("set source weight for {provider} to {weight}"),
        Lang::Zh => format!("已设置 {provider} 的源权重为 {weight}"),
    }
}

pub fn user_config_unknown_key(lang: Lang, key: &str) -> String {
    match lang {
        Lang::En => format!("unknown user config key: {key}"),
        Lang::Zh => format!("未知的用户配置键: {key}"),
    }
}
