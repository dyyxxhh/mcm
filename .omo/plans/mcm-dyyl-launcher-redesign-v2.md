# mcm-dyyl-launcher-redesign-v2 - Work Plan

## TL;DR (For humans)

**What you'll get:** MCM becomes a Linux-first Minecraft launcher and package/share system: it can install and launch real Minecraft instances, publish/manage shared packages from both CLI and Web after YY-ID login, run dyyl-driven `.mcm` build/install flows, and repair the online `curl | bash` install routes.

**Why this approach:** The current server and package foundation already exists, but key surfaces are mock or deployment-fragile: OIDC is mock-only, static web files depend on the wrong PM2 cwd, and launching/building still needs real Minecraft/dyyl plumbing. The plan fixes deployment and auth first, then builds CLI/Web share parity, install routes, launcher core, and dyyl/.mcm v2 on top.

**What it will NOT do:** No desktop GUI in this phase. No PCL code/assets/text copying unless a separate explicit license approval is obtained. HMCL GPLv3 code reuse is recommended for launcher-critical logic, but only after a source-file-level license/provenance audit and attribution plan. No OIDC secret literals in repo, plans, evidence, logs, or test output.

**Effort:** XL
**Risk:** High - multiple production surfaces: auth, server deployment, browser UI, package distribution, launcher runtime, dyyl protocol, and `.mcm` format migration.
**Decisions to sanity-check:** The implementation is not a staged MVP: every scoped server, auth, share, curl-bash, launcher, dyyl, and `.mcm` v2 capability below must be completed in this plan. Linux x86_64 is the first required verified platform; Web is only share/YY-ID/dyyl-address management, not a full launcher GUI; OIDC credentials are injected by PM2 env/secret file and verified redacted.

Your next move: run `$start-work .omo/plans/mcm-dyyl-launcher-redesign-v2.md` to execute, or ask for a dual high-accuracy review first. Full execution detail follows below.

---

> TL;DR (machine): XL/high-risk Rust CLI+server+dyyl architecture plan; repair MCM server/static/curl-bash/OIDC, add CLI+Web pkg share parity, implement real launcher core, dyyl streaming host, and `.mcm` v2 JSON lock with strict QA and no secret leakage.

## Scope

### Must have
- **MCM server deploy/static repair:** `mcm serve` must serve `/`, `/index.html`, `/app.js`, `/styles.css`, `/health`, `/api/share/*`, `/api/auth/*`, `/install`, `/install/pkg/{slug}`, and `/release/{filename}` correctly even when PM2 `exec cwd` is not the repo root. Live evidence before planning: PM2 `mcm` is online on `0.0.0.0:8950`, `/health` returns `200`, `/api/share/list` returns `200`, but `/`, `/index.html`, `/app.js`, `/styles.css` return 404 because PM2 cwd is the dyyl repo while static files live in the MCM repo.
- **Real YY-ID/Casdoor/OIDC:** replace production mock auth with real OIDC code exchange, token validation, session creation, and user identity extraction. Keep mock OIDC for tests/dev only. Use deployment contract:
  - `MCM_OIDC_ISSUER=https://auth.dyyapp.com`
  - `MCM_OIDC_CLIENT_ID=<inject via PM2 env or secret file>`
  - `MCM_OIDC_CLIENT_SECRET=<inject via PM2 env or secret file>`
  - `MCM_OIDC_REDIRECT_URL=https://mc.dyyapp.com/api/auth/oidc/callback`
  The user supplied real OIDC credentials in the planning conversation, but the executor must not copy/paste the production secret. The operator/user must inject production credentials out-of-band into PM2 env or a `0600` secret file outside the repo; the executor only implements config loading and verifies redacted presence. The secret literal must never be written to the repository, plan, evidence, stdout, stderr, logs, screenshots, or test fixtures.
- **OIDC flow shape is fixed:** Web uses normal authorization-code flow through the server callback. CLI share login starts on the CLI by requesting `/api/auth/oidc/start?client=cli`, prints the returned server-generated browser `auth_url`, receives a short-lived `login_id`, then polls `/api/auth/oidc/poll/{login_id}` until the browser callback completes. The CLI stores only the resulting MCM session token, never provider access/id/refresh tokens. The server is the confidential OIDC client; the CLI never receives the OIDC client secret.
- **OIDC secret custody is fixed:** the worker must not handle or paste the literal production OIDC client secret. The operator/user must inject it out-of-band into PM2 env or a `0600` secret file outside the repo. The worker may verify only key presence and redacted status (`<present redacted>`), never the literal. If production secret injection is not already present, worker pauses only that production-live verification step and completes implementation/tests with fake secrets.
- **CLI pkg share management parity:** CLI must support package publish/share, update, delete, list public packages, list mine/owned packages, download/install by slug or URL, display copyable install commands, login/session status/logout, and apply identical server policy to Web.
- **Web pkg share management parity:** Web UI must support YY-ID login, session display, public package list, my packages list, upload/publish valid `.mcm`, update owned package, delete owned package, download package, and copy `curl -fsSL https://mc.dyyapp.com/install/pkg/<slug> | bash`.
- **Curl-bash online install repair:** `https://mc.dyyapp.com/install` bootstraps/updates MCM; `https://mc.dyyapp.com/install/pkg/<package-name>` ensures MCM exists then runs the normal package install path with `--yes`/non-interactive semantics. Release artifacts must be served from the configured data dir/release path and verified with the fixed SHA-256 model below before install.
- **Release integrity model is fixed:** use SHA-256 files for this plan. `/release/mcm-linux-x86_64` serves the Linux binary; `/release/mcm-linux-x86_64.sha256` serves one line `<hex>  mcm-linux-x86_64`; curl-bash downloads both, verifies `sha256sum -c`, then installs. No signature alternative in this plan unless the plan is revised.
- **Install prefix policy:** curl-bash defaults to a user-writable install location (`$HOME/.local/bin` or an explicit `MCM_INSTALL_DIR`/`MCM_BIN_DIR` chosen by implementation and documented). It must not silently escalate privileges. If a system location is requested and not writable, the script prints the exact `sudo install ...` command for the user instead of running sudo automatically.
- **PCL/HMCL replacement feature parity (concrete, license-aware HMCL reuse recommended):** provide real Minecraft instance lifecycle and launch capability equivalent in product function: game list/default/info/install/remove/rename/config, Vanilla/Fabric/Forge/NeoForge/Quilt installs, Java/runtime resolution, version manifest fetch, assets/libraries/natives/classpath, offline auth by default, Microsoft/Mojang online auth switch, launch command generation and execution, mod/resource/shader/config package handling, `.mcm v2`/dyyl import-export through `mcm make`, `mcm build`, `mcm install`, and `mcm do`, and clear CLI coverage for management flows. Worker should prefer copying/porting selected HMCL GPLv3 launcher logic for high-risk launcher-critical pieces instead of re-inventing them, but only after recording exact upstream file paths, commit SHA, license headers, copied/adapted functions, attribution notices, and AGPL/GPL obligations. PCL currently has a custom restrictive `LICENCE`; do not copy PCL code/assets/text unless the worker obtains explicit separate permission or a legal review approving compatibility.
- **No deferrals allowed:** any worker response proposing MVP/first-wave/later deferral for a listed Linux x86_64 feature is a plan violation unless the user revises this plan first.
- **dyyl + `.mcm` v2:** language name is `dyyl`. Add streaming host protocol so dyyl emits MCM command events and waits for MCM responses. `.dyyl` is source; `mcm build <in.dyyl>` produces a pure JSON `.mcm` lock; `mcm make <out.dyyl>` exports current instance as dyyl source; `mcm install <pack.mcm>` executes lock install steps only; `mcm do` can execute full dyyl/MCM commands. No compatibility with old `.mcm` v1 is required.
- **`.mcm` v2 meaning is fixed:** v2 is the shared package file format and installable lock format. Server metadata remains storage/index metadata around uploaded v2 packages, not a second package schema. Local installed-state locks may reference v2 package slug/hash/author/source but remain separate client state files. Existing v1 files fail with an actionable “v1 unsupported; rebuild from dyyl” error.
- **Permission model:** uploaded/shared `.mcm` locks must be install-only. Server validates uploads. `mcm install` silently removes commands without install permission and executes allowed install steps. `mcm do` executes full/do graph. Web install bypassing confirmation is intentional.
- **Source weighting:** choose package/mod sources by `effective_downloads = source_weight * max(raw_download_count, 1)`, so missing/zero download count counts as one.
- **Linux-first verification:** current environment is Linux x86_64. Linux x86_64 support must be fully implemented and verified. Unsupported OS/arch may fail explicitly, but no Linux x86_64 feature listed in this plan may be deferred as “later wave/MVP/first wave only.”

