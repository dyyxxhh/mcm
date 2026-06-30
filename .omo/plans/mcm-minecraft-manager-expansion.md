# mcm-minecraft-manager-expansion - Work Plan

## TL;DR (For humans)

**What you'll get:** MCM will be planned as a full Minecraft manager: it keeps the current mod-manager behavior, adds `game/pkg/source/upgrade/run/config/do` command families, supports open `.mcm` sharing, custom manually imported sources, one-command installer links, and a phased path to full version/loader/Java install and game launch.

**Why this approach:** The current project is a compact Rust CLI, so the safest path is to lock existing behavior with tests, split the single large file into focused modules, then add the share/source HTTP service and launcher features in layers instead of one risky rewrite. The share/source service stays in Rust and can run as `share`, `source`, or `both` behind PM2 on port 8950.

**What it will NOT do:** It will not copy HMCL/PCL code, UI text, assets, or implementation structure. It will not require an admin token or Turnstile for authenticated publishing. It will not silently run install/download/delete/autoremove in ordinary CLI use without either `-y/--yes` or an explicit second confirmation.

**Effort:** XL
**Risk:** High - this is a product/architecture expansion spanning CLI, HTTP service, package/source schemas, installer distribution, downloads, and launcher behavior.
**Decisions to sanity-check:** OIDC-authenticated publish/update/delete instead of anonymous upload; Rust service instead of Node service; manually imported sources are trusted after import; package install links use permanent package names and web install runs with `--yes` semantics.

Your next move: start work with the worker, or request a high-accuracy review first. Full execution detail follows below.

---

> TL;DR (machine): XL/high-risk Rust CLI+service expansion; preserve current behavior semantics, modularize, add command grammar, `.mcm` schema, share/source server modes, OIDC publish/update/delete, curl|bash installers, custom trusted sources, retry-tolerant game/runtime install and launch.

## Scope
### Must have
- Preserve current MCM behavior semantically before expanding, but do not preserve old CLI compatibility. Existing mod-manager behavior moves under `mod`/`mods`; tests should protect behavior, not old command spelling. Preserve mock provider, Modrinth/CurseForge provider behavior, local jar info, lock ownership, filename/download URL safety.
- Refactor from the current single-file architecture into modules before adding large features. `src/lib.rs` currently owns CLI, config, providers, install, state, safety, and tests (`src/lib.rs:16-2530`) and must not keep growing.
- User-facing CLI command taxonomy:
  - Top-level `install [target]` is a low-power package installer, like a restricted `do`. It only accepts a local `.mcm` path or URL plus optional `-y`/`--yes`; it does not accept arbitrary parameters and does not install raw mod names or `mc...` smart targets.
  - If top-level `install` is run without a target, it selects the lexicographically smallest `*.mcm` file in the current directory and installs it. This differs from `do`, which remains the higher-power executor. `install` and `do` both require second confirmation by default, including when auto-selecting a file; `-y/--yes` skips bypassable confirmations.
  - Minecraft/game smart target grammar belongs under `game install` or package contents, not top-level `install`: `mc`, `mc1.21.1`, `mc-neoforge`, `mc1.21.1-neoforge`, `mc1.21.1-neoforge-21.1.172`, and equivalent Fabric/Forge/NeoForge/Quilt forms.
  - `upgrade`: upgrade current/default game only.
  - `full-upgrade`: upgrade all configured games.
  - `source add/remove/info/list`: manually manage sources. No source, including the author source, is preloaded by default.
  - `pkg dl|download/share/install/make/info/list`: package download/share/install/export flows. `dl` is alias for `download`.
  - `config`: interactive global config editor when run alone; also supports non-interactive subcommands/flags if added later.
  - `game default/install/remove/info/rename/config/list`: game/version/instance management; `default` is the English command for the selected/default version.
  - `do [file]`: execute an `.mcm` file; without argument, use exactly one `*.mcm` in the current directory and error if zero or multiple.
  - `run`: launch the default game. First-class Microsoft/Mojang auth support is in scope, but tests must be built around mocks because the user has no paid account for manual validation.
  - Existing mod-manager commands move under both `mod` and `mods` command groups, with both spellings accepted. No old top-level mod-manager command compatibility is required. Top-level `install` is only for `.mcm` paths/URLs and current-directory `.mcm` selection, not Minecraft smart targets.
- Default local root under the user's home: `~/mcm` (platform-appropriate equivalent) containing instances/config/saves/cache/state by default, with configurable paths.
- `.mcm` is JSON, schema-versioned, size-limited, and supports MCM format plus import from standard `.mrpack` and CurseForge manifest packages.
  - Package import/export supports mods, shaders, resource packs, datapacks, selected NBT/structure files, configs, version-scoped config, scripts/actions, and explicitly selected local/private settings/history. Shared public uploads exclude credentials/tokens/secrets and do not include personal settings/history by default.
- Share/source server on the 8950 deployment surface:
  - One Rust service binary/subcommand managed by PM2.
  - Modes: `share`, `source`, `both`/`all`.
  - Server package JSON/blob storage outside `/x`; client/local storage may use normal MCM user-data paths.
  - `share` mode: public download of `.mcm` JSON packages plus authenticated publish/update/delete. Upload has no admin token and no Turnstile after OIDC is added. Publishing uses OIDC login, package limits, body size limit, schema validation, and globally unique case-insensitive package slug. Download is public and convenient, with optional rate/bandwidth limits.
  - `source` mode: serve a manually imported source index and metadata/artifacts. Any computer can run it. Users fully trust a source once they manually import it. A source can declare capabilities such as `mods`, `packages`, `games`, `loaders`, and `java`; client uses sources according to declared capability.
  - `both` mode: enable share and source routes in one process/config.
- Web UI/auth/publish flow:
  - Execution worker must run `npx getdesign@latest add ollama` before frontend design work and load the generated design guidance.
  - A `DESIGN.md` gate must exist before UI pages/components.
  - After authenticated publish, show copyable one-command install snippets.
- CLI/browser Turnstile flow:
  - Superseded for package publishing by OIDC. Turnstile is not required for authenticated publish/update/delete and should not appear in publish/update/delete docs or required env config.
  - CLI OIDC flow prints an auth URL, user logs in in browser, service redirects to `https://mc.dyyapp.com/api/auth/oidc/callback`, and CLI receives/polls a short session result so it can publish/update/delete packages.
  - OIDC provider base URL is `https://auth.dyyapp.com`; redirect URL is `https://mc.dyyapp.com/api/auth/oidc/callback`; client ID/secret values are configured only through environment/secret files and must not be committed to the repository or plan artifacts.
  - Authenticated package author policy: one user can perform at most one publish/update push per day; both new publish and update count as the daily push. Deleting does not count as a push but also does not reset the daily push limit. One user can have at most 5 packages existing at the same time; authenticated users can update, delete, and publish through CLI.
  - Package update model: updates overwrite the current package rather than keeping public version history. Do not retain old package backups on the server after update. Upgrade logic must still account for dependencies and refuse/skip upgrades when dependencies are not satisfied.
  - Package slug ownership: deleting a package reserves its slug for the deleting owner for 2 days, then releases it. Local installs record the package author's user ID. Upgrade refuses and warns if the remote package slug now belongs to a different user ID.
