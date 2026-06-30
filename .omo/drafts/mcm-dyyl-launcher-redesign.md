# mcm-dyyl-launcher-redesign draft

status: awaiting-approval
intent: CLEAR
pending_action: write `.omo/plans/mcm-dyyl-launcher-redesign.md` after approval

## Components ledger

1. real-launcher-core — replace mock game/runtime/auth/launch with real install + launch path comparable to HMCL/PCL core; evidence: `src/run_cmd.rs`, `src/launch.rs`, `src/game_install.rs`, `src/runtime.rs`, `/mnt/.../mcm` exploration.
2. dyyl-runtime-integration — MCM uses external `/x/dyyl`/dyyl command and auto-installs/updates it during MCM install/update; evidence: user decision + `/x/dyyl/Cargo.toml`, `/x/dyyl/dyyl-api-reference.md`.
3. mcm-package-v2 — incompatible `.mcm` redesign: DYyl source authoring + compiled lock/install manifest that freezes resolved mod/dependency versions except `mcm.mod.install.client` / `mcm.pkg.install.client`; evidence: `src/mcm_package.rs`, `src/pkg_cmd.rs`, `src/pkg_install.rs`.
4. permission-model — `mcm install` executes only install-permitted DYyl commands after stripping denied commands; `mcm do` can execute all DYyl/MCM commands after confirmation; upload validates install-permission compatibility; evidence: `src/confirmation.rs`, `src/server/share.rs`, `src/server/storage/helpers.rs`, `/x/dyyl/src/runtime/cmd/dispatch.rs`.
5. cli-share-parity — Web remains only share management + YY-ID login; CLI must expose same management functions except browser login entry; evidence: user decision + `web/app.js`, `src/server/share.rs`, `src/pkg_cmd.rs`.
6. version-root-semantics — all DYyl/MCM commands execute relative to selected version directory; `file.read`/`file.write` use version directory as root; evidence: `src/game_model.rs`, `src/pkg_install.rs`, `/x/dyyl/src/runtime/cmd/file.rs` currently requires absolute paths.

## Decisions recorded