### Must NOT have (guardrails, anti-slop, scope boundaries)
- Do not build a desktop GUI in this phase.
- Do not copy PCL code/assets/text/icons/strings or mirror PCL implementation structure without separate explicit permission/legal approval. Do not copy HMCL assets/UI text/icons/strings. HMCL GPLv3 source-code reuse is recommended for launcher-critical logic, but only through the license-aware provenance process in this plan.
- Do not commit, print, screenshot, log, or store the OIDC client secret. Tests must use fake secrets. Evidence must show redaction and env presence without value disclosure.
- Do not treat mock OIDC as production success. Mock remains test/dev-only.
- Do not leave static web serving dependent on process cwd.
- Do not let `mcm install` run arbitrary do/full-power commands from shared packs.
- Do not preserve old `.mcm` v1 compatibility.
- Do not commit `.omo/evidence` by default. Evidence is generated for review and must be scrubbed before any intentional commit.
- Do not add extra GUI/server features beyond share management, YY-ID login entry, dyyl address `/x/dyyl`, install routes, and required management APIs.

## Verification strategy
> Zero human intervention - all verification is agent-executed.
- Test decision: TDD for production behavior. Add failing Rust tests/HTTP tests/browser tests first, capture RED assertions, then implement smallest GREEN changes.
- Rust gates: `cargo fmt --check`; `cargo clippy --all-targets --all-features -- -D warnings`; `cargo test --all-targets --all-features`.
- Real surface gates: `curl` against local and public-equivalent URLs, PM2 process env/cwd checks, browser automation for Web share UI, CLI command invocations in temp dirs, package publish/update/delete/download round trips, curl-bash dry run or disposable install path, launcher dry-run plus fake-Java non-dry-run execution with argv/exit-code assertions.
- Secret gates: grep/evidence must prove no OIDC secret literal in repo, `.omo/`, logs, screenshots, test fixtures, generated configs, or stdout/stderr. Runtime verification may prove `MCM_OIDC_CLIENT_SECRET` is present only by checking key existence or redacted output.
- License gate: run the repository's dependency license audit (`cargo deny check licenses` if `cargo-deny`/`deny.toml` is available; otherwise document why unavailable and run the closest configured license check). HMCL-derived code must have provenance/NOTICE entries before final PASS.
- Deployment authority: the worker may update PM2 ecosystem/config and restart the `mcm` PM2 process when executing this plan, but must record exact commands and before/after `pm2 describe mcm` output with secrets redacted.
- Evidence path convention: `.omo/evidence/task-<N>-mcm-dyyl-launcher-redesign-v2.<ext>` for each todo; final wave evidence under `.omo/evidence/f*-mcm-dyyl-launcher-redesign-v2.*`.

## Execution strategy

### Worker precision contract (do not improvise)

The executor must treat this section as the source of truth when implementing. If current code disagrees, change code to match this contract unless a test proves the contract is impossible; in that case stop and report the exact blocker.

#### Server/env contract
- PM2 app name: `mcm` unless the existing deployment uses a different name and the user explicitly changed it.
- Bind target for local/public service: `0.0.0.0:8950` behind reverse proxy for `https://mc.dyyapp.com`.
- Production PM2 mode for this plan: `mcm serve --mode share --bind 0.0.0.0:8950`. In `share` mode, `/health`, `/install`, `/install/pkg/{slug}`, `/release/{filename}`, `/api/auth/*`, `/api/share/*`, and Web static routes must be available; `/api/source/*` may return source-disabled JSON. In `source` mode, `/api/source/*` is enabled but share publish/update/delete may be disabled. In `both` mode, share and source are both enabled. `/x/dyyl` is not served by MCM in this plan; it is an external dyyl install/source location referenced by docs/reverse proxy.
- Required env names:
  - `MCM_SHARE_DATA_DIR`: server DB/blob/release root; must not be under `/x`; current live value was `/home/usr/.mcm/share`.
  - `MCM_WEB_DIR`: optional explicit web asset dir if assets are not embedded; if absent, server must still find assets via binary-relative/release-relative fallback.
  - `MCM_OIDC_ISSUER`: exact production value `https://auth.dyyapp.com`.
  - `MCM_OIDC_CLIENT_ID`: inject via PM2 env/secret file; do not write literal value in repo/evidence/logs.
  - `MCM_OIDC_CLIENT_SECRET`: inject via PM2 env/secret file; never print literal; debug output must show `<redacted>`.
  - `MCM_OIDC_REDIRECT_URL`: exact production value `https://mc.dyyapp.com/api/auth/oidc/callback`.
- Startup behavior:
  - Production/real auth mode must fail clearly if issuer/client id/client secret/redirect URL are missing.
  - Test/dev mock mode must be explicit, e.g. `MCM_AUTH_MODE=mock` or test-only router helper. Do not silently fall back to mock in production.
  - Secret validation output may print env key names and `<present redacted>` only.

#### HTTP route contract
All responses must be JSON except static assets, release files, and shell install scripts.

| Method | Path | Auth | Success | Failure |
| --- | --- | --- | --- | --- |
| `GET` | `/health` | none | `200 {"status":"ok","mode":"share|source|both"}` | never depends on OIDC/static assets |
| `GET` | `/` | none | `200 text/html`, same SPA as `/index.html` | not 404 when PM2 cwd is not repo root |
| `GET` | `/index.html` | none | `200 text/html` | 404 only if configured/embedded asset truly absent |
| `GET` | `/app.js` | none | `200 application/javascript` or compatible JS content type | same cwd rule |
| `GET` | `/styles.css` | none | `200 text/css` | same cwd rule |
| `GET` | `/api/auth/oidc/start` | none | `200 {"auth_url":"https://auth.dyyapp.com/...","state":"...","login_id":"..."?}` | `500/503` redacted config/provider error |
| `GET` | `/api/auth/oidc/callback?code=...&state=...` | provider callback | creates MCM session cookie and either redirects to Web or returns JSON compatible with CLI polling | `400` invalid/replayed state, `401/502` token validation/exchange failure; no token/secret in body |
| `GET` | `/api/auth/oidc/poll/{login_id}` | none; possession of random login id is the capability | while pending: `200 {"status":"pending"}`; complete: `200 {"status":"complete","token":"<mcm-session-token>","owner":"...","expires_at_unix":...}`; expired: `410 {"status":"expired"}`; denied/failure: `400/401 {"status":"denied","error":"..."}` | login id TTL 10 minutes; one successful poll consumes the token result; token is an MCM session token only, never OIDC provider token |
| `GET` | `/api/auth/oidc/session` | cookie or bearer | `200 {"owner":"stable-id","display_name":"..."?}` | `401 {"error":"unauthenticated"}` |
| `POST` | `/api/auth/oidc/logout` | cookie or bearer | clears session, `200 {"status":"ok"}` | idempotent ok if no session |
| `GET` | `/api/share/list` | none | `200 {"packages":[...]}` | storage errors are `500` redacted |
| `GET` | `/api/share/mine` | OIDC session | `200 {"packages":[owned...]}` | `401` unauthenticated |
| `POST` | `/api/share/pkg` | OIDC session | publish package, `201 {"slug":"...","install_command":"curl -fsSL https://mc.dyyapp.com/install/pkg/<slug> | bash"}` | `400` invalid package/slug, `401`, `409`, `413`, `415`, `429` policy |
| `GET` | `/api/share/pkg/{slug}` | none | raw `.mcm` JSON bytes, `200 application/json` | `404` missing |
| `PUT` | `/api/share/pkg/{slug}` | OIDC session | update owned package, `200` metadata | `401`, `403`, `404`, policy errors |
| `DELETE` | `/api/share/pkg/{slug}` | OIDC session | delete owned package, `200 {"status":"deleted"}` | `401`, `403`, `404` |
| `GET` | `/api/share/pkg/{slug}/install-command` | none | `200 {"install_command":"curl -fsSL https://mc.dyyapp.com/install/pkg/<slug> | bash"}` | `404` missing |
| `GET` | `/install` | none | shell script for Linux x86_64 MCM bootstrap/update | script exits nonzero on unsupported OS/arch/checksum mismatch |
| `GET` | `/install/pkg/{slug}` | none | shell script that bootstraps MCM then runs `mcm install <downloaded-or-url .mcm> --yes` | invalid slug safely quoted/rejected |
| `GET` | `/release/{filename}` | none | allowlisted release artifact or `.sha256` from data dir: `mcm-linux-x86_64`, `mcm-linux-x86_64.sha256` | traversal and unknown filename 404 |