- Installer routes:
  - `https://mc.dyyapp.com/install` returns a bootstrap shell script for installing/updating MCM.
  - `https://mc.dyyapp.com/install/pkg/<package-name>` returns/redirects to a package-install script flow for a permanent, human-readable, unique package name. Web install scripts intentionally run the package flow with yes/non-interactive semantics as requested, so they should not pause for confirmations unless a non-bypassable safety rule is later defined.
  - Shell scripts verify release checksums/signatures or pinned hashes before installing binaries.
  - Package scripts delegate to normal MCM commands, not bespoke shell package logic.
  - First supported bootstrap/install target is Linux x86_64 only. Other OS/arch combinations may be detected but must exit with an explicit unsupported-platform message in the first implementation.
- Complete Minecraft-manager path:
  - Version install model using Mojang version manifests.
  - Loader install model for at least Fabric first, with interfaces ready for Quilt/Forge/NeoForge.
  - Java runtime discovery/install model with compatibility matrix and retry/resume.
  - Launch command builder with dry-run/mock mode first, then real launch.
  - Microsoft/Mojang auth is in scope, but must be testable through mocked provider/session flows because no paid account is available for manual QA.
- Retry/resume tolerance:
  - Downloads use retries with backoff, partial-file validation, staged writes, resume when supported, clear progress/error messages, and no repeated manual clicking for transient failures.
- Trust/confirmation policy:
  - Manually imported sources are trusted by user intent.
  - Schema/hash validation still catches corruption/bugs.
  - Install/download/delete operations require second confirmation by default, except clearly harmless read-only/list/info/dry-run actions.
  - `.mcm` packages may declare scripts/actions. First implementation supports Linux shell scripts only. Scripts run with cwd set to the game version/instance root, not the user's current shell directory. Packages containing scripts require a very strong warning unless `-y/--yes` is supplied. If a script needs root, the script may invoke `sudo` itself; MCM does not wrap script execution in automatic sudo, but the package/action warning should make root risk clear.
  - `autoremove` is MC-critical and must show a strong warning that removing apparently unused mods/resources may break worlds/saves or modded structures, then require second confirmation by default.
  - Dangerous operations from packages/sources, especially script execution, root/system changes, deleting/removing game versions, deleting/overwriting worlds, changing installed mods/resources, download/install actions, `autoremove`, or launch-on-install, require a warning and second confirmation by default in ordinary CLI use.
  - Web `/install/pkg/<package-name>` flows are the exception requested by the user: the generated script may pass `--yes`/non-interactive approval so package install and declared launch can proceed without prompts.
  - If a package installs/creates a game version, it may modify that version's per-version config. If it does not install/create a version, it must not modify existing version config. Higher-power `do` may modify config according to its own schema and confirmations.
  - `-y/--yes` may skip bypassable confirmations for non-interactive use; non-bypassable confirmations must be explicitly named in implementation.
  - If root is needed, interactive mode asks/offers elevation; non-interactive mode prints the exact `sudo`/`pkexec` command instead of failing generically.
- AGPLv3/open-source target:
  - Add AGPLv3 license/compliance docs.
  - Plan for hosted-service source availability.
  - Run dependency license audit.
- License guardrail:
  - HMCL/PCL can be used only as conceptual UX/product references.
  - Do not copy HMCL/PCL code, UI text, assets, icons, strings, or implementation structure.
  - Direct HMCL code reuse is forbidden unless a separate explicit license review accepts GPLv3+extra-term obligations. PCL/PCL2 code/assets are no-copy due custom restricted license.

### Must NOT have (guardrails, anti-slop, scope boundaries)
- Must not start implementation before characterization tests pin current behavior semantics; old CLI spelling compatibility is not required.
- Must not keep adding all features to `src/lib.rs`; files over 250 pure LOC require split or explicit `SIZE_OK` justification for pure data/generated artifacts only.
- Must not add hidden default sources. A fresh install has zero custom sources unless the user imports one.
- Must not require admin token or Turnstile for authenticated package publish/update/delete once OIDC is implemented.
- Must not store server share JSON under `/x`.
- Must not silently execute scripts, start games, install/download packages or runtimes, delete versions, run `autoremove`, overwrite worlds, delete user data, or perform root-required actions without the defined confirmation policy, except the explicit web `/install/pkg/<package-name>` yes-mode flow.
- Must not make `curl | bash` download or execute unverified binaries.
- Must not treat passing unit tests as enough; every major feature needs CLI/API real-surface QA evidence.
- Must not implement moderation/admin dashboards, payment, GUI desktop app, or every loader/version/OS in the first implementation wave unless a later plan explicitly expands scope.
- Must not rely on manual paid-account testing for Microsoft/Mojang auth; use mock provider/session tests and document any remaining real-account validation gap.
- Must not copy or port HMCL/PCL code/assets despite AGPLv3 target.

## Verification strategy
> Zero human intervention - all verification is agent-executed.
- Test decision: TDD for production changes. Rust uses Cargo tests with existing `assert_cmd`, `predicates`, `tempfile`; add focused integration files under `tests/` plus unit/property tests as needed. HTTP service tests use local bound test server and mock OIDC provider/session. Frontend/UI tests include browser screenshots and design QA after UI implementation.
- Baseline commands:
  - `cargo test`
  - `cargo test --lib`
  - `cargo test --test help`
  - `cargo test --test mvp`
  - `cargo run -- --help`
  - `cargo run -- <subcommand> --help` for every new subcommand family.