- No desktop GUI in first-stage plan; do not plan a Tauri/Egui/desktop shell now.
- Web install bypassing interaction is intentional; preserve it.
- Do not preserve backward compatibility for old JSON `.mcm` schema.
- DYyl integration is external-command based, not embedded Rust runtime, but MCM install/update must install/update DYyl automatically.
- Normal build/export freezes resolved mod versions and dependencies into compiled `.mcm`; only client-install commands resolve at install time.
- Need add missing `mcm.user.config 设置项, 值` API.
- `.mcm` v2 compiled artifact is a pure JSON lock file; editable source is `.dyyl`.
- Top-level commands: `mcm make <out.dyyl>` exports current instance to DYyl source; `mcm build <in.dyyl>` compiles DYyl source to locked `.mcm` JSON.
- `mcm install` silently strips commands outside install permission and continues.
- Terminology: language name is `dyyl` (lowercase), not `DYyl`.
- Compiled `.mcm` JSON lock does not retain original `.dyyl` source by default.
- `mcm.user.config <key>, <value>` writes to global user config table in `config.toml`; precedence: lower than current-version config, higher than built-in defaults.
- Install-permission model: `mcm.*` mostly allowed when it affects only the selected/current version; prohibited MCM commands include user/global source mutation (`mcm.user.source.*` per user wording / likely `mcm.source.user.*` per current dyyl docs), `mcm.fullupgrade`, `mcm.selfupgrade`, `mcm.do`; `io.*`, `user.id`, `user.name`, `logic.*`, `math.*`, `time.*`, `file.*`, and network fetch/download are allowed, with file/net paths rooted or policy-filtered by the selected game version directory.
- Account scope: first batch must include both offline launch and Microsoft/Mojang online auth; default launch mode is offline, switching to online must be easy.
- Current YY-ID/share login reality: existing MCM server auth is mock OIDC only (`/api/auth/oidc/start`, `/callback`, `/session` in `src/server/auth.rs` route to `src/server/auth/mock.rs`). It does not implement real YY-ID / Casdoor / OIDC provider exchange despite README env names.
- MCM server is now in scope. Live check on 2026-06-28: `pm2 status` shows `mcm` online, pid `693822`, args `serve --mode share --bind 0.0.0.0:8950`; `curl http://127.0.0.1:8950/health` returns `200 {"mode":"share","status":"ok"}`; `/api/share/list` returns `200 {"packages":[]}`; `/api/auth/oidc/start` returns mock auth URL and `mock_user`. Homepage and static files still return 404 because PM2 `exec cwd` is `/mnt/.../nas/lucky/dyyl` (`PWD=/x/dyyl`), while server serves relative `web/index.html`, `web/app.js`, `web/styles.css`; those files exist under the MCM repo cwd, not the dyyl cwd. Plan must include making web asset serving deployment-safe and replacing mock auth with real YY-ID/Casdoor/OIDC login.
- OIDC deployment values: issuer is `https://auth.dyyapp.com`; redirect/callback URL is `https://mc.dyyapp.com/api/auth/oidc/callback`; user provided a client id and client secret in chat on 2026-06-28. Do NOT commit or write the secret value to repo artifacts; plan must name the exact env vars (`MCM_OIDC_ISSUER`, `MCM_OIDC_CLIENT_ID`, `MCM_OIDC_CLIENT_SECRET`, and if code adds it `MCM_OIDC_REDIRECT_URL`) and require PM2/secret-file injection plus a deployment verification that env is present without printing the secret.
- Share management scope is explicitly both CLI and Web: CLI must support package share management (`pkg share`/publish, list mine, update, delete, download/install link display, auth/session handling); Web must support the same share-management operations after YY-ID login (publish/upload valid `.mcm`, list public and mine, update owned package, delete owned package, copy curl/bash install command), subject to the same server policy.
- Curl-bash online installation is in scope: repair `https://mc.dyyapp.com/install` and `https://mc.dyyapp.com/install/pkg/<package-name>` flows, release artifact serving, checksum/signature/pinned-hash verification, Linux x86_64 bootstrap, and package install delegation to normal `mcm install ... --yes` semantics.
- PCL/HMCL replacement requirement must be concrete for the worker: real Minecraft instance install/management, loader support (Vanilla/Fabric/Forge/NeoForge/Quilt), Java/runtime resolution, assets/libraries/natives/classpath, offline + Microsoft/Mojang auth, launch command generation/execution, mod/resource/shader/config package management, import/export where practical, and clear CLI/Web parity; no copying HMCL/PCL code/assets/text, only feature parity as a product target.

## Approval gate