Package metadata fields returned by list/mine/detail endpoints must include at least: `slug`, `name`, `version`, `description` if present, `owner`, `updated_at`, `created_at`, `sha256`, `size_bytes`, and `install_command`.

#### CLI command contract
Implement these exact commands or compatible aliases; tests must assert stdout/stderr and exit codes.

| Command | Behavior |
| --- | --- |
| `mcm pkg auth login --server <url>` | prints browser auth URL, polls session transaction, stores MCM session token; never prints provider tokens/client secret |
| `mcm pkg auth status --server <url>` | prints authenticated owner/display name or unauthenticated; exit 0 for readable status |
| `mcm pkg auth logout --server <url>` | removes local session and calls server logout when possible |
| `mcm pkg share <file.mcm> --server <url> [--yes]` | validates package, requires auth, publishes, prints slug and exact curl-bash install command |
| `mcm pkg list --server <url>` | lists public packages with slug/version/owner/updated/install command or URL |
| `mcm pkg list --mine --server <url>` | lists packages owned by current auth session; 401 gives login instruction |
| `mcm pkg update <slug> <file.mcm> --server <url> [--yes]` | updates owned package, prints updated metadata/install command |
| `mcm pkg delete <slug> --server <url> [--yes]` | deletes owned package after normal confirmation unless `--yes` |
| `mcm pkg download <slug-or-url> --server <url> [--output <path>]` | downloads package JSON without executing it |
| `mcm pkg install <slug-or-url> --server <url> --yes` | downloads/resolves package and delegates to low-power `mcm install` semantics |
| `mcm make <out.dyyl>` | exports current configured instance/package state as dyyl source |
| `mcm build <in.dyyl> [-o <out.mcm>]` | runs dyyl in build host mode and writes deterministic `.mcm` v2 JSON lock |
| `mcm install <pack.mcm|url> --yes` | executes install-permitted v2 lock steps only |
| `mcm do <file.dyyl|pack.mcm> --yes` | executes full/do-capable command graph |
| `mcm game install <name> <target> --yes` | installs/records real Minecraft instance for smart targets like `mc1.21.1-fabric` |
| `mcm run [--dry-run]` | dry-run prints complete launch command; non-dry-run spawns Java and propagates exit/log path |

#### Web UI contract
The Web UI is not a full launcher GUI. It must contain only these product surfaces:
- Design system is already present at `DESIGN.md` and must be followed exactly. Use its black/white documentation-first style, pill buttons, rounded inputs, flat cards, install snippet component, error banner, empty state, spinner, responsive widths, and “no emoji” icon rule. Do not rerun getdesign or invent a new visual language unless the user explicitly asks.
- Header/session area: unauthenticated state shows “Login with YY-ID”; authenticated state shows owner/display name and logout.
- Public packages table/cards: slug, name/version, owner, updated time, copy install command, download action.
- My packages area: visible after login; lists owned packages; provides update, delete, and copy install command.
- Publish/upload area: accepts `.mcm` file; validates client-side extension/size for UX but server remains authoritative; shows server validation errors.
- Error states: unauthenticated, daily push limit, max package limit, duplicate slug, invalid package, forbidden owner mismatch.
- Responsive requirements: no horizontal overflow at 375px, 768px, 1280px; package actions remain reachable.
- Required stable selectors for browser tests: `[data-testid=login-yyid]`, `[data-testid=session-owner]`, `[data-testid=logout]`, `[data-testid=public-packages]`, `[data-testid=my-packages]`, `[data-testid=package-upload]`, `[data-testid=package-update]`, `[data-testid=package-delete]`, `[data-testid=copy-install-command]`, `[data-testid=error-banner]`.

#### Launcher completion contract
Do not stop at dry-run-only or metadata-only. Linux x86_64 completion requires:
- `mcm game install` can create runnable instance records for Vanilla, Fabric, Forge, NeoForge, and Quilt targets.
- Version resolution supports latest and explicit forms for Minecraft and loaders.
- Required files are downloaded or fixture-resolved in tests: client jar, version JSON, libraries, assets index/assets, loader libraries, and natives.
- Java is discovered or the user gets an actionable install/runtime error that names the required Java major version and exact install/config command. Managed Java auto-install is not required in this plan; actionable discovery/error is required.
- `mcm run --dry-run` prints exact Java executable, JVM args, natives path, classpath, main class, game args, assets path, auth args, working dir.
- `mcm run` non-dry-run launches a process; tests use a fake Java executable to verify argv and exit propagation.
- Offline auth is default. Microsoft/Mojang online mode is selectable and mock-tested; tokens are redacted.

#### HMCL/PCL source reuse contract
- HMCL repo/license evidence gathered during planning: `HMCL-dev/HMCL` is public and GitHub reports `GPL-3.0`; its `LICENSE` is GNU GPL v3. GPLv3 section 13 permits combining GPLv3-covered work with AGPLv3-covered work, with GPL terms applying to the GPL part and AGPL section 13 network-source obligations applying to the combination.
- Recommended HMCL reuse: copy/port small, well-bounded launcher algorithms or data-model logic where it materially reduces risk (for example version manifest parsing, library/native classification, launch argument assembly), provided each copied/ported unit has a provenance comment or `NOTICE` entry with upstream repo URL, commit SHA, file path, original copyright/license header if present, and adaptation notes. The worker should not waste time re-deriving tricky launcher behavior that HMCL already implements under GPLv3-compatible terms.
- Required HMCL audit before copying: inspect the exact upstream source file license header and dependency context. Do not copy files with extra incompatible terms, third-party embedded code, assets, translations, icons, UI text, or generated blobs.
- PCL repo/license evidence gathered during planning: `Meloong-Git/PCL` GitHub reports license `Other/NOASSERTION`; root `LICENCE` is a custom “PCL 分发有限许可 / 存储库合理使用指南” with restrictions and naming/attribution/usage requirements. Treat it as not AGPL-compatible for direct copying unless separate explicit permission/legal review says otherwise.
- Allowed PCL use without extra approval: high-level behavioral reference only, plus very small factual observations that are not copyrightable. No PCL source copying, no assets, no UI text, no names/branding implying association.
- Final verification must include a provenance table for every HMCL-derived code section and a scan proving no PCL code/assets/text were copied.