- Lint/type checks to add before final completion if dependencies allow:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo deny check licenses` or documented equivalent for dependency license audit.
- Evidence: every todo writes command output/logs/screenshots under `.omo/evidence/task-<N>-mcm-minecraft-manager-expansion.<ext>` and names the exact artifact in the worker final report.
- Real-surface QA examples:
  - CLI: `cargo run -- --config-dir <tmp> --state-dir <tmp> --provider mock ...` with asserted stdout/stderr/files.
  - HTTP: `curl` against a locally started service in each mode (`share`, `source`, `both`).
  - Installer: run generated install script in temp prefix with mock release server; verify checksum mismatch aborts.
  - Browser UI: Playwright/Chrome screenshots at 375/768/1280 for auth/publish/install pages after frontend exists.

## Execution strategy
### Parallel execution waves
> Target 5-8 todos per wave. Fewer than 3 (except the final) means you under-split.
- Wave 0: baseline tests, architecture split map, license/tooling gates.
- Wave 1: modular refactor and command grammar skeleton while preserving current behavior.
- Wave 2: domain schemas for games, packages, sources, `.mcm`, config/state/locks, confirmation policy.
- Wave 3: package/source CLI flows and standard package import/export.
- Wave 4: share/source HTTP service modes, storage, OIDC auth, publish/update/delete/download APIs.
- Wave 5: frontend pages, install scripts, permanent package install routes.
- Wave 6: game/version/loader/Java runtime download/install/run dry-run, retry/resume, and upgrade semantics.
- Wave 7: AGPL/compliance docs, deployment docs, full final verification.

### Dependency matrix
| Todo | Depends on | Blocks | Can parallelize with |
| --- | --- | --- | --- |
| 1 | none | 2, 3, all behavior work | none |
| 2 | 1 | 4, 5, 6 | 3 |
| 3 | 1 | 4, 24 | 2 |
| 4 | 2, 3 | 5, 6, 7, 8 | none |
| 5 | 4 | 8, 9, 10 | 6, 7 |
| 6 | 4 | 11, 12 | 5, 7 |
| 7 | 4 | 8, 11, 18 | 5, 6 |
| 8 | 5, 7 | 9, 10, 18 | 11 |
| 9 | 8 | 10, 15, 20 | 12, 13 |
| 10 | 9 | 16, 20 | 12, 13 |
| 11 | 6, 7 | 12, 13, 14 | 8 |
| 12 | 11 | 14, 15 | 9, 10 |
| 13 | 11 | 14, 15 | 9, 10 |
| 14 | 12, 13 | 15, 16 | none |
| 15 | 9, 12, 14 | 16, 17 | none |
| 16 | 10, 15 | 17, 18 | none |
| 17 | 16 | 20 | 18, 19 |
| 18 | 7, 8, 16 | 20, 21 | 17, 19 |
| 19 | 3 | 20 | 17, 18 |
| 20 | 10, 17, 18, 19 | 21, 22 | none |
| 21 | 20 | 22, 23 | none |
| 22 | 21 | 23 | none |
| 23 | 22 | final | 24 |
| 24 | 3 | final | 23 |

## Todos
> Implementation + Test = ONE todo. Never separate.
<!-- APPEND TASK BATCHES BELOW THIS LINE WITH edit/apply_patch - never rewrite the headers above. -->
- [x] 1. Baseline characterization: pin current CLI behavior before refactor
  What to do / Must NOT do: Add or adjust tests only to lock current behavior semantics for profile/search/info/install/list/status/remove/uninstall/autoremove/provider/local jar behavior before restructuring. These tests may later be migrated to new `game` and `mod(s)` command spelling; do not require old top-level compatibility. Do not change production behavior in this todo. Preserve existing isolated temp-dir style.
  Parallelization: Wave 0 | Blocked by: none | Blocks: 2, 3, all production work
  References (executor has NO interview context - be exhaustive): `tests/mvp.rs:6-312`, `tests/help.rs:4-38`, `src/lib.rs:49-91`, `src/lib.rs:360-630`, `README.md:1-69`.
  Acceptance criteria (agent-executable): `cargo test --test mvp`; `cargo test --test help`; `cargo test --lib`; new tests fail if profile/game add/use/list/show semantics, mock install/remove/autoremove/status semantics, local jar info, or provider key behavior is regressed. Later todos may change CLI spelling without preserving old top-level command names.
  QA scenarios (name the exact tool + invocation): Happy: `cargo test --test mvp -- --nocapture` writes `.omo/evidence/task-1-mcm-minecraft-manager-expansion.txt`. Failure: intentionally run `cargo run -- --config-dir $(mktemp -d)/c --state-dir $(mktemp -d)/s list` without profile and capture `no active profile` error in evidence.
  Commit: Y | test(baseline): characterize current CLI behavior

- [x] 2. Split oversized Rust architecture without changing behavior
  What to do / Must NOT do: Refactor `src/lib.rs` into focused modules such as `cli`, `app`, `config`, `profile`, `lock`, `provider`, `install`, `safety`, `jar_info`, and `mock`. Keep semantics stable during refactor, but do not promise old CLI spelling compatibility after the command redesign. Do not add new features yet. No module >250 pure LOC unless pure data/test fixture with explicit reason.
  Parallelization: Wave 1 | Blocked by: 1 | Blocks: 4, 5, 6
  References: `src/lib.rs:16-91`, `src/lib.rs:123-170`, `src/lib.rs:224-228`, `src/lib.rs:463-537`, `src/lib.rs:633-698`, `src/lib.rs:1006-1870`, `src/lib.rs:1872-1930`, `src/lib.rs:1932-2530`.
  Acceptance criteria: `cargo test`; `cargo fmt --check`; `cargo clippy --all-targets --all-features -- -D warnings` or record missing dependency/tooling reason. Pure LOC check for changed Rust files: `awk '!/^[[:space:]]*$/ && !/^[[:space:]]*(\/\/|#)/' <file> | wc -l` all <=250 or documented exception.
  QA scenarios: Happy: run `cargo run -- --provider mock --config-dir <tmp>/c --state-dir <tmp>/s profile add dev --mods-dir <tmp>/mods --mc-version 1.20.1 --loader fabric` and capture `added profile dev`. Failure: run existing `cargo test --test mvp missing_download_url_errors_without_partial_install -- --nocapture` and capture no partial jar. Evidence `.omo/evidence/task-2-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | refactor(core): split CLI manager modules without behavior change