status: awaiting-approval
pending action: write `.omo/plans/mcm-dyyl-launcher-redesign.md` as the decision-complete worker plan.
approach: one architecture-scale plan covering real launcher core, dyyl streaming host + `.mcm` v2, MCM share server static/deploy fixes, real YY-ID/Casdoor/OIDC auth, CLI and Web pkg share management, curl-bash online install repair, and concrete PCL/HMCL replacement feature parity. Treat user-provided OIDC values as deployment secrets: assume first value is client id and second value is client secret unless corrected; never write the secret literal to repo/plan/evidence/logs, only env names and redacted verification.
- dyyl distribution: the MCM curl/bash service also serves dyyl; MCM install/update installs latest dyyl alongside MCM.
- Deployment ports/domains: `mc.dyyapp.com` / existing MCM share server deployment is out of scope for this plan per user instruction. `l.dyyapp.com` reverse-proxies to dyyl-only curl/bash installer service on local port 8951; this installer installs dyyl only, not MCM, and is PM2-managed behind the already-configured reverse proxy.
- dyyl API naming: change docs/tests/API to `mcm.user.source.*` instead of existing `mcm.source.user.*`.
- Platform scope: if Windows can be built/tested in this environment, include it; otherwise first-stage supported platform is Linux, with path/platform abstraction so Windows can be added later.
- Loader scope: first batch includes Vanilla + Fabric + Forge + NeoForge + Quilt.
- `mcm build <in.dyyl>` resolves empty/latest mod/pkg versions by live remote/provider/source resolution at build time and writes the resolved versions/dependencies into `.mcm` lock.
- dyyl execution model: add a dyyl host-command protocol/mode; MCM acts as host for `mcm.*`. In build mode host commands record/resolve into lock steps, not real install. In install/do mode host commands execute according to permission mode.
- dyyl parsing/host contract: MCM does not reimplement dyyl parsing. Add dyyl external machine-readable modes, e.g. `dyyl --emit-ast-json <file>` for parse-only syntax/AST and a streaming host protocol such as `dyyl --host-json <file>` for execution with host-command events. This protocol must be interactive/real-time, not batch-at-end: dyyl writes one JSON event to stdout (or a dedicated pipe) whenever it reaches an `mcm.*` command, flushes, then waits for MCM's JSON response on stdin before continuing. Therefore if a script calls MCM three times with 100 seconds between calls, MCM handles each call immediately at the moment dyyl reaches it. `mcm build <in.dyyl>` invokes dyyl in host-json/build mode, receives structured `mcm.*` command events from dyyl, resolves them, responds, and writes structured JSON lock steps. Existing `source_line` in `.mcm` is for readability/audit only, never the parser source of truth.
- `.mcm` install model: installing compiled `.mcm` executes JSON lock steps only; it does not run dyyl. dyyl is used for `mcm make`, `mcm build`, `mcm do`, and source `.dyyl` authoring/execution.
- Source weighting: user/global sources store a numeric weight. Candidate popularity ranks by `effective_downloads = source_weight * max(raw_download_count, 1)`, so missing or zero download counts are treated as 1 and source weight still has effect. Default source weight is 1.0; `mcm.user.source.weight <url>, <weight>` changes persistent user source weight.
- MCM host session state: every single `.dyyl` script execution (`mcm build`, `mcm do`, or other source `.dyyl` run) owns a per-script session containing `selected_game` / `selected_version`. `mcm.game.choose` / `mcm.game.choose.auto` set the selected version for all subsequent commands in that same script, matching the dyyl docs definition: “此脚本之后的大部分指令将执行在选择的版本上”. The selection remains valid until another choose command in the same script changes it or the script ends. It must not change global default version unless an explicit config command says so. All version-scoped commands (`mcm.game.config`, `mcm.mod.install*`, `mcm.pkg.install*`, `mcm.game.run/stop`, `file.*`, `net.download` destination resolution) operate against that script-selected version. If no version is selected, version-scoped commands return the dyyl/MCM error sentinel instead of falling back to global state, except commands whose purpose is to install/choose a version.
- Do-capable `.mcm` lock design: `.mcm` remains JSON lock, but lock steps preserve dyyl-like executable lines plus normalized structured fields. Install mode reads only `install_steps` after silent permission stripping. Do mode can execute `do_steps` / full compiled command graph with all MCM syntax. Public upload/share validates and accepts only install-permission profile; do-capable local `.mcm` is not accepted as a share package unless stripped/compiled to install-only.
- Compiled lock special metadata should not use invalid JSON comments; represent comments/special MCM data as explicit fields such as `source_line`, `note`, `permission`, `resolved`, and `meta`, while keeping `source_line` dyyl-like for readability.

## Pending owner decisions

- DYyl external command invocation contract if no binary target currently exists in `/x/dyyl/Cargo.toml`.
- Exact install-permitted DYyl/MCM command allowlist.
- Confirm whether environment/toolchain can actually build/test Windows target; if not, Linux-only first batch.
- Confirm final test strategy wording in approval brief: TDD + CLI/server integration tests + real-surface CLI artifacts.
- Rust toolchain install/use is allowed during execution; worker must run fmt/clippy/test/CLI real-surface verification.
- Runtime check 2026-06-28: PM2 has no `mcm` / `mcm-share` process and neither `127.0.0.1:8950` nor `127.0.0.1:8951` is listening. `pm2 describe mcm-share` and `pm2 describe mcm` report non-existent. Per user instruction, do not plan work for MCM share server/8950. Plan only the dyyl-only installer service on 8951, plus health checks for `l.dyyapp.com` / local 8951.

## Evidence notes

- Current `.mcm` is JSON schema v1 in `src/mcm_package.rs`.
- Current `pkg_make_mcm()` prints JSON from lock in `src/pkg_cmd.rs`.
- Current DYyl has MCM API documented but not implemented: `/x/dyyl/dyyl-api-reference.md:262-304`; unknown MCM command fixture exists at `/x/dyyl/tests/fixtures/mcm-unknown.dyyl`.
- Current DYyl file/net commands require absolute paths: `/x/dyyl/src/runtime/cmd/file.rs`, `/x/dyyl/src/runtime/cmd/net.rs`; MCM requires version-root-relative semantics for install/do contexts.