#### `.mcm` v2 lock contract
Minimum top-level fields for v2 JSON locks:
- `schema_version: 2`
- `kind: "mcm-lock"`
- `identity`: `{ "name", "version", "description"? }`
- `author`: `{ "owner_id"?, "source"? }`
- `permissions`: `{ "install": true, "do": bool, "full": bool }`
- `game`: selected game/version/loader constraints if applicable
- `steps`: ordered structured operations, each with `op`, `permission`, arguments, and optional `source_line`
- `artifacts`: downloadable artifacts with URL/source/hash/target metadata
- `created_at`, `generator`
Rules: v1 unsupported; unknown future schema version fails; secret-like keys rejected recursively; paths reject absolute, `..`, backslash, NUL, Windows-reserved components; `source_line` is for humans only and is never reparsed.

Minimum v2 op registry for this plan:

| `steps[].op` | Permission | Required args | Path/network rules | Upload allowed | Install effect | Do/full effect |
| --- | --- | --- | --- | --- | --- | --- |
| `game.choose` | install | `game`, `version` | no paths | yes | selects version context for following install steps | same |
| `game.install` | install | `game`, `target` | target parsed by game grammar | yes | installs/records version/loader | same |
| `mod.install` | install | `id`, optional `version`, `side` | provider/source URL must pass URL safety | yes | installs mod artifact | same |
| `pkg.install` | install | `slug_or_url` | URL safety, package must validate as v2 | yes | installs nested install-only package | same |
| `file.copy` | install | `src_artifact`, `dest` | `dest` version-root relative only | yes | copies artifact to version root | same |
| `file.write` | install | `dest`, `content` or `artifact` | `dest` version-root relative only; no secrets | yes | writes file under version root | same |
| `net.download` | install | `url`, `dest`, `sha256` | URL safety; `dest` version-root relative; hash required | yes | downloads+verifies | same |
| `config.set` | install | `scope`, `key`, `value` | scope `version|user`; no secret keys | yes | writes config | same |
| `shell.run` | do | `command`, optional `cwd` | no upload; cwd version-root relative if present | no | stripped locally / rejected on upload | executes in do mode after warning/confirmation policy |
| `mcm.do` | do | nested command | no upload | no | stripped locally / rejected on upload | executes nested command |
| `root.system` | full | explicit command | no upload; non-bypassable confirmation | no | stripped locally / rejected on upload | prints/executes per root-system policy only in full/do context |

Local `mcm install` on a mixed lock strips `do`/`full` steps and executes remaining install steps. Server upload of any lock containing non-install steps rejects the whole upload with `400`, never strips and publishes a modified package.

#### dyyl host protocol contract
Use newline-delimited JSON (NDJSON) over stdio and document it in `/x/dyyl` and MCM docs. dyyl writes protocol JSON messages to stdout, diagnostics to stderr, and flushes after every line. Host reads one JSON object per line, responds with one JSON object per line, and correlates by `id`. Required event shapes:
- dyyl to host: `{ "type":"mcm_command", "id":"...", "name":"mcm.game.choose", "args": [...], "source_line": "..." }`
- host to dyyl success: `{ "type":"mcm_response", "id":"...", "ok": true, "value": ... }`
- host to dyyl failure sentinel: `{ "type":"mcm_response", "id":"...", "ok": false, "error": { "code":"...", "message":"..." } }`
Ordering: dyyl must not send a second command requiring the first response until the first response arrives unless it marks the command `parallel_safe:true`; MCM may reject unsupported parallel commands with `error.code="parallel_unsupported"`. Timeout: MCM host timeout defaults to 60s per command and returns `error.code="host_timeout"`; dyyl exits nonzero on unhandled error sentinel. Build mode records resolved lock steps; install mode must not run dyyl; do mode executes full graph.

### Parallel execution waves
- **Wave 1 - Server/deploy foundation:** fix static serving, release/install routes, config/env contract, and share API gaps. These unblock browser, curl-bash, and auth verification.
- **Wave 2 - Real auth:** implement production OIDC while preserving mock tests. Requires Wave 1 config/deploy stability.
- **Wave 3 - Share management parity:** CLI and Web share management over the same API. CLI and Web can proceed in parallel after auth/API contracts stabilize.
- **Wave 4 - Launcher core:** complete real Minecraft install/launch primitives and PCL/HMCL replacement management flows for the full scoped Linux x86_64 feature set. Can proceed in parallel with dyyl design after server contracts are stable.
- **Wave 5 - dyyl/.mcm v2:** streaming host protocol, build/make/install/do, permission model, source weighting. Requires launcher/package primitives.
- **Wave 6 - Deployment docs + final hardening:** PM2 config, release artifacts, public URL checks, docs, security/scope audits.

### Dependency matrix
| Todo | Depends on | Blocks | Can parallelize with |
| --- | --- | --- | --- |
| 1 | none | 2,3,4,5,6 | none |
| 2 | 1 | 4,5,6 | 3 |
| 3 | 1 | 6,18 | 2 |
| 4 | 1,2 | 7,8,9 | 5 |
| 5 | 1,2 | 8,9,18 | 4 |
| 6 | 1,2,3 | 18 | 7 |
| 7 | 4 | 9,18 | 6,8 |
| 8 | 4,5 | 18 | 7 |
| 9 | 4,5,7,8 | 18 | 10 |
| 10 | 1 | 11,12,13 | 6,7 |
| 11 | 10 | 13,18 | 12 |
| 12 | 10 | 13,18 | 11 |
| 13 | 10,11,12 | 15,18 | 14 |
| 14 | 1 | 15,16,17 | 10 |
| 15 | 13,14 | 16,17,18 | none |
| 16 | 15 | 17,18 | none |
| 17 | 15,16 | 18 | none |
| 18 | all prior | final verification | none |

## Todos
> Implementation + Test = ONE todo. Never separate.
<!-- APPEND TASK BATCHES BELOW THIS LINE WITH edit/apply_patch - never rewrite the headers above. -->

- [x] 1. Server asset roots and PM2 cwd independence
  What to do / Must NOT do: Add a deployment-safe static asset root for MCM Web assets and make `mcm serve` resolve `web/index.html`, `web/app.js`, and `web/styles.css` independent of process cwd. Prefer compile-time embedded assets or an explicit `MCM_WEB_DIR` / binary-relative fallback; tests must cover cwd different from repo root. Do not rely on PM2 cwd being corrected as the only fix.
  Parallelization: Wave 1 | Blocked by: none | Blocks: 2,3,4,5,6
  References (executor has NO interview context - be exhaustive): `src/server/mod.rs:127-149` mounts static files via `ServeFile::new("web/...")`; `src/server/mod.rs:162-166` reads `web/index.html` relative to cwd; live PM2 evidence: `exec cwd /mnt/.../nas/lucky/dyyl`, `/health` 200, `/` and static files 404; MCM repo contains `web/index.html`, `web/app.js`, `web/styles.css`.
  Acceptance criteria (agent-executable): a test starts router/server with current dir set to a temp dir with no `web/` and `GET /`, `/index.html`, `/app.js`, `/styles.css` return 200 with correct content types; live `curl -i http://127.0.0.1:8950/` after PM2 restart returns 200 text/html even if PM2 cwd is not repo root.
  QA scenarios (name the exact tool + invocation): Happy: `curl -i http://127.0.0.1:8950/`, `/index.html`, `/app.js`, `/styles.css`, `/health` after PM2 restart; evidence `.omo/evidence/task-1-mcm-dyyl-launcher-redesign-v2.txt`. Failure: start server with cwd `/tmp` and no `web/`; static files still resolve from configured/embedded root; evidence same file.
  Commit: Y | fix(server): serve web assets independent of cwd