- [x] 3. Add AGPL and dependency/license compliance gates
  What to do / Must NOT do: Add AGPLv3 license file and docs explaining hosted-service source availability. Add dependency license audit tooling/config. Explicitly document HMCL/PCL clean-room rule and no-copy restrictions. Do not copy license text snippets from HMCL/PCL except citations in docs.
  Parallelization: Wave 0/1 | Blocked by: 1 | Blocks: 4, 24
  References: `Cargo.toml:1-23`; launcher research findings in `.omo/drafts/minecraft-manager-expansion.md`; HMCL GPLv3+extra terms and PCL custom license research.
  Acceptance criteria: `LICENSE` is AGPLv3; docs mention AGPL hosted-service obligations; `cargo deny check licenses` or equivalent configured and run; docs state HMCL/PCL are conceptual references only.
  QA scenarios: Happy: run license audit command and capture pass/allowed exceptions. Failure: run a grep proving no new file contains `Plain Craft Launcher` copied source markers except docs citations. Evidence `.omo/evidence/task-3-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | docs(license): add AGPL compliance and clean-room policy

- [x] 4. Define canonical CLI grammar and help skeleton
  What to do / Must NOT do: Add Clap command skeletons and help output for `upgrade`, `full-upgrade`, `source`, `pkg`, `game`, `do`, `run`, `config`, `mod`, `mods`, and low-power top-level `install [mcm-path-or-url]`, plus aliases (`pkg dl` = `download`, `mod` = `mods`). Do not retain `profile` as a `game` alias and do not retain old top-level mod-manager commands for compatibility. Top-level `install` allows only an optional `.mcm` file path/URL plus `-y/--yes`; no raw mod names and no `mc...` smart targets. Define parser grammar for `game install` Minecraft targets before implementation: `mc` means latest vanilla MC; `mc1.21.1` means vanilla MC 1.21.1; `mc-neoforge` means latest MC supporting latest compatible NeoForge; `mc1.21.1-neoforge` means MC 1.21.1 with latest compatible NeoForge; `mc1.21.1-neoforge-21.1.172` means MC 1.21.1 with NeoForge 21.1.172; Fabric/Forge/NeoForge/Quilt all use the same grammar. Do not add `@latest` equivalent extensions because omission already means latest. Stub new behavior with clear “not implemented yet” only where downstream tasks will fill it.
  Parallelization: Wave 1 | Blocked by: 2, 3 | Blocks: 5, 6, 7, 8
  References: `src/lib.rs:16-91`, `tests/help.rs:4-38`, user command list in draft lines `Request summary`.
  Acceptance criteria: `cargo run -- --help` contains all top-level command names including `mod`, `mods`, and `install`; each new command and subcommand `--help` exits 0; `game install` target parser tests classify `mc`, `mc1.21.1`, `mc-neoforge`, `mc1.21.1-neoforge`, `mc1.21.1-neoforge-21.1.172`, and Fabric/Forge/Quilt equivalents exactly; parser rejects `mc1.21.1-neoforge@latest`; top-level `install` rejects options other than `-y/--yes` and rejects non-`.mcm` raw targets like `sodium` or `mc-neoforge`; existing `tests/help.rs` updated; `cargo test --test help` passes.
  QA scenarios: Happy: capture `cargo run -- --help`, `cargo run -- pkg --help`, `cargo run -- mods --help`, `cargo run -- game install --help`, and parser test output for vanilla and loader target forms. Failure: `cargo run -- source unknown-subcommand` exits nonzero with Clap error; invalid `mc1.21.1-neoforge-???`, unsupported `mc1.21.1-neoforge@latest`, `mcm install mc-neoforge`, and `mcm install sample.mcm --extra` error with syntax guidance. Evidence `.omo/evidence/task-4-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(cli): add Minecraft manager command grammar

- [x] 5. Implement typed config model for `~/mcm`, games, paths, and precedence
  What to do / Must NOT do: Introduce typed config/state model separating global config, game records, default game, configurable instance/saves/config/cache paths, and one-way data migration from current `Config { active_profile, profiles }` if old data exists. Implement base `game` management commands: `game default`, `game list`, `game info`, `game rename`, `game config`, and safe `game remove` state handling. Default root is platform home `mcm` folder. Do not keep old `profile` command compatibility and do not delete old profile data.
  Parallelization: Wave 2 | Blocked by: 4 | Blocks: 8, 9, 10
  References: `src/lib.rs:123-136`, `src/lib.rs:272-332`, `README.md:7-21`.
  Acceptance criteria: Unit tests parse old config and new config; integration test creates default root under temp home/config override; `game default` shows/sets default game; `game list` prints all games and marks default; `game info <name>` prints root/version/loader/config paths; `game rename old new` updates config and default pointer when applicable without touching unrelated games; `game config <name>` can show and set version-scoped configurable fields; `game remove <name>` requires confirmation via policy and removes only MCM-owned game metadata/files according to the eventual remove implementation; old profile data migrates to game records, but old `profile` command compatibility is not required.
  QA scenarios: Happy: create two games in temp config, set default, list/info, rename default game, set a version config field, and verify state. Failure: config with missing default game errors with actionable message; `game rename missing new` and `game config missing` fail without mutating config; `game remove` without confirmation fails. Evidence `.omo/evidence/task-5-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(config): introduce game root and path model

- [x] 6. Define `.mcm` package schema and parser boundary
  What to do / Must NOT do: Add schema-versioned Rust types for `.mcm` packages: identity/name, version, description, game version, loader, dependencies, mods, shaderpacks, resourcepacks, datapacks, saves/NBT/structures, configs, optional actions, optional launch request, and explicit local/private settings/history. Parse untrusted JSON once into typed values. Enforce package-name normalization and reserved names. Do not pass raw `serde_json::Value` inside domain logic.
  Parallelization: Wave 2 | Blocked by: 4 | Blocks: 11, 12
  References: `Cargo.toml:13-18`, `src/lib.rs:951-959`, Minecraft format research in draft; Modrinth `.mrpack` and CurseForge manifest notes.
  Acceptance criteria: Unit tests for valid package, unknown schema version, duplicate/invalid package names, secret/token field rejection, size/depth limits. `cargo test package_schema` passes.
  QA scenarios: Happy: CLI `mcm pkg info ./valid.mcm` prints normalized package name. Failure: `mcm pkg info ./evil.mcm` with `../` path or token field exits nonzero. Evidence `.omo/evidence/task-6-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(pkg): add typed mcm package schema

- [x] 7. Implement confirmation and trusted-source safety policy
  What to do / Must NOT do: Centralize confirmation policy for operations. Imported sources are trusted, but install/download/delete operations, version removal, package install, runtime install, source-provided actions, script execution, root/system changes, world overwrite/delete, `autoremove`, and launch-on-install require second confirmation unless bypassable and `-y/--yes` is supplied. `autoremove` must be classified as MC-critical and warn that it can break worlds/saves/modded structures. Define harmless actions (read-only/list/info/dry-run/help) that do not need confirmation, and define non-bypassable actions if any. Add root-required actionable escalation helper. Do not scatter ad-hoc prompts.
  Parallelization: Wave 2 | Blocked by: 4 | Blocks: 8, 11, 18
  References: `src/lib.rs:714-720`, `src/lib.rs:586-630`, user clarification on trusted imported sources and `-y/--yes`.
  Acceptance criteria: Unit tests cover bypassable confirmation, `--yes`, non-TTY failure, second confirmation, root escalation message, no-confirmation read-only actions, install/download confirmation, delete version confirmation, and `autoremove` critical warning text. Existing install/remove/autoremove confirmations route through policy.
  QA scenarios: Happy: package install/download prompt succeeds after typed confirmation; package with launch request prompts and succeeds after typed confirmation; `autoremove` shows MC-critical warning and succeeds only after second confirmation. Failure: same operations in non-interactive mode without `--yes` exit with confirmation-required message; read-only `info/list/help` never prompt. Evidence `.omo/evidence/task-7-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(safety): centralize trusted-source confirmations

- [x] 8. Implement source config CLI and no-default-source invariant
  What to do / Must NOT do: Implement `source add/remove/info/list`. Store manually imported source URL/file, display metadata, support trust confirmation at add time, and guarantee fresh install has no custom source. Do not preinstall author source. Current `--provider` Modrinth/CurseForge global flag may remain for existing mod search but should be separate from custom sources.
  Parallelization: Wave 3 | Blocked by: 5, 7 | Blocks: 9, 10, 18
  References: `src/lib.rs:28-47`, `src/lib.rs:351-358`, user custom source requirements.
  Acceptance criteria: `mcm source list` is empty on fresh config; `mcm source add https://example.test/index.json` requires confirmation unless `--yes`; `source info` prints trusted/manual status; `source remove` removes it.
  QA scenarios: Happy: add/list/info/remove source using temp config and capture outputs. Failure: adding same source twice returns conflict/duplicate message. Evidence `.omo/evidence/task-8-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(source): manage manually trusted custom sources

- [x] 9. Implement source index format and provider adapter
  What to do / Must NOT do: Define versioned source index format for packages/projects/artifacts: source identity, declared capabilities (`mods`, `packages`, `games`, `loaders`, `java`), package/project ID, versions, hashes, sizes, compatibility, download URLs/mirrors, optional local blob references for sources that host actual files, and optional declared dangerous actions. Add provider adapter so custom source can participate in package/mod/game/loader/runtime resolution according to capability. Support both index-only external URLs and direct source-hosted artifact blobs, always with hash verification. Do not auto-execute actions from index metadata.
  Parallelization: Wave 3 | Blocked by: 8 | Blocks: 10, 15, 20
  References: `src/lib.rs:172-228`, `src/lib.rs:740-757`, `src/lib.rs:885-917`, source service requirements.
  Acceptance criteria: Unit tests parse valid/invalid index; integration test serves local source index and `mcm source info` / package lookup resolves it; both external URL and source-hosted blob artifact entries resolve; hash/size metadata is preserved.
  QA scenarios: Happy: local test HTTP source returns package metadata and CLI resolves it. Failure: malformed index exits with schema error and does not mutate config. Evidence `.omo/evidence/task-9-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(source): add custom source index adapter

- [x] 10. Implement package install/download/make/share CLI core
  What to do / Must NOT do: Implement `pkg info/install/download|dl/make/share` core around `.mcm` schema and source/share URLs. Package install/download requires second confirmation by default unless `--yes`; harmless `info/list` does not. Top-level `install` and `do` also require confirmation by default. Package install reuses current install planning for mods and extends to resource/shader/config assets. `.mcm` may contain Linux shell scripts/actions; scripts run from the game version/instance root. If scripts are present, show a strong warning unless `--yes`; scripts that need root may call `sudo` themselves. If a package installs/creates a game version, it may modify only that version's per-version config; if it does not install/create a version, it must not modify existing version config. `pkg make` defaults to excluding secrets/personal settings/history; explicit flags required for local/private export. `pkg share` initiates OIDC-authenticated publish/update flow with local/mock service first.
  Parallelization: Wave 3 | Blocked by: 9 | Blocks: 16, 20
  References: `src/lib.rs:463-537`, `src/lib.rs:633-698`, `src/lib.rs:951-959`, `tests/mvp.rs:121-225`.
  Acceptance criteria: Integration tests install a local `.mcm` containing mock mods and shader/resource pack assets into temp game root; install/download/do without `--yes` prompts; no-target top-level `install` selects lexicographically smallest `.mcm` then prompts; script-containing package warns strongly unless `--yes`; script cwd is the game version/instance root; package that creates a version may modify that version config; package that does not create/install a version is rejected if it attempts version config changes; `pkg make` creates valid JSON; `pkg dl` alias matches `download`; secrets are excluded by default.
  QA scenarios: Happy: `mcm pkg install ./sample.mcm --yes` writes expected files and lock entries; interactive install succeeds after confirmation; version-creating package updates its own version config. Failure: duplicate/unsafe asset path in `.mcm` aborts without partial install; non-interactive install/download without `--yes` exits with confirmation-required message; script package without confirmation refuses; non-version package attempting config changes is rejected. Evidence `.omo/evidence/task-10-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(pkg): install and create mcm packages

- [x] 11. Import/export standard modpack formats
  What to do / Must NOT do: Add import/export support for Modrinth `.mrpack` first and CurseForge manifest import/export-compatible output second. Treat resourcepacks/shaderpacks/saves/config/datapacks as opaque assets with safe path checks. Do not parse or rewrite shader GLSL/NBT internals except safe copying/metadata detection.
  Parallelization: Wave 3 | Blocked by: 6, 7 | Blocks: 12, 13, 14
  References: Minecraft format research in draft; existing `zip` dependency `Cargo.toml:18`; `src/lib.rs:635-698`; `src/lib.rs:1872-1930`.
  Acceptance criteria: Tests import `.mrpack` with `modrinth.index.json`, hashes, overrides; tests import CurseForge `manifest.json` + `overrides`; path traversal/zip bomb-ish oversized archive rejected by limits.
  QA scenarios: Happy: `mcm pkg install ./sample.mrpack --yes --provider mock` installs declared assets. Failure: archive containing `../evil` aborts with no writes. Evidence `.omo/evidence/task-11-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(pkg): import standard modpack formats

- [x] 12. Build Rust HTTP service shell with share/source/both modes
  What to do / Must NOT do: Add Rust HTTP service subcommand/binary, preferably Axum-style stack, configurable listen address defaulting to `127.0.0.1:8950`, PM2-friendly logs/env config, and modes `share`, `source`, `both`. Keep source and share route sets independently enabled/disabled by mode. Do not bind public `0.0.0.0` by default.
  Parallelization: Wave 4 | Blocked by: 11 | Blocks: 14, 15
  References: `Cargo.toml:7-18`, no current server surface finding, Turnstile/share research.
  Acceptance criteria: Local service starts in each mode; `share` mode returns 404/disabled for source routes; `source` mode returns disabled for share upload routes; `both` serves both; tests use random local port.
  QA scenarios: Happy: `curl http://127.0.0.1:<port>/health` returns JSON with mode. Failure: `curl /api/source/index` in share-only mode returns documented disabled status. Evidence `.omo/evidence/task-12-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(service): add share and source server modes

- [x] 13. Implement server storage outside `/x`
  What to do / Must NOT do: Add SQLite metadata plus filesystem JSON/blob storage for packages/sources under configurable server data directory defaulting outside `/x` (e.g. `/var/lib/mcm-share` or user-specified local data dir for dev). Enforce package name uniqueness, normalization, reserved names, ownership, 2-day slug reservation after delete, overwrite-on-update current package storage with no retained old-package backup, and conflict responses. Redis optional for rate/auth state if configured; no MySQL first wave.
  Parallelization: Wave 4 | Blocked by: 11 | Blocks: 14, 15
  References: user `/x` clarification; Turnstile/share storage research; `src/lib.rs:293-349` for existing config/state path patterns.
  Acceptance criteria: Storage init refuses default path under `/x`; upload of duplicate package returns 409; update overwrites current package and does not retain old package backup; delete reserves slug for owner for 2 days; metadata persists across service restart; tests run with temp data dir.
  QA scenarios: Happy: start service with `MCM_SHARE_DATA_DIR=<tmp>/srv`, publish package, update it, restart, download updated metadata and confirm old content is not available. Failure: start with data dir `/x/mcm-share` returns clear configuration error; different user cannot claim deleted slug until reservation expires. Evidence `.omo/evidence/task-13-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(service): add durable share storage