- [x] 2. Release artifact and curl-bash route repair
  What to do / Must NOT do: Repair `/install`, `/install/pkg/{slug}`, and `/release/{filename}` so online installation works from `https://mc.dyyapp.com` and local test server. Bootstrap must support Linux x86_64, fetch `/release/mcm-linux-x86_64` and `/release/mcm-linux-x86_64.sha256`, verify with `sha256sum -c`, install/update binary to a documented user-writable prefix by default, and fail explicit unsupported platform for other OS/arch. Package route must ensure MCM exists, download the named package, then delegate to normal `mcm install <downloaded-or-url .mcm> --yes`. Do not add bespoke package semantics in shell beyond bootstrap/download/delegation. Do not run sudo automatically.
  Parallelization: Wave 1 | Blocked by: 1 | Blocks: 4,5,6
  References: `src/server/mod.rs:130-133` registers `/install`, `/install/pkg/{slug}`, `/release/{filename}`; `src/server/mod.rs:207-230` serves release files from `{data_dir}/release/`; prior plan `.omo/plans/mcm-minecraft-manager-expansion.md:59-64` defines install routes; user explicitly said curl bash online install likely needs repair.
  Acceptance criteria: local HTTP tests assert `/install` returns executable shell with `set -e`, platform detection, `/release/mcm-linux-x86_64`, `/release/mcm-linux-x86_64.sha256`, `sha256sum -c`, and no unquoted package names; `/install/pkg/sample` returns/executes flow that calls `mcm install ... --yes`; invalid slug returns 400/404; missing release artifact returns clear error; release filenames are allowlisted and path traversal is rejected.
  QA scenarios: Happy: `curl -fsSL http://127.0.0.1:8950/install` and `/install/pkg/sample` into temp scripts, run with disposable `PATH`/install dir and fake release artifact/checksum; evidence `.omo/evidence/task-2-mcm-dyyl-launcher-redesign-v2.txt`. Failure: request `/release/../../etc/passwd`, unsupported arch env, missing checksum, tampered artifact; all fail safely.
  Commit: Y | fix(install): repair verified curl bash routes

- [x] 3. Server configuration and secret-safe deployment contract
  What to do / Must NOT do: Finalize `ServerConfig` and deployment docs for OIDC and server asset/data dirs. Ensure envs are parsed: `MCM_SHARE_DATA_DIR`, `MCM_WEB_DIR` if introduced, `MCM_OIDC_ISSUER`, `MCM_OIDC_CLIENT_ID`, `MCM_OIDC_CLIENT_SECRET`, `MCM_OIDC_REDIRECT_URL`. Issuer default/expected value is `https://auth.dyyapp.com`; redirect expected value is `https://mc.dyyapp.com/api/auth/oidc/callback`. Client id and secret are injected out-of-band through PM2 env or a `0600` secret file outside the repo; worker must not paste or print the production secret. Add a redacted config debug/status path useful for deployment checks.
  Parallelization: Wave 1 | Blocked by: 1 | Blocks: 6,18
  References: `src/server/config.rs:75-86` has `ServerConfig` fields; `src/server/config.rs:99-116` already reads issuer/client id/client secret; `src/server/config.rs:43-63` has `SecretString` redaction; PM2 live env currently only has `MCM_SHARE_DATA_DIR`; prior plan `.omo/plans/mcm-minecraft-manager-expansion.md:54-55` defines issuer/callback; user supplied credentials but secret must not be persisted.
  Acceptance criteria: config tests prove secret redaction; startup in real auth mode fails clearly if issuer/client_id/client_secret/redirect_url are missing; mock mode must be explicit for tests/dev and never silently used when production env is incomplete; PM2 example includes env names only.
  QA scenarios: Happy: run server with fake OIDC env values and verify a redacted config diagnostic/log contains issuer/client id presence and `<redacted>` for secret; evidence `.omo/evidence/task-3-mcm-dyyl-launcher-redesign-v2.txt`. Failure: omit `MCM_OIDC_CLIENT_SECRET` in real mode; startup/auth start fails with non-secret error text.
  Commit: Y | chore(server): define secret-safe oidc deployment config

- [x] 4. Real YY-ID/Casdoor OIDC flow
  What to do / Must NOT do: Implement production OIDC start/callback/session/logout over Casdoor/YY-ID. `/api/auth/oidc/start` must create nonce/state, build provider auth URL under `https://auth.dyyapp.com`, include redirect URI `https://mc.dyyapp.com/api/auth/oidc/callback`, and store pending state. Callback must validate state, exchange code for tokens, validate issuer/audience/expiry/nonce/signature as applicable, derive stable owner from `sub` plus display name from `preferred_username`/`name`/`email`, create session cookie, and redirect or return JSON compatible with CLI/Web. Keep mock OIDC under explicit test/dev mode only. Do not log tokens, codes, state values beyond safe truncated IDs, or secrets.
  Parallelization: Wave 2 | Blocked by: 1,2 | Blocks: 7,8,9
  References: `src/server/auth.rs:217-222` routes currently point to mock handlers; `src/server/auth/mock.rs:63-159` mock start/callback/session; `src/server/config.rs:111-115` reads OIDC env placeholders; completed plan learnings `.omo/notepads/mcm-minecraft-manager-expansion/learnings.md:737-795` describe session store/extractor and real OIDC stub boundary.
  Acceptance criteria: unit/integration tests with a local fake OIDC provider cover start URL shape, CLI `login_id` issuance, poll pending/complete/expired/denied, valid callback creates session, one successful CLI poll consumes token result, invalid/replayed state fails, token exchange failure fails, wrong issuer/audience fails, expired token fails, `/session` returns owner, `/logout` clears session, mock mode still passes existing tests.
  QA scenarios: Happy: run fake OIDC provider locally, `curl -c cookies /api/auth/oidc/start`, follow auth/callback, then `curl -b cookies /api/auth/oidc/session` returns stable owner; evidence `.omo/evidence/task-4-mcm-dyyl-launcher-redesign-v2.txt`. Failure: wrong state, wrong issuer, missing secret env, replayed callback all fail without secret/token leakage.
  Commit: Y | feat(auth): add real oidc login for yy-id

- [x] 5. Share API completeness for CLI and Web management
  What to do / Must NOT do: Extend share HTTP API to support all shared package management needed by CLI and Web: public list with metadata, owned/mine list, publish/upload, update owned package, delete owned package, download package, install command metadata, slug validation, owner checks, daily push limit, max 5 packages/user, delete slug reservation, schema validation for install-only uploaded `.mcm` locks. Keep no admin token and no Turnstile. Do not duplicate policy separately in CLI/Web. Route/command/UI matrix must be exact: list public = `GET /api/share/list` + `mcm pkg list` + Web public list; list mine = authenticated `GET /api/share/mine` + `mcm pkg list --mine` + Web mine tab; publish = authenticated `POST /api/share/pkg` + `mcm pkg share <file>` + Web upload; update = authenticated `PUT /api/share/pkg/{slug}` + `mcm pkg update <slug> <file>` + Web update; delete = authenticated `DELETE /api/share/pkg/{slug}` + `mcm pkg delete <slug>` + Web delete; download = `GET /api/share/pkg/{slug}` + `mcm pkg download/install <slug>` + Web download; install snippet = `GET /api/share/pkg/{slug}/install-command` or equivalent metadata + CLI/Web display.
  Parallelization: Wave 2 | Blocked by: 1,2 | Blocks: 8,9,18
  References: `src/server/share.rs:37-47` current routes include list/pkg GET/PUT/DELETE/POST; `.omo/plans/mcm-minecraft-manager-expansion.md:45-58` defines publish/update/delete policy; `.omo/notepads/.../learnings.md:737-790` lists implemented mock-OIDC policy and tests; user explicitly requires CLI and Web pkg share management.
  Acceptance criteria: API tests cover public list, mine list, publish valid install-only package, reject non-install permissions on upload, update owner succeeds, update other user 403, delete owner succeeds, duplicate slug 409, sixth package limit, daily push limit, oversized 413, non-JSON/invalid package 415/400, install command endpoint returns exact curl-bash command.
  QA scenarios: Happy: authenticated fake OIDC user publishes, lists mine, updates next day via fake clock, downloads, deletes; evidence `.omo/evidence/task-5-mcm-dyyl-launcher-redesign-v2.txt`. Failure: unauthenticated publish 401, other-user update/delete 403, do-capable `.mcm` upload rejected.
  Commit: Y | feat(share): complete package management api