- [x] 14. Implement OIDC auth and package publish/update/delete API
  What to do / Must NOT do: Add OIDC login/session endpoints for CLI/browser auth using provider base URL `https://auth.dyyapp.com` and callback `https://mc.dyyapp.com/api/auth/oidc/callback`. Add mock OIDC provider/session for tests. Implement authenticated package publish/update/delete without admin token and without Turnstile, with content-type/body-size/schema limits, globally unique case-insensitive slugs, per-user one-publish-or-update-push-per-day limit, max 5 existing packages per user, and audit logs. Delete does not count as a push but does not reset the daily push limit. Do not commit or log OIDC client secrets/tokens.
  Parallelization: Wave 4 | Blocked by: 12, 13 | Blocks: 15, 16
  References: user OIDC details; user no-admin-token/no-Turnstile clarification; `src/lib.rs:674-698` for URL safety style.
  Acceptance criteria: Publish with valid mock OIDC session succeeds; publish without login fails; update overwrites the current package; update/delete by package owner succeeds; update/delete by another user fails; duplicate slug returns 409; second publish or update push by same user on same day fails; deleting a package does not reset the daily push limit; sixth simultaneous package fails; oversized body returns 413; non-JSON returns 415; no admin token or Turnstile required anywhere in publish/update/delete.
  QA scenarios: Happy: mock OIDC login, publish `.mcm`, then in a separate test day/window update it and verify latest download is overwritten content, download it, delete it. Failure: unauthenticated publish fails; same-day publish+update fails; second same-day publish fails even after delete; sixth existing package fails; deleted slug cannot be claimed by another user for 2 days. Evidence `.omo/evidence/task-14-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(share): add OIDC package publishing

- [x] 15. Implement source service routes and client integration
  What to do / Must NOT do: Add source-mode HTTP routes for source metadata/index/artifacts and client fetch/cache integration. Source mode supports both index-only external URLs and direct artifact/blob hosting. Imported sources are trusted, but schema/hash/size verification still runs. Add source service docs showing any computer can serve a source. Do not preconfigure source URLs.
  Parallelization: Wave 4 | Blocked by: 9, 12, 14 | Blocks: 16, 17
  References: source requirements; source index Todo 9; server mode Todo 12.
  Acceptance criteria: Service in source mode serves index; service can host direct artifact/blob downloads; `mcm source add <local-service-url> --yes` then `mcm pkg install <name> --yes` resolves package from that source; external URL artifact and direct-hosted artifact both install; corrupted hash aborts.
  QA scenarios: Happy: local source service package install succeeds from both external URL fixture and service-hosted blob fixture. Failure: hash mismatch from trusted source aborts as corruption/integrity error, not hostile-source warning. Evidence `.omo/evidence/task-15-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(source): serve and consume custom sources

- [x] 16. Build upload/install web UI with getdesign and real browser QA
  What to do / Must NOT do: Before UI work, run `npx getdesign@latest add ollama`, load the generated guidance, and create/read `DESIGN.md`. Build minimal polished web pages for login, publish/update/delete package, package detail, copyable install commands, errors, loading, empty states. No emojis as icons. Do not create UI before design gate.
  Parallelization: Wave 5 | Blocked by: 10, 14, 15 | Blocks: 17, 18
  References: frontend `design/README.md`, `taste-skill.md`, `perfection/README.md`; user getdesign instruction.
  Acceptance criteria: UI can log in via mock OIDC, publish/update/delete a valid `.mcm` subject to the one-push-per-day rule, and shows `curl -fsSL https://mc.dyyapp.com/install | bash` plus package install command. Browser screenshots at 375/768/1280 captured. Design tokens trace to `DESIGN.md`.
  QA scenarios: Happy: Playwright opens publish page, completes mock login, publishes sample, sees copy buttons, updates package in an allowed separate test day/window, deletes package. Failure: duplicate package name or daily publish/update push limit displays intentional error state. Evidence `.omo/evidence/task-16-mcm-minecraft-manager-expansion.png` and `.txt`.
  Commit: Y | feat(web): add share auth publish and install UI

- [x] 17. Implement `/install` bootstrap script route
  What to do / Must NOT do: Add server route returning POSIX shell bootstrap for installing/updating MCM. Script supports Linux x86_64 first; other OS/arch detections exit with explicit unsupported-platform message. Script installs to user-writable default, verifies checksum/signature/pinned hash, offers exact sudo command only if system path selected, supports dry-run/preview. Do not pipe unverified binary execution.
  Parallelization: Wave 5 | Blocked by: 16 | Blocks: 20
  References: user curl install requirement; Turnstile/share research deployment notes.
  Acceptance criteria: `curl -fsSL http://127.0.0.1:<port>/install` returns shell; running it with temp `MCM_INSTALL_PREFIX` installs mock/release Linux x86_64 binary; checksum mismatch aborts; unsupported OS/arch exits nonzero with explicit message.
  QA scenarios: Happy: run script in temp prefix against mock release endpoint and capture installed `mcm --version`. Failure: tampered checksum causes abort before install. Evidence `.omo/evidence/task-17-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(install): add verified bootstrap script route

- [x] 18. Implement `/install/pkg/<package-name>` permanent package install route
  What to do / Must NOT do: Add route for permanent unique package-name install flow. Generated script ensures MCM is installed, then delegates to the low-power top-level `mcm install <downloaded-or-url .mcm> --yes` or equivalent package install yes-mode. Validate/safely quote package names. Include script preview/dry-run. Launch game if package declares it, following the requested web yes-mode behavior.
  Parallelization: Wave 5 | Blocked by: 7, 8, 16 | Blocks: 20, 21
  References: user permanent route requirement; package/schema Todo 6; safety Todo 7.
  Acceptance criteria: `curl -fsSL /install/pkg/sample | bash` in temp environment installs sample via normal MCM command with yes-mode; malicious package name cannot inject shell; missing package returns 404; script preview works.
  QA scenarios: Happy: package route installs sample `.mcm` through CLI with evidence. Failure: package name containing `;rm -rf` or spaces/shell metacharacters is rejected or safely encoded and cannot execute. Evidence `.omo/evidence/task-18-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(install): add permanent package installer links

- [x] 19. Implement retry/resume download engine
  What to do / Must NOT do: Replace direct one-shot downloads with reusable download engine: backoff retries, range/resume where supported, partial temp files, hash/size validation, staged atomic finalize, progress events, clear final error. Apply to mod jars, packages, Java/runtime downloads, and installer assets. Do not require repeated user confirmation for each retry.
  Parallelization: Wave 6 | Blocked by: 3 | Blocks: 20
  References: `src/lib.rs:463-537`, `src/lib.rs:1399-1411`, `src/lib.rs:1705-1715`, `src/lib.rs:992-1004`, user HMCL/PCL failure experience.
  Acceptance criteria: Unit/integration tests simulate transient failures then success; resume partial download; hash mismatch removes/quarantines partial file; no partial installed artifact on final failure.
  QA scenarios: Happy: local flaky HTTP server fails first N requests and final install succeeds. Failure: permanent hash mismatch aborts with no finalized file. Evidence `.omo/evidence/task-19-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(download): add retryable resumable downloads

- [x] 20. Implement Minecraft version and loader install model
  What to do / Must NOT do: Add game version install domain using Mojang version manifest and mock manifest for tests. Game version install and version removal require second confirmation by default unless `--yes`; info/list/dry-run do not. Support smart install targets under `game install`: `mc` selects latest vanilla MC; `mc1.21.1` selects vanilla MC 1.21.1; `mc-neoforge` selects latest MC version supporting NeoForge and latest compatible NeoForge; `mc1.21.1-neoforge` selects MC 1.21.1 and latest compatible NeoForge; `mc1.21.1-neoforge-21.1.172` selects MC 1.21.1 and NeoForge 21.1.172; Fabric/Forge/Quilt follow the same grammar. Do not support `@latest` forms. Support release versions first; snapshots optional only if explicitly flagged. Add Fabric, Forge, NeoForge, and Quilt install interfaces, with at least Fabric+NeoForge implemented if scope must be staged. Use retry engine. Microsoft auth is handled in run/auth todo, not here.
  Parallelization: Wave 6 | Blocked by: 9, 10, 17, 18, 19 | Blocks: 21, 22
  References: Minecraft launcher version research; `src/lib.rs:885-917`; HMCL/PCL conceptual runtime/version research.
  Acceptance criteria: `mcm game install dev --mc-version 1.20.1 --loader fabric --provider mock --yes` creates instance metadata and required mock files; `mcm game install mc --yes --dry-run` resolves latest vanilla mock MC; `mcm game install mc1.21.1 --yes --dry-run` resolves vanilla MC 1.21.1; `mcm game install mc-neoforge --yes --dry-run` resolves latest compatible mock MC+NeoForge pair; `mcm game install mc1.21.1-neoforge --yes --dry-run` resolves MC 1.21.1 plus latest compatible mock NeoForge; `mcm game install mc1.21.1-neoforge-21.1.172 --yes --dry-run` resolves exact mock NeoForge 21.1.172; Fabric/Forge/Quilt parser/resolution tests exist; top-level `mcm install mc-neoforge` is rejected; install without `--yes` prompts; game/version remove prompts and warns before deleting; invalid version errors; loader compatibility recorded.
  QA scenarios: Happy: mock version/loader install creates expected files under temp `~/mcm`; smart NeoForge target dry-runs print resolved MC and NeoForge versions; pinned NeoForge dry-run prints exact pinned loader version; version remove succeeds only after confirmation or `--yes`. Failure: unsupported loader/version combination exits with actionable error; `@latest` syntax is rejected; non-interactive install/remove without `--yes` exits with confirmation-required message. Evidence `.omo/evidence/task-20-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(game): install Minecraft versions and loaders

- [x] 21. Implement Java runtime discovery/install and compatibility matrix
  What to do / Must NOT do: Add Java runtime model: discover user/system/managed Java, compatibility matrix by Minecraft version, install managed runtime under MCM root when configured/needed, and require second confirmation for Java/runtime downloads/installs unless `--yes`; root escalation only when installing system-wide. Use retry engine. Do not copy HMCL/PCL code; use Mojang/runtime docs and clean-room logic.
  Parallelization: Wave 6 | Blocked by: 18, 20 | Blocks: 22, 23
  References: HMCL/PCL conceptual Java research; root escalation requirement; `src/lib.rs:272-291` for directories.
  Acceptance criteria: Tests select Java 8/17/21 based on mock MC version matrix; missing Java produces install plan or actionable error; managed Java install uses temp root, verifies hash, and requires confirmation unless `--yes`.
  QA scenarios: Happy: mock Java runtime download/install selected for MC 1.20.1 after confirmation or with `--yes`. Failure: root-required install in non-interactive mode prints exact sudo/pkexec command and exits nonzero; runtime install without confirmation exits with confirmation-required message. Evidence `.omo/evidence/task-21-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(runtime): manage compatible Java runtimes

- [x] 22. Implement launch command builder and `run` dry-run/real boundary
  What to do / Must NOT do: Add launch command builder with structured stages: precheck, Java selection, auth/session selection, files complete, args build, natives/classpath, optional package launch action confirmation, process start. Implement Microsoft/Mojang auth support with mockable provider/session tests; because no paid account is available, do not rely on manual real-account validation. First ensure dry-run/mock mode emits exact command without executing real Minecraft; real launch behind explicit path/config.
  Parallelization: Wave 6 | Blocked by: 20, 21 | Blocks: 23
  References: HMCL/PCL conceptual launch pipeline research; `src/lib.rs:321-332`; user `run` requirement.
  Acceptance criteria: `mcm run --dry-run` for default game prints stable launch command; mock Microsoft auth/session flow contributes expected auth placeholders/arguments; missing game/runtime/auth errors are actionable; package-requested launch requires confirmation unless `--yes` and allowed.
  QA scenarios: Happy: dry-run launch for mock installed game emits Java path, classpath, main class/game args, and mock auth fields. Failure: package install with launch request but no confirmation refuses to launch; missing/expired mock auth session errors without needing a real paid account. Evidence `.omo/evidence/task-22-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(run): build and validate launch commands