- [x] 6. Production PM2 and public route deployment verification
  What to do / Must NOT do: Add/update PM2 ecosystem/deployment instructions and scripts so `mcm` runs with correct binary path, data dir, optional web dir, OIDC env injection, and bind/reverse proxy assumptions. Before changing PM2, capture redacted `pm2 describe mcm` and current start command/ecosystem location. Provide exact restart and rollback commands with secrets redacted. Validate local service and public-domain-equivalent paths. Do not store secret values in ecosystem files committed to repo; use placeholders or external secret file.
  Parallelization: Wave 1/2 | Blocked by: 1,2,3 | Blocks: 18
  References: live PM2 evidence: `script path .../mcm/target/release/mcm`, args `serve --mode share --bind 0.0.0.0:8950`, cwd `.../dyyl`, env only `MCM_SHARE_DATA_DIR`; README prior PM2 example from old plan includes env names only; user wants current server now in scope.
  Acceptance criteria: documented PM2 start/restart/rollback steps; before/after redacted `pm2 describe mcm` captured; after restart shows expected args/env keys/cwd policy; `curl` checks pass for `/`, `/health`, `/api/share/list`, `/api/auth/oidc/start`, `/install`; secret values not printed.
  QA scenarios: Happy: restart PM2 with configured env and run curl smoke suite; evidence `.omo/evidence/task-6-mcm-dyyl-launcher-redesign-v2.txt`. Failure: run with missing OIDC secret in real mode and confirm explicit redacted startup/auth error.
  Commit: Y | docs(deploy): add pm2 oidc and route verification

- [x] 7. CLI auth/session commands for pkg share
  What to do / Must NOT do: Add CLI commands for OIDC/session lifecycle used by package sharing: start login by printing server-generated auth URL, poll a short-lived server login transaction until browser callback completes, store only the MCM session token securely in MCM config/state, show session status, and logout. Commands must be non-interactive-friendly but never print tokens/secrets. Integrate with existing `mcm pkg share` flow. The CLI must never know the OIDC client secret.
  Parallelization: Wave 3 | Blocked by: 4 | Blocks: 9,18
  References: README generated by prior plan describes `mcm pkg share ./my-pack.mcm` login flow; auth session extractor reads bearer or cookie in `src/server/auth.rs`; current CLI package command surface in `src/pkg_cmd.rs` must be updated; user requires CLI pkg share management.
  Acceptance criteria: CLI tests with fake server cover login start URL output, token storage redacted, status authenticated/unauthenticated, logout removes token, publish uses Authorization bearer, missing login gives actionable error.
  QA scenarios: Happy: `mcm pkg auth login --server http://127.0.0.1:<port>` against fake OIDC/share server then `mcm pkg auth status`; evidence `.omo/evidence/task-7-mcm-dyyl-launcher-redesign-v2.txt`. Failure: invalid token returns unauthenticated and does not leave corrupt session file.
  Commit: Y | feat(pkg): add cli share auth session commands

- [x] 8. CLI pkg share/list/update/delete/install management
  What to do / Must NOT do: Implement CLI package share management parity: `mcm pkg share <file.mcm>`, `mcm pkg list` public, `mcm pkg list --mine`, `mcm pkg update <slug> <file.mcm>`, `mcm pkg delete <slug>`, `mcm pkg download <slug-or-url>`, `mcm pkg install <slug-or-url> --yes`, and display exact `curl -fsSL https://mc.dyyapp.com/install/pkg/<slug> | bash` after publish/update. Preserve confirmation policy except curl-bash/web install route. Do not bypass server policy client-side.
  Parallelization: Wave 3 | Blocked by: 4,5 | Blocks: 9,18
  References: current README/old plan CLI grammar for `pkg`; share API from todo 5; existing package install/download code in `src/pkg_cmd.rs`, `src/pkg_install.rs`, `src/mcm_package.rs`; user explicitly says include CLI PKG share management.
  Acceptance criteria: integration tests cover publish, list public, list mine, update owned package, delete owned package, download by slug, install by slug, unauthenticated publish/update/delete error, same-day push limit surfaced, install command printed exactly.
  QA scenarios: Happy: run CLI against local test server with fake OIDC session and temp config; evidence `.omo/evidence/task-8-mcm-dyyl-launcher-redesign-v2.txt`. Failure: other-user delete/update and invalid `.mcm` upload fail with clear messages.
  Commit: Y | feat(pkg): add cli share management

- [x] 9. Web pkg share management UI with visual QA
  What to do / Must NOT do: Build/repair Web share UI for YY-ID login and package management: login button, session/user state, public package list, my package list, upload publish, update owned package, delete owned package with confirmation, download link, copy curl-bash install command, error states for policy failures. Keep Web limited to share management + YY-ID login entry + dyyl address; do not build desktop launcher UI. Follow existing `DESIGN.md` exactly: high-contrast black/white, documentation-first, pill buttons, rounded inputs, flat bordered cards, install snippet style, CSS spinner, inline SVG/CSS icons only, no emoji, no gradients, no shadows, no magic-number CSS when a token exists.
  Parallelization: Wave 3 | Blocked by: 4,5,7,8 | Blocks: 18
  References: `web/index.html`, `web/app.js`, `web/styles.css`; `DESIGN.md:1-168` existing MCM Web UI Design System; todo 1 fixes static serving; share API todo 5; user explicitly says include Web端 PKG share管理.
  Acceptance criteria: browser tests cover login through fake OIDC, public list, mine list, publish valid package, update package, delete package, copy install command, unauthenticated state, policy error display. UI works at 375, 768, 1280 widths.
  QA scenarios: Happy: Playwright opens local server, logs in via fake OIDC, publishes/updates/deletes and screenshots each breakpoint; evidence `.omo/evidence/task-9-mcm-dyyl-launcher-redesign-v2/`. Failure: unauthenticated upload prompts login; daily limit/403 errors shown without console errors. Run visual-qa skill after UI changes.
  Commit: Y | feat(web): add package share management ui

- [x] 10. Complete Minecraft metadata, loaders, and instance model
  What to do / Must NOT do: Replace mock game install surfaces with a complete real metadata-backed instance model for the scoped Linux x86_64 product. Mandatory capabilities: Vanilla, Fabric, Forge, NeoForge, and Quilt install resolution; Mojang version manifest; game list/default/info/install/remove/rename/config; mod/resource/shader/datapack package placement; offline and online launch readiness; explicit/latest Minecraft and loader version resolution. Resolve latest/explicit Minecraft versions and compatible loader versions. Do not make top-level `mcm install mc-neoforge`; smart Minecraft targets stay under `mcm game install` and package/dyyl contents. Do not mark any listed loader or game-management capability as deferred. The worker should inspect and port/copy selected HMCL GPLv3 launcher logic for complex launcher behavior after source-file-level provenance audit; PCL direct copying remains forbidden without separate approval.
  Parallelization: Wave 4 | Blocked by: 1 | Blocks: 11,12,13
  References: current `src/game_install.rs` noted as mock manifest/jar/loader; existing game command files; user requires PCL/HMCL replacement concreteness; prior draft decisions: first batch Vanilla+Fabric+Forge+NeoForge+Quilt, Linux first.
  Acceptance criteria: tests with fixture manifests cover `mc`, `mc1.21.1`, `mc-fabric`, `mc1.21.1-fabric`, `mc1.21.1-fabric-<fixture-loader-version>`, `mc-forge`, `mc1.21.1-forge`, `mc1.21.1-forge-<fixture-loader-version>`, `mc-neoforge`, `mc1.21.1-neoforge`, `mc1.21.1-neoforge-21.1.172`, `mc-quilt`, `mc1.21.1-quilt`, and `mc1.21.1-quilt-<fixture-loader-version>`; install creates version/instance metadata without network in fixture mode; unsupported target errors clearly; if HMCL code is copied/ported, `NOTICE`/provenance docs list upstream commit/file/function and license; no PCL code/assets/text appear in diff.
  QA scenarios: Happy: `mcm game install dev mc1.21.1-fabric --yes --dry-run/fixture` then `mcm game list/info/default`; evidence `.omo/evidence/task-10-mcm-dyyl-launcher-redesign-v2.txt`. Failure: invalid loader/version combination fails with actionable provider error.
  Commit: Y | feat(game): resolve real minecraft versions and loaders