- [x] 23. Implement upgrade/full-upgrade semantics
  What to do / Must NOT do: Implement `upgrade` for current/default game and `full-upgrade` for all configured games. Define upgrade scope: mods/packages/loaders/runtime according to lock state and source/provider metadata. Upgrade actions require second confirmation unless `--yes`; dry-run by default if destructive/major changes; confirmation policy applies. Package upgrades must verify the local recorded package author user ID matches the remote package owner ID; if owner differs, warn and refuse. Package updates overwrite current package on the server, but client upgrades must still check dependency constraints; if dependencies are not satisfied, skip/refuse upgrade rather than partially upgrading. `autoremove` must show a severe MC-specific warning and require second confirmation because removing mods/resources can break worlds/saves. Do not auto-overwrite worlds/saves.
  Parallelization: Wave 6 | Blocked by: 22 | Blocks: final
  References: `src/lib.rs:768-917`, `src/lib.rs:539-630`, user `upgrade/full-upgrade` definition.
  Acceptance criteria: Mock tests upgrade one game only; full-upgrade iterates all games; upgrade without `--yes` prompts; `autoremove` warning includes “may break worlds/saves” or equivalent; locked manual/auto reasons preserved; incompatible or dependency-unsatisfied updates are reported and skipped/refused; owner-ID mismatch is reported and refused; dry-run prints plan.
  QA scenarios: Happy: two temp games installed with old mock package versions and satisfied dependencies; `full-upgrade --yes` upgrades both; `autoremove` succeeds only after critical warning confirmation. Failure: incompatible/dependency-unsatisfied update returns warning/skip without corrupting installed game; remote owner ID differs from local recorded author ID and upgrade refuses; non-interactive upgrade/autoremove without `--yes` exits with confirmation-required message. Evidence `.omo/evidence/task-23-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | feat(upgrade): add game and full upgrade flows

- [x] 24. Write deployment, operations, and user docs
  What to do / Must NOT do: Update README/docs for new CLI grammar, `.mcm` schema, share/source server modes, PM2 deployment, OIDC env names (no secret values), data dir outside `/x`, one-command install routes, custom source import trust model, confirmation policy, AGPL source availability, and HMCL/PCL clean-room note. Do not include real secrets or passwords. Do not document Turnstile as required for publish/update/delete.
  Parallelization: Wave 7 | Blocked by: 3 | Blocks: final
  References: `README.md:1-69`, user deployment/domain notes, OIDC/share requirements, share/source research.
  Acceptance criteria: Docs contain runnable examples for `source add`, `pkg share`/publish login, `curl -fsSL https://mc.dyyapp.com/install | bash`, `curl -fsSL https://mc.dyyapp.com/install/pkg/<name> | bash`, PM2 mode config, OIDC env config names without secret values, no-admin-token/no-Turnstile publish policy, daily publish limit, and max 5 package limit.
  QA scenarios: Happy: copy README example commands into a temp/mock environment and run smoke subset. Failure: grep docs and repo for the provided MySQL password/Turnstile secret-like token values; none appear. Evidence `.omo/evidence/task-24-mcm-minecraft-manager-expansion.txt`.
  Commit: Y | docs(manager): document MCM manager workflows

## Final verification wave
> Runs in parallel after ALL todos. ALL must APPROVE. Surface results and wait for the user's explicit okay before declaring complete.
- [x] F1. Plan compliance audit: read this plan and final diff; verify every Must Have is implemented or explicitly deferred by a user-approved follow-up; verify every Must NOT Have is respected; output `.omo/evidence/f1-plan-compliance-mcm-minecraft-manager-expansion.md`.
- [x] F2. Code quality review: run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, pure LOC check on changed Rust files, and review type-boundary/parser/confirmation/download code for unwrap/expect/as/oversized modules; output `.omo/evidence/f2-code-quality-mcm-minecraft-manager-expansion.md`.
- [x] F3. Real manual QA: run end-to-end CLI and HTTP flows in temp dirs: current MVP smoke, source add/install, OIDC mock login publish/update/delete/download, daily publish/update push limit and max-package limit failures, delete-not-resetting-daily-limit check, `/install`, `/install/pkg/sample`, game install, Java mock install, run dry-run, upgrade/full-upgrade. Include exact commands and outputs in `.omo/evidence/f3-real-qa-mcm-minecraft-manager-expansion.txt`.
- [x] F4. Scope fidelity/security/license: verify publish has no admin token/Turnstile, server storage default is outside `/x`, fresh install has no custom source, imported sources are trusted with confirmation policy, `curl|bash` verifies artifacts, no HMCL/PCL code/assets copied, no OIDC secrets or provided secret-like values written, AGPL/license docs exist. Output `.omo/evidence/f4-scope-security-license-mcm-minecraft-manager-expansion.md`.

## Commit strategy
- Use atomic commits per todo or tightly coupled todo group. Keep implementation and direct tests in the same commit.
- Suggested sequence:
  1. `test(baseline): characterize current CLI behavior`
  2. `refactor(core): split manager modules`
  3. `docs(license): add AGPL compliance policy`
  4. `feat(cli): add manager command grammar`
  5. `feat(config): add game root model`
  6. `feat(pkg): add package schema and install flows`
  7. `feat(source): add trusted custom sources`
  8. `feat(service): add share/source server modes`
9. `feat(share): add OIDC package publishing`
  10. `feat(web): add auth publish and install UI`
  11. `feat(install): add bootstrap and package install routes`
  12. `feat(download): add retryable downloads`
  13. `feat(game): add version loader runtime run flows`
  14. `feat(upgrade): add upgrade flows`
  15. `docs(manager): document workflows`
- Before each commit, inspect `git status --short`, `git diff --stat`, and staged diff. Do not stage unrelated workspace files. Do not commit secrets.
- If hooks/tests fail, fix in a new commit attempt; do not bypass hooks.

## Success criteria
- Existing MVP behavior is preserved or intentionally migrated to new command spelling; old top-level command compatibility is not required.
- New command grammar appears in help and has integration tests.
- `~/mcm`/config/game model works with temp dirs and preserves/migrates existing profile behavior.
- `.mcm` packages parse through typed schema, reject unsafe content, and install assets with lock/state ownership.
- Modrinth `.mrpack` and CurseForge manifest import paths have tests.
- Share/source service runs in `share`, `source`, and `both` modes with correct route enablement.
- Publish/update/delete requires OIDC login, no admin token, no Turnstile, rate/size/schema limits, daily publish-or-update push limit, delete not resetting that limit, max 5 existing packages per user, and unique case-insensitive package slug.
- Server package storage defaults outside `/x` and refuses unsafe `/x` default configuration.
- Custom sources are manually imported, trusted after import, and never default-bundled.
- OIDC CLI/browser login has mock-provider tests for success, missing login, wrong user, callback/session expiry, and token secrecy.
- `/install` and `/install/pkg/<package-name>` scripts verify downloaded artifacts and delegate package logic to MCM.
- Downloads are retryable/resumable and no partial corrupt files are finalized.
- Game install, Java runtime selection/install, launch dry-run, `upgrade`, and `full-upgrade` are tested with mock/deterministic fixtures.
- Install/download/delete/version-remove/upgrade/autoremove and other dangerous actions require second confirmation unless `-y/--yes` and policy allow bypass; harmless read-only/list/info/dry-run/help actions do not prompt.
- `autoremove` specifically warns that it is dangerous for Minecraft and may break worlds/saves/modded structures before it can proceed.
- AGPLv3/license docs and dependency license audit are present.
- Frontend pages have `DESIGN.md`, getdesign/ollama integration step recorded, browser screenshots, and loading/empty/error states.
- Final verification wave F1-F4 all approve before declaring implementation complete.