- [x] 11. Java/runtime, assets, libraries, natives, classpath
  What to do / Must NOT do: Implement launcher runtime resolution sufficient for real launch: required Java version detection/selection or install guidance, asset index download/verification, library download/verification, native extraction, classpath assembly, JVM/game args interpolation, OS/arch rules for Linux x86_64. Do not silently use incomplete classpaths.
  Parallelization: Wave 4 | Blocked by: 10 | Blocks: 13,18
  References: `src/launch.rs` currently uses insufficient mock auth/natives/classpath/assets handling; PCL/HMCL replacement requirement includes Java/runtime/assets/libraries/natives/classpath.
  Acceptance criteria: fixture tests build expected classpath/assets/natives for representative vanilla and loader versions; missing Java gives clear error/instruction; checksum mismatch fails; native extraction path traversal rejected.
  QA scenarios: Happy: run launcher prepare/dry-run command for fixture instance and verify classpath contains expected libs/assets/natives; evidence `.omo/evidence/task-11-mcm-dyyl-launcher-redesign-v2.txt`. Failure: tampered library hash and missing Java fail before launch.
  Commit: Y | feat(launch): prepare java assets libraries and natives

- [x] 12. Offline and Microsoft/Mojang auth for launching
  What to do / Must NOT do: Implement launch auth modes: offline default and online Microsoft/Mojang account mode with mockable provider/session tests. Switching online/offline must be simple from CLI/config. Launch command must include correct username/uuid/access token/user type semantics for selected mode. Do not require YY-ID for Minecraft game launch; YY-ID is for Web/share login.
  Parallelization: Wave 4 | Blocked by: 10 | Blocks: 13,18
  References: user decision: first batch Microsoft/Mojang online and offline, default offline; current `src/launch.rs` uses mock auth; existing config surfaces.
  Acceptance criteria: tests cover offline UUID stability, online mock session success, expired online session refresh/fail behavior, config switch, and no YY-ID coupling. Tokens redacted in debug/logs.
  QA scenarios: Happy: configure offline and generate launch dry-run with stable offline identity; configure fake online account and generate online launch dry-run; evidence `.omo/evidence/task-12-mcm-dyyl-launcher-redesign-v2.txt`. Failure: expired/invalid online token prevents launch without leaking token.
  Commit: Y | feat(auth): add minecraft launch auth modes

- [x] 13. Real `mcm run` launch command and execution
  What to do / Must NOT do: Finish `mcm run` so non-dry-run is implemented and dry-run is trustworthy. It must use instance metadata, runtime prep, auth mode, classpath, natives, assets, and loader args from todos 10-12. Provide safe process spawning, working directory, logs, and clear errors. Do not leave `not implemented` in non-dry-run path.
  Parallelization: Wave 4 | Blocked by: 10,11,12 | Blocks: 15,18
  References: `src/run_cmd.rs` currently returns not implemented for non-dry-run; `src/launch.rs` current command builder; PCL/HMCL replacement requirement includes launch command generation/execution.
  Acceptance criteria: tests cover dry-run command string, non-dry-run spawning with a fake Java executable, process exit propagation, log path, missing instance, missing runtime, and loader args. Real fixture dry-run must be manually inspected by test assertions.
  QA scenarios: Happy: set `JAVA` to fake executable that records argv, run `mcm run --dry-run` and `mcm run` in temp instance; evidence `.omo/evidence/task-13-mcm-dyyl-launcher-redesign-v2.txt`. Failure: fake Java exits nonzero and MCM reports exit code/log path.
  Commit: Y | feat(run): execute real minecraft launch commands

- [x] 14. dyyl streaming host protocol design and implementation
  What to do / Must NOT do: Extend `/x/dyyl` and MCM integration so dyyl can run with a streaming JSON host protocol. dyyl handles syntax/variables/logic/loops; when it reaches `mcm.*`, it emits a JSON command event and waits for MCM JSON response. Implement enough protocol for build/do/source/install contexts, error sentinels, `mcm.game.choose` session state, and version-root file semantics. Do not batch all commands at the end.
  Parallelization: Wave 5 | Blocked by: 1 | Blocks: 15,16,17
  References: `/x/dyyl/src/main.rs` currently supports `dyyl [--debug] <filename>`; `/x/dyyl/src/runtime/cmd/dispatch.rs` treats `mcm.*` as unknown; `/x/dyyl/dyyl-api-reference.md` lists MCM API but current runtime lacks it; user decided streaming host protocol, dyyl external command, language name `dyyl`.
  Acceptance criteria: dyyl tests cover `--host-json` or equivalent protocol, command event/response roundtrip, MCM error sentinel propagation, unknown command failure, and `mcm.game.choose` scoping until next choose/script end.
  QA scenarios: Happy: run dyyl fixture containing two `mcm.*` commands with a fake host and verify event order/responses; evidence `.omo/evidence/task-14-mcm-dyyl-launcher-redesign-v2.txt`. Failure: host returns error sentinel and dyyl exits with clear location.
  Commit: Y | feat(dyyl): add streaming mcm host protocol

- [x] 15. `.mcm` v2 JSON lock schema and build/make/install/do split
  What to do / Must NOT do: Replace `.mcm` v1 with v2 pure JSON lock as the package/installable lock file format. `.dyyl` is source. `mcm build <in.dyyl>` resolves latest/empty versions at build time and writes fixed JSON lock. `mcm make <out.dyyl>` exports current instance to dyyl source. `mcm install <pack.mcm>` does not run dyyl; it executes install-permitted lock steps only, silently stripping non-install steps for local installs. Server upload of mixed/do/full locks rejects instead of stripping. `mcm do` executes dyyl/MCM full commands. Preserve `source_line` for readability/audit only, not reparse source. No v1 compatibility: v1 parse attempts fail with actionable rebuild-from-dyyl error. Local client install state may reference v2 package slug/hash/source/author but is not itself the `.mcm` package schema.
  Parallelization: Wave 5 | Blocked by: 13,14 | Blocks: 16,17,18
  References: `src/mcm_package.rs` current JSON schema v1; `src/pkg_cmd.rs` current pkg make/install/do dispatch; `src/pkg_install.rs` current package install execution; user decisions on `.dyyl` source, `.mcm` lock, build/make/do/install split.
  Acceptance criteria: tests parse/validate v2 locks, reject v1, build dyyl to deterministic lock, make exports dyyl, install executes only install steps, do executes full graph, source_line retained but not executed, latest resolution fixed at build except explicitly client-resolved commands.
  QA scenarios: Happy: `mcm build sample.dyyl -o sample.mcm`, inspect JSON lock, `mcm install sample.mcm --yes`, `mcm do sample.dyyl --yes`; evidence `.omo/evidence/task-15-mcm-dyyl-launcher-redesign-v2.txt`. Failure: v1 package rejected; lock with do-only command uploaded/installed has command stripped/rejected per context.
  Commit: Y | feat(pkg): introduce mcm v2 lock and dyyl build

- [x] 16. Permission model, upload validation, and version-root file/network semantics
  What to do / Must NOT do: Implement command permission classification for install/do/full; server upload validation accepts install-only shared locks; `mcm install` silently removes no-install-permission commands before execution; `mcm do` runs full graph. Enforce `file.*` and network download target paths relative to selected game version root; reject traversal/absolute/backslash paths. `mcm.game.choose` affects only current dyyl script from that line until next choose or script end; if no selected version for version-scoped command, return error sentinel rather than global default except install/choose exceptions.
  Parallelization: Wave 5 | Blocked by: 15 | Blocks: 17,18
  References: user decisions on permission model, upload install-only, file.* version root, network download target root, choose semantics, no fallback default; `/x/dyyl/src/runtime/cmd/file.rs` currently requires absolute paths; server share upload validation from todo 5.
  Acceptance criteria: tests cover permission matrix, silent strip in install, do/full execution in do, upload rejection for non-install locks, path traversal rejection, version-root target resolution, choose reset behavior, missing choose error sentinel.
  QA scenarios: Happy: install lock with mixed commands strips do-only and completes; do same lock executes do commands; evidence `.omo/evidence/task-16-mcm-dyyl-launcher-redesign-v2.txt`. Failure: `../`, absolute path, no choose, and uploaded do-capable lock fail safely.
  Commit: Y | feat(pkg): enforce lock permissions and version roots

- [x] 17. Source weighting and provider integration for dyyl/build/install
  What to do / Must NOT do: Implement source/provider selection rule `effective_downloads = source_weight * max(raw_download_count, 1)` across mod/package/source resolution used by build/install/do. Apply to Modrinth/CurseForge/custom sources consistently. Ensure `mcm.user.source.*` naming is used, not `mcm.source.user.*`. `mcm.user.config <key>, <value>` writes global user `config.toml` `user` table with priority below current version config and above built-in defaults.
  Parallelization: Wave 5 | Blocked by: 15,16 | Blocks: 18
  References: `src/provider.rs` `Candidate`/download_count; `src/provider/composite.rs` provider aggregation/sorting; user decisions on source weighting and API naming; dyyl API reference currently needs rename.
  Acceptance criteria: provider tests prove missing/zero downloads treated as 1, source_weight changes ordering, ties deterministic, custom source capability respected, API names accepted/rejected as specified, user config precedence tested.
  QA scenarios: Happy: fixture providers with zero/missing/high downloads choose expected artifact; evidence `.omo/evidence/task-17-mcm-dyyl-launcher-redesign-v2.txt`. Failure: old `mcm.source.user.*` command errors with migration hint.
  Commit: Y | feat(provider): apply weighted source selection

- [x] 18. Documentation, operator handoff, and no-secrets audit
  What to do / Must NOT do: Update README/operator docs for CLI grammar, Web pkg share management, YY-ID/OIDC env setup, PM2 deployment, curl-bash installs, real launcher capabilities, dyyl/.mcm v2 commands, permission model, Linux-first support, and HMCL/PCL license policy. Docs must say HMCL GPLv3 code reuse is recommended for launcher-critical logic when provenance/NOTICE obligations are satisfied, while PCL direct copying remains forbidden without separate approval. Include exact commands for PM2 and curl verification. Do not include secret values; do not imply mock OIDC is production. Evidence is not committed unless scrubbed and intentionally staged.
  Parallelization: Wave 6 | Blocked by: all prior | Blocks: final verification
  References: existing README generated by prior plan; `.omo/plans/mcm-minecraft-manager-expansion.md`; all todos above; user stated worker needs concrete instructions.
  Acceptance criteria: docs include runnable examples for `mcm pkg auth/login/status/logout`, `mcm pkg share/list --mine/update/delete`, Web flows, `curl -fsSL https://mc.dyyapp.com/install | bash`, `curl -fsSL https://mc.dyyapp.com/install/pkg/<slug> | bash`, `mcm game install`, `mcm run`, `mcm make`, `mcm build`, `mcm install`, `mcm do`; no secret literal or placeholder that looks like a real secret; docs say where to inject env.
  QA scenarios: Happy: follow docs in a temp dir/local server and capture command outputs; evidence `.omo/evidence/task-18-mcm-dyyl-launcher-redesign-v2.txt`. Failure: scan repo/.omo/log evidence for token-looking accidental secrets and for any redacted-safe fingerprint supplied by the operator; never print or require the literal production secret.
  Commit: Y | docs: document launcher share dyyl and deployment flows

## Final verification wave
> Runs in parallel after ALL todos. ALL must return unconditional PASS. The worker reports results; completion does not depend on manual human QA.
- [x] F1. Plan compliance audit: read this plan and the final diff; verify every Must Have is implemented, every Must NOT Have is respected, no scope creep, every todo has evidence. Output `.omo/evidence/f1-plan-compliance-mcm-dyyl-launcher-redesign-v2.md`.
- [x] F2. Code quality review: run fmt/clippy/tests, inspect module sizes and error handling, verify no `unwrap`/panic in production paths unless justified, verify secret redaction, verify no mock-only production path. Output `.omo/evidence/f2-code-quality-mcm-dyyl-launcher-redesign-v2.md`.
- [x] F3. Real manual QA: with local server/PM2 and fake OIDC plus deploy env shape, run curl checks for `/`, static assets, `/health`, auth, share APIs, install routes; run CLI pkg flows; run browser Web flows with screenshots; run launcher dry-run/fake Java execution; run dyyl build/install/do. Output `.omo/evidence/f3-real-qa-mcm-dyyl-launcher-redesign-v2/`.
- [x] F4. Security/scope/license audit: prove no OIDC secret leak, no committed real credentials, upload validation rejects do-capable shared locks, curl-bash verifies artifacts, path traversal rejected, any HMCL-derived code has GPLv3 provenance/attribution and AGPL compatibility notes, no PCL code/assets/text copied without explicit approval, AGPL/source docs still present. Output `.omo/evidence/f4-security-scope-mcm-dyyl-launcher-redesign-v2.md`.

## Commit strategy
- Use small, reviewable commits by dependency wave. Do not commit secrets or generated evidence containing secrets.
- Suggested commits:
  1. `fix(server): serve web assets independent of cwd`
  2. `fix(install): repair verified curl bash routes`
  3. `chore(server): define secret-safe oidc deployment config`
  4. `feat(auth): add real oidc login for yy-id`
  5. `feat(share): complete package management api`
  6. `docs(deploy): add pm2 oidc and route verification`
  7. `feat(pkg): add cli share auth session commands`
  8. `feat(pkg): add cli share management`
  9. `feat(web): add package share management ui`
  10. `feat(game): resolve real minecraft versions and loaders`
  11. `feat(launch): prepare java assets libraries and natives`
  12. `feat(auth): add minecraft launch auth modes`
  13. `feat(run): execute real minecraft launch commands`
  14. `feat(dyyl): add streaming mcm host protocol`
  15. `feat(pkg): introduce mcm v2 lock and dyyl build`
  16. `feat(pkg): enforce lock permissions and version roots`
  17. `feat(provider): apply weighted source selection`
  18. `docs: document launcher share dyyl and deployment flows`

## Success criteria
- `curl -i http://127.0.0.1:8950/`, `/index.html`, `/app.js`, `/styles.css`, `/health`, `/api/share/list`, `/install`, `/install/pkg/<slug>` all return expected status/content locally after PM2 restart, independent of PM2 cwd.
- Real OIDC mode builds provider URL under `https://auth.dyyapp.com`, uses redirect `https://mc.dyyapp.com/api/auth/oidc/callback`, exchanges code for tokens, creates sessions, rejects invalid callbacks, and never logs secrets/tokens. Mock OIDC remains only test/dev.
- CLI and Web can publish/list/update/delete/download/install shared packages with identical server policy and install command display.
- Curl-bash MCM and package install flows verify artifacts before installing and delegate package execution to normal `mcm install ... --yes` semantics.
- MCM can install/manage real Minecraft instances and generate/execute launch commands on Linux x86_64 with offline default and online auth support.
- dyyl streaming host protocol works; `.dyyl` builds deterministic `.mcm` v2 JSON locks; `mcm install` executes install-only lock steps; `mcm do` executes full commands.
- Source weighting uses `source_weight * max(raw_download_count, 1)` consistently.
- Full Rust gates pass: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets --all-features`.
- License audit passes: `cargo deny check licenses` when available, or a documented equivalent if the tool is unavailable; HMCL-derived code has NOTICE/provenance entries.
- Final verification F1-F4 all approve, with no OIDC secret leakage, HMCL reuse provenance complete if any HMCL code is copied/ported, and no PCL copying without explicit separate approval.
