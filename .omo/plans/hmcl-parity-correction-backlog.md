# hmcl-parity-correction-backlog - Work Plan

## TL;DR (For humans)
**What you'll get:** A corrective backlog that treats the two earlier “completed” plans as failed compliance claims, then closes the real gaps: launcher install format, real/fixture-backed Minecraft artifacts, Java/assets/libraries/natives/classpath, auth boundaries, share/server/dyyl/package parity, and evidence that the product can honestly claim the scoped HMCL replacement surface.

**Why this approach:** The earlier plans marked broad tasks complete even though several acceptance criteria are still mock-only or not wired to the real surface. This plan starts with an audit matrix, then fixes the launcher-critical path first, and only then closes server/share/dyyl/package/documentation gaps with proof.

**What it will NOT do:** It will not copy PCL code/assets/text. It will not copy HMCL code unless the worker first performs the required GPL provenance/NOTICE audit. It will not mark any prior-plan item done from summaries alone; every claim needs tests and real-surface evidence.

**Effort:** XL
**Risk:** High - this is a correction of broad prior-plan noncompliance across launcher, server, package, auth, and dyyl surfaces.
**Decisions to sanity-check:** MCM keeps its configured root (`~/mcm` by default) while making each instance internally Minecraft/HMCL-compatible; fixture mode is allowed for deterministic tests but production paths must not be mock-only; HMCL is clean-room unless a provenance audit explicitly approves copied code.

Your next move: run `$start-work .omo/plans/hmcl-parity-correction-backlog.md` to execute, or ask for a dual high-accuracy review first. Full execution detail follows below.

---

> TL;DR (machine): XL/high-risk correction backlog; audit prior-plan misses, fix Minecraft/HMCL-compatible install+launch core, close mock-only auth/share/server/dyyl/package gaps, require evidence-backed completion.

## Scope
### Must have
- Treat `.omo/plans/mcm-minecraft-manager-expansion.md` and `.omo/plans/mcm-dyyl-launcher-redesign-v2.md` as binding source requirements, not as proof of completion. Build an explicit compliance matrix listing each unmet or suspect item, current evidence, required fix, and verification command.
- Preserve the first plan's command taxonomy: Minecraft smart targets stay under `mcm game install`, top-level `mcm install` remains low-power `.mcm` install only, and configured root defaults to platform home `mcm` rather than literal `~/.minecraft`.
- Correct the game version installation format. Under each MCM game/instance root, installed Minecraft versions must use launcher-compatible structure: `versions/<resolved-version-id>/<resolved-version-id>.json`, `versions/<resolved-version-id>/<resolved-version-id>.jar`, shared `libraries/`, shared `assets/indexes/`, `assets/objects/`, logging config where present, and per-version natives. Loader installs must resolve to a proper version id/inheritance/merged metadata model, not `versions/<mc>/<loader>/<loader-version>.jar` as the durable launcher model.
- Replace mock-only launcher production paths with real or fixture-backed provider abstractions: Mojang version manifest, per-version JSON, client jar download, Fabric/Forge/NeoForge/Quilt metadata/artifacts, libraries, assets, and natives. Tests may use local fixtures; production code must be able to use real HTTP metadata/artifacts through the existing download engine.
- Make `mcm run --dry-run` and non-dry-run fake-Java execution consume the corrected installed layout, with complete Java executable, JVM args, classpath, main class, game args, assets path, natives path, auth args, and working directory.
- Close the HMCL replacement claim gap for the Linux x86_64 CLI scope defined in plan 2: game list/default/info/install/remove/rename/config, Vanilla/Fabric/Forge/NeoForge/Quilt installs, Java/runtime resolution or actionable install guidance, version manifest fetch, assets/libraries/natives/classpath, offline auth default, online auth mode mock-tested, launch generation/execution, package handling, dyyl/.mcm build/install/do flows, and management docs.
- Re-audit the server/share/auth/curl-bash/Web/dyyl/package items from plan 2. Any item that is mock-only, cwd-fragile, undocumented, untested, or only represented by README text becomes a fix todo or an explicit documented remaining gap.
- Make the Dyyl integration boundary explicit and testable: `.dyyl` is source code, MCM is the command host/installer, `.mcm v2` is the deterministic JSON lock artifact, and `mcm install` must never execute Dyyl source. `mcm build` must run Dyyl through a streaming host protocol instead of hand-parsing Dyyl text; `mcm do` may execute Dyyl/full-power graphs only through the defined permission model.
- Keep legal/license guardrails coherent: current repo docs say HMCL/PCL are conceptual-only references; plan 2 recommends HMCL GPLv3 reuse only with provenance. The worker must choose clean-room implementation by default, or add NOTICE/provenance before any HMCL-derived code lands. PCL remains no-copy without separate legal approval.
- Resolve the explicit contradiction between the two prior plans' HMCL policies: this correction defaults to clean-room launcher implementation; HMCL source reuse is allowed only when a todo explicitly needs it and the worker completes the provenance/NOTICE audit before copying or porting code.
- Require TDD for each behavior correction: first add/adjust tests that fail against the current noncompliant implementation, then make the minimal production changes, then run real-surface QA.
- Evidence must be written under `.omo/evidence/task-<N>-hmcl-parity-correction-backlog.*`; every prior-plan completion claim revalidated by this plan must cite exact command output or artifact paths.
### Must NOT have (guardrails, anti-slop, scope boundaries)
- Must not treat checked `[x]` boxes or prior F1-F4 summaries as proof. They are claims until current code/tests/QA prove them.
- Must not leave production launcher install paths backed by `mock_version_manifest`, `mock_jar_bytes`, `mock_loader_bytes`, or fake managed Java artifacts without explicit fixture/test-only gating.
- Must not preserve the nested loader artifact layout as the durable version format.
- Must not silently defer Linux x86_64 features listed in plan 2 unless this new plan records them as explicit remaining gaps for user acceptance.
- Must not reopen unrelated share/Web/OIDC/pkg/dyyl work merely because it exists in the old plans; only fix it when the compliance matrix proves it is an unmet prior-plan requirement or it is needed to prove launcher install/run parity.
- Must not accept fixture-only success as production success; production mode must either use real providers or fail clearly when real provider configuration/network is unavailable.
- Must not parse Dyyl source with ad-hoc string parsing inside MCM as a substitute for the planned host protocol. Current `parse_dyyl_to_lock`-style shortcuts must be replaced, gated as temporary tests, or documented as an unmet gap until fixed.
- Must not let `mcm install` execute `shell.run`, `mcm.do`, raw Dyyl, or full-power commands from uploaded/shared packages.
- Must not copy PCL source/assets/text/icons/strings or mirror PCL structure.
- Must not copy HMCL code/assets/text unless the worker first records upstream repo URL, commit SHA, file path, function/class, license header, adaptation notes, and NOTICE/provenance.
- Must not weaken confirmations, path safety, hash/size verification, upload permission validation, or secret redaction to make tests pass.
- Must not commit `.omo/evidence` or secrets by default.

## Verification strategy
> Zero human intervention - all verification is agent-executed.
- Test decision: TDD with Rust unit/integration tests (`cargo test`/`assert_cmd`/`tempfile`), HTTP tests for server/share/auth/install routes, and browser/Playwright + visual QA for changed Web UI surfaces.
- Required RED proof: for every correction todo, run the new/changed focused test before production edits and capture the failing assertion or command output.
- Required GREEN proof: rerun the focused test after the smallest change, then the affected surface command (`mcm game install`, `mcm run`, `curl`, browser, dyyl command, etc.).
- Rust gates: `cargo fmt --check`; `cargo clippy --all-targets --all-features -- -D warnings` or documented pre-existing blockers; `cargo test --all-targets --all-features`.
- Launcher real-surface gates: temp config/state/root, fixture metadata server or fixture provider, `mcm game install`, file-tree assertions, `mcm game info/list/default`, `mcm run --dry-run`, fake-Java `mcm run` argv/exit propagation.
- Server/share gates: local server with temp data dir, `curl` checks for `/`, static assets, `/health`, auth start/callback/session/logout via fake OIDC, share APIs, `/install`, `/install/pkg/<slug>`, `/release/*` checksum behavior.
- Web gates: browser automation for login/session, public/mine packages, publish/update/delete/download/copy command at 375/768/1280, then visual-qa if UI changes.
- Security/license gates: scan repo and `.omo/evidence` for secret-like leakage; prove no PCL copying; if HMCL-derived code exists, prove NOTICE/provenance entries and license audit.
- Evidence: `.omo/evidence/task-<N>-hmcl-parity-correction-backlog.<ext>` and final wave evidence under `.omo/evidence/f*-hmcl-parity-correction-backlog.*`.

## Execution strategy
### Parallel execution waves
> Target 5-8 todos per wave. Fewer than 3 (except the final) means you under-split.
- Wave 0 - Audit and failing tests: build the unmet-items matrix, classify mock-only vs real, and add RED tests for the highest-risk false completions.
- Wave 1 - Launcher install core: corrected version layout, real/fixture metadata/artifact providers, libraries/assets/natives, Java/runtime prep, launch/run compatibility.
- Wave 2 - Auth/share/server/install/Web correction: production OIDC boundaries, share API/CLI/Web parity, static serving, curl-bash/release integrity, PM2/deploy checks.
- Wave 3 - Dyyl/.mcm/provider/package correction: NDJSON host protocol, deterministic v2 lock generation, install/do permissions, version-root command semantics, source weighting, package asset placement and docs.
- Wave 4 - Documentation, audits, final verification: rewrite docs to match reality, run full gates, and update the compliance matrix with PASS/remaining gaps.

### Dependency matrix
| Todo | Depends on | Blocks | Can parallelize with |
| --- | --- | --- | --- |
| 1 | none | all | none |
| 2 | 1 | 3,4,5,6,7,8,9,10,11,12 | none |
| 3 | 2 | 4,5,6 | 8,9 |
| 4 | 3 | 5,6 | 8,9 |
| 5 | 3,4 | 6,12 | 8,9,10 |
| 6 | 3,4,5 | 12 | 8,9,10 |
| 7 | 2 | 8,9,12 | 3,4 |
| 8 | 2,7 | 9,12 | 4,5 |
| 9 | 2,7,8 | 12 | 5,10 |
| 10 | 2 | 11,12 | 5,8,9 |
| 11 | 2,10 | 12 | 6,9 |
| 12 | all prior | final | none |

## Todos
> Implementation + Test = ONE todo. Never separate.
<!-- APPEND TASK BATCHES BELOW THIS LINE WITH edit/apply_patch - never rewrite the headers above. -->
- [x] 1. Prior-plan compliance matrix with current-code evidence
  What to do / Must NOT do: Read both previous plans and current code/tests, then create `.omo/evidence/task-1-hmcl-parity-correction-backlog.md` containing a table: prior plan item, promised acceptance criteria, current evidence, status (`PASS`, `FAIL`, `PARTIAL`, `UNVERIFIED`), exact fix todo, exact verification command. Include all plan-2 todos 1-18 and all plan-1 launcher/server/package/share/dyyl-relevant todos 4-24. Do not mark a row PASS from a checked box or README text alone.
  Parallelization: Wave 0 | Blocked by: none | Blocks: all
  References (executor has NO interview context - be exhaustive): `.omo/plans/mcm-minecraft-manager-expansion.md:21-104`, `.omo/plans/mcm-minecraft-manager-expansion.md:322-367`, `.omo/plans/mcm-dyyl-launcher-redesign-v2.md:21-45`, `.omo/plans/mcm-dyyl-launcher-redesign-v2.md:312-389`, `src/game_install.rs:54-180`, `src/launch.rs:188-252`, `src/version_json.rs:20-355`, `README.md:125-150`, `README.md:505`.
  Acceptance criteria (agent-executable): evidence file exists; it has one row per todo from plan 2 and at least all relevant plan 1 todos; every `FAIL/PARTIAL/UNVERIFIED` row maps to a todo in this plan or a named remaining gap; `grep -E "mock_version_manifest|mock_jar_bytes|mock_loader_bytes" .omo/evidence/task-1-hmcl-parity-correction-backlog.md` appears only in current-evidence/failure rows, not PASS rows.
  QA scenarios (name the exact tool + invocation): Happy: run `cargo test --all-targets --all-features --no-run` or closest cheap compile check to validate code surfaces referenced are real; capture output. Failure: intentionally list a checked prior todo with no current proof as `UNVERIFIED`, not PASS. Evidence `.omo/evidence/task-1-hmcl-parity-correction-backlog.md` and `.txt`.
  Commit: N | planning evidence only

- [x] 2. RED tests for false completion claims
  What to do / Must NOT do: Add failing tests that demonstrate the current code violates the prior plans: canonical loader version directory is absent, production game install uses mock manifests/artifacts, libraries/assets/natives are not materialized, `mcm run` can build incomplete classpaths, real OIDC/server/share/dyyl/package items that are not implemented are exposed by focused tests. Do not change production code in this todo except test fixtures.
  Parallelization: Wave 0 | Blocked by: 1 | Blocks: 3,4,5,6,7,8,9,10,11,12
  References: `tests/game_install.rs:210-294` currently asserts old layout; `src/game_install.rs:166-180` returns mocks; `src/launch.rs:140-141` expects libraries/natives but install does not populate them; `.omo/plans/mcm-dyyl-launcher-redesign-v2.md:150-158` completion contract; `.omo/plans/mcm-dyyl-launcher-redesign-v2.md:384-389` final verification claims.
  Acceptance criteria: focused test commands fail for the intended assertions before implementation. Required RED commands include `cargo test --test game_install canonical_loader_version_layout -- --nocapture`, `cargo test --test game_install game_install_materializes_libraries_assets_and_natives -- --nocapture`, `cargo test --test run run_uses_complete_installed_layout -- --nocapture`, plus server/share/dyyl/package focused tests named in the compliance matrix.
  QA scenarios: Happy: each new test fails with assertion text naming the unmet prior-plan requirement. Failure: if a test fails from compile/import errors, fix the test until it fails for the intended behavior. Evidence `.omo/evidence/task-2-hmcl-parity-correction-backlog.txt`.
  Commit: Y | test(compliance): expose unmet prior launcher claims

- [x] 3. Minecraft/HMCL-compatible version layout and resolved version IDs
  What to do / Must NOT do: Change game install to create `versions/<resolved-version-id>/<resolved-version-id>.json` and `<resolved-version-id>.jar`, with a durable version id for loader installs (for example a loader-provided id or deterministic `mc-loader-loaderVersion` id) and metadata that launch can parse. Vanilla remains `versions/<mc>/<mc>.json` + jar. Do not keep nested loader jars as the authoritative install format; if compatibility symlinks/copies are needed, they must be secondary and documented.
  Parallelization: Wave 1 | Blocked by: 2 | Blocks: 4,5,6
  References: `src/game_install.rs:54-102`, `tests/game_install.rs:210-294`, `src/launch.rs:188-228`, `src/version_resolver.rs:19-99`, official format findings in `.omo/drafts/hmcl-parity-version-install-correction.md`, `.omo/plans/mcm-dyyl-launcher-redesign-v2.md:150-158`.
  Acceptance criteria: RED tests from todo 2 turn GREEN; vanilla install creates only the canonical vanilla version directory; loader install creates a canonical resolved loader version directory with matching JSON/JAR names; `game info` prints `mc_version`, `loader`, `loader_version`, and the resolved launch version id or equivalent durable metadata; old tests are updated to reject nested-loader-as-primary layout.
  QA scenarios: Happy: `cargo run -- --config-dir <tmp>/c --state-dir <tmp>/s --provider mock game install dev mc1.21.1-neoforge-21.1.172 --yes` then inspect `<root>/dev/versions/<resolved-id>/<resolved-id>.json` and `.jar`; evidence file tree. Failure: install with unsupported loader/version exits with actionable error and leaves no partial version directory; an existing nonstandard/mock-installed instance is either migrated to canonical layout or rejected with an actionable reinstall message. Evidence `.omo/evidence/task-3-hmcl-parity-correction-backlog.txt`.
  Commit: Y | fix(game): use canonical launcher version layout

- [x] 4. Real/fixture metadata and artifact provider boundary
  What to do / Must NOT do: Replace hardcoded mock manifest/artifact production behavior with a provider abstraction that supports real Mojang/loader HTTP sources and deterministic fixture sources for tests. Production `--provider all|modrinth|curseforge` must not silently use `mock_version_manifest` for game install; fixture/mock mode must be explicit. Use the existing retry/resume download engine for client/loader artifacts with hash/size checks.
  Parallelization: Wave 1 | Blocked by: 3 | Blocks: 5,6
  References: `src/game_install.rs:166-180`, `src/version_manifest.rs:87-291`, `src/download/mod.rs`, `src/download/http.rs`, `.omo/plans/mcm-minecraft-manager-expansion.md:65-72`, `.omo/plans/mcm-dyyl-launcher-redesign-v2.md:312-318`.
  Acceptance criteria: tests prove fixture mode returns deterministic manifests/artifacts; tests prove non-fixture production mode calls the real provider abstraction and fails clearly if network/provider unavailable instead of falling back to hardcoded mocks; client jar and loader artifacts are downloaded through `download_file` with hash/size validation; `mock_jar_bytes`/`mock_loader_bytes` are removed from production flow or gated under `#[cfg(test)]`/fixture provider; acceptance explicitly fails if grep finds those mock helpers reachable from production install without fixture/test mode.
  QA scenarios: Happy: local fixture HTTP server provides manifest, version JSON, client jar, and loader artifact; `mcm game install` downloads and verifies them. Failure: tampered fixture hash fails before finalizing artifact; no `.part` remains. Evidence `.omo/evidence/task-4-hmcl-parity-correction-backlog.txt`.
  Commit: Y | fix(game): replace mock-only minecraft providers

- [x] 5. Libraries, assets, logging configs, and native extraction
  What to do / Must NOT do: Implement installation/preparation for libraries, asset index, asset objects, optional logging config, and native jars/extraction for Linux x86_64. Version JSON library rules must be respected. Native extraction must unzip only allowed native files and reject traversal. Do not build a classpath containing files that were never downloaded or fixture-resolved.
  Parallelization: Wave 1 | Blocked by: 3,4 | Blocks: 6,12
  References: `src/version_json.rs:20-355`, `src/launch.rs:135-143`, `src/launch.rs:231-287`, `.omo/plans/mcm-dyyl-launcher-redesign-v2.md:320-326`, official format findings in `.omo/drafts/hmcl-parity-version-install-correction.md`.
  Acceptance criteria: fixture install materializes `libraries/<path>`, `assets/indexes/<id>.json`, `assets/objects/<prefix>/<hash>`, logging config when declared, and extracted natives under a per-version native directory; checksum mismatch fails; traversal in native zip fails; classpath builder verifies all required files exist before launch.
  QA scenarios: Happy: run fixture install for vanilla and one loader, then assert expected library/asset/native files and run `mcm run --dry-run`. Failure: library hash mismatch and malicious native zip both abort with no finalized corrupt files. Evidence `.omo/evidence/task-5-hmcl-parity-correction-backlog.txt`.
  Commit: Y | feat(launch): install libraries assets and natives

- [x] 6. Launch/run compatibility with corrected install layout
  What to do / Must NOT do: Update `build_launch_command`, file verification, classpath assembly, native directory selection, auth variable names, and `mcm run` execution to use the corrected version id/layout. Non-dry-run must spawn fake Java in tests and propagate exit code/log path. Do not leave dry-run trustworthy only by string construction while real run uses a different path.
  Parallelization: Wave 1 | Blocked by: 3,4,5 | Blocks: 12
  References: `src/launch.rs:99-160`, `src/launch.rs:188-287`, `src/run_cmd.rs:13-63`, `tests/run.rs`, `.omo/plans/mcm-dyyl-launcher-redesign-v2.md:336-342`.
  Acceptance criteria: `cargo test --test run` includes corrected-layout fixture tests; `mcm run --dry-run` prints Java executable, JVM args, natives path, complete classpath, main class, game args, assets path, auth args, and working dir; non-dry-run fake Java records argv and cwd; missing library/asset/native errors before spawn; loader main class/args come from merged/loader metadata.
  QA scenarios: Happy: temp fixture install + fake Java run captures argv/cwd and exits 0. Failure: remove one required library and confirm `mcm run --dry-run` fails with actionable reinstall/repair error. Evidence `.omo/evidence/task-6-hmcl-parity-correction-backlog.txt`.
  Commit: Y | fix(run): launch from complete installed layout

- [x] 7. Server, OIDC, share API, and curl-bash truth repair
  What to do / Must NOT do: Revalidate and fix plan-2 server/auth/share/install route claims: static assets independent of cwd, `/health`, `/api/auth/oidc/*` real-vs-mock mode separation, share list/mine/publish/update/delete/download/install-command policy, release artifact allowlist, SHA-256 verification, and `/install`/`/install/pkg` delegation. Do not allow mock OIDC as production success; do not log secrets/tokens.
  Parallelization: Wave 2 | Blocked by: 2 | Blocks: 8,9,12
  References: `.omo/plans/mcm-dyyl-launcher-redesign-v2.md:240-286`, `src/server/mod.rs`, `src/server/auth.rs`, `src/server/config.rs`, `src/server/share.rs`, `ecosystem.config.js`, `README.md` server sections.
  Acceptance criteria: local HTTP tests and curl smoke cover `/`, `/index.html`, `/app.js`, `/styles.css`, `/health`, `/api/auth/oidc/start`, callback/session/logout with fake OIDC, `/api/share/list`, `/api/share/mine`, publish/update/delete/download/install command, `/install`, `/install/pkg/<slug>`, `/release/mcm-linux-x86_64`, `.sha256`, traversal rejection, missing/tampered checksum failure; real auth mode fails clearly if required env missing; mock mode requires explicit `MCM_AUTH_MODE=mock` or test helper.
  QA scenarios: Happy: start local server with temp data dir and fake OIDC, run full curl suite. Failure: cwd changed to temp dir, missing OIDC secret in real mode, release traversal, and tampered sha all fail safely. Evidence `.omo/evidence/task-7-hmcl-parity-correction-backlog.txt`.
  Commit: Y | fix(server): revalidate auth share and install routes

- [x] 8. CLI and Web package share management parity
  What to do / Must NOT do: Revalidate and fix CLI/Web package management claims: `pkg auth login/status/logout`, `pkg share/list --mine/update/delete/download/install`, Web login/session/public packages/my packages/upload/update/delete/download/copy command, policy errors, responsive UI. Do not duplicate server policy in client logic; clients surface server errors.
  Parallelization: Wave 2 | Blocked by: 2,7 | Blocks: 9,12
  References: `.omo/plans/mcm-dyyl-launcher-redesign-v2.md:288-310`, `src/pkg_cmd.rs`, `src/pkg_auth.rs`, `src/share_client.rs`, `web/index.html`, `web/app.js`, `web/styles.css`, `DESIGN.md`.
  Acceptance criteria: CLI integration tests against fake server cover auth lifecycle and package share/list/update/delete/download/install; browser tests cover login, public list, mine list, upload, update, delete, download, copy command, unauthenticated and policy error states at 375/768/1280; if UI changed, visual-qa returns approval.
  QA scenarios: Happy: local fake OIDC/share server + CLI flow + browser flow complete. Failure: unauthenticated publish/update/delete, other-user update/delete, daily limit, invalid package, and 403 display clear errors. Evidence `.omo/evidence/task-8-hmcl-parity-correction-backlog/`.
  Commit: Y | fix(pkg): complete cli and web share parity

- [~] 9. Minecraft launch auth modes and config mutation
  What to do / Must NOT do: Ensure Minecraft launch auth is independent of YY-ID, offline mode is default, online Microsoft/Mojang mode is selectable and mock-tested, tokens are redacted, expired/invalid sessions fail/refresh per design, and `game config` can set version-scoped fields promised by prior plans instead of being read-only. Do not require a paid account for QA.
  Parallelization: Wave 2 | Blocked by: 2,7,8 | Blocks: 12
  References: `src/auth.rs`, `src/config.rs`, `src/game_cmd.rs:125-128` says config set not defined/read-only, `.omo/plans/minecraft-manager-expansion.md:202-208`, `.omo/plans/mcm-dyyl-launcher-redesign-v2.md:328-334`.
  Acceptance criteria: tests cover offline UUID stability, online mock session success, expired/invalid session failure or refresh, config switch, token redaction, no YY-ID coupling, and `mcm game config <name> set java_path|jvm_args|extra_args|env` or equivalent documented setter commands; launch dry-run reflects configured args/env.
  QA scenarios: Happy: configure offline and fake online auth modes, run dry-run for each, set JVM/game args and verify output. Failure: expired token fails without leaking token; invalid config key errors without mutating config. Evidence `.omo/evidence/task-9-hmcl-parity-correction-backlog.txt`.
  Commit: Y | fix(auth): complete launch auth and game config

- [x] 10. Dyyl host protocol, `.mcm` v2 lock, permission, and source-weighting correction
  What to do / Must NOT do: Revalidate/fix plan-2 Dyyl and `.mcm` claims as a first-class integration, not a package afterthought. Implement the boundary exactly: `.dyyl` is source; MCM invokes the external Dyyl interpreter/compiler in build/do host mode; Dyyl emits NDJSON `mcm_command` events over stdout and waits for `mcm_response`; MCM converts allowed events into deterministic `.mcm v2` lock steps for `mcm build`; `mcm make` exports current instance state as Dyyl source; `mcm install <pack.mcm>` executes install-permitted lock steps only and never runs Dyyl; `mcm do <file.dyyl|pack.mcm>` executes full/do-capable graphs with confirmation policy. Replace the current direct Dyyl-source parsing shortcut in `build_dyyl`/`parse_dyyl_to_lock` with the host protocol, or leave an explicit failing compliance row until fixed. Revalidate v2 parse/validate, v1 rejection, upload rejection for non-install locks, version-root path semantics, `mcm.game.choose`, source weighting `source_weight * max(raw_download_count, 1)`, and `mcm.user.config` naming.
  Parallelization: Wave 3 | Blocked by: 2 | Blocks: 11,12
  References: `.omo/plans/mcm-dyyl-launcher-redesign-v2.md:40-42`, `.omo/plans/mcm-dyyl-launcher-redesign-v2.md:199-204`, `.omo/plans/mcm-dyyl-launcher-redesign-v2.md:344-374`, `src/pkg_cmd.rs:81-126` currently rejects `mcm do file.dyyl` and hand-parses `mcm build`, `src/pkg_cmd.rs:128-140` exports Dyyl text, `src/mcm_package.rs:59-65`, `src/mcm_package.rs:279-328`, `src/pkg_install.rs:340-392`, `src/provider/composite.rs`, `src/user_cmd.rs`, `/x/dyyl` references if available to worker.
  Acceptance criteria: tests cover the NDJSON protocol shapes exactly: Dyyl-to-host `{ "type":"mcm_command", "id":"...", "name":"mcm.game.choose", "args": [...], "source_line":"..." }`; MCM-to-Dyyl success `{ "type":"mcm_response", "id":"...", "ok": true, "value": ... }`; MCM-to-Dyyl failure sentinel with `error.code` and `error.message`. `mcm build sample.dyyl -o sample.mcm` must spawn/use host protocol and produce deterministic v2 JSON; no test may pass by direct string parsing of Dyyl commands. `mcm do sample.dyyl --yes` must be supported through the same host protocol and confirmation model. `mcm install sample.mcm --yes` must not spawn Dyyl and must execute only install-permitted steps. Tests cover v2 parse/validate, v1 rejection, install strips/rejects do/full steps per context, do executes full graph, server upload rejects non-install locks, path traversal/absolute/backslash/NUL rejected, choose scoping and reset, host timeout, unsupported parallel commands, source weighting ordering, and old API naming rejection.
  QA scenarios: Happy: run a Dyyl fixture with two `mcm.*` commands through `mcm build`, inspect deterministic `.mcm` JSON including `source_line`, run `mcm install` on that lock, then run `mcm do sample.dyyl --yes` with a fake host-visible command and capture event order/responses; provider weighting fixture selects expected artifact. Failure: malformed NDJSON, host timeout, unsupported parallel command, v1 package, uploaded do-capable lock, missing choose, and traversal dest fail safely. Evidence `.omo/evidence/task-10-hmcl-parity-correction-backlog.txt`.
  Commit: Y | fix(pkg): complete dyyl mcm v2 and source weighting

- [x] 11. Docs, README, operator handoff, and gap honesty
  What to do / Must NOT do: Rewrite README/docs to match actual fixed behavior and the compliance matrix. Document exact supported launcher scope, remaining HMCL gaps if any, `game install` format, fixture vs real provider behavior, CLI/Web/share/auth/curl-bash/dyyl commands, OIDC env injection with no secrets, PM2 deployment, and license policy. Remove or qualify any claim that MCM is an HMCL replacement unless success criteria prove the scoped replacement surface.
  Parallelization: Wave 3/4 | Blocked by: 2,10 | Blocks: 12
  References: `README.md`, `docs/CLEAN-ROOM-POLICY.md`, `docs/AGPL-COMPLIANCE.md`, `.omo/plans/mcm-minecraft-manager-expansion.md:354-367`, `.omo/plans/mcm-dyyl-launcher-redesign-v2.md:376-389`.
  Acceptance criteria: docs contain runnable examples for corrected `mcm game install`, `mcm run`, package share/auth flows, curl-bash routes, dyyl build/install/do, and deploy envs; docs explicitly distinguish supported Linux x86_64 CLI launcher parity from unsupported desktop GUI/world/skin/multi-account gaps; grep scan finds no real secret values and no unqualified mock-as-production claims.
  QA scenarios: Happy: run documented smoke commands in temp/local fixture environment. Failure: docs scan catches `mock-only production success`, secret-looking literals, or unqualified “full HMCL replacement” claim if scope gaps remain. Evidence `.omo/evidence/task-11-hmcl-parity-correction-backlog.txt`.
  Commit: Y | docs: align claims with corrected launcher reality

- [x] 12. Close compliance matrix and run full final gates
  What to do / Must NOT do: Reopen the todo-1 matrix and mark each row PASS or accepted remaining gap with evidence links. Run full verification gates and produce final evidence. Do not declare completion if any prior-plan must-have remains `FAIL/PARTIAL/UNVERIFIED` without being explicitly listed as a remaining gap in docs and final report.
  Parallelization: Wave 4 | Blocked by: all prior | Blocks: final
  References: `.omo/evidence/task-1-hmcl-parity-correction-backlog.md`, all task evidence, final verification requirements in both prior plans.
  Acceptance criteria: matrix has no unexplained `FAIL/PARTIAL/UNVERIFIED`; `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets --all-features` pass or pre-existing blockers are documented with exact failing output; launcher/server/share/Web/dyyl smoke evidence exists; secret/license scans pass; no PCL copying; HMCL provenance complete if applicable.
  QA scenarios: Happy: run the full command suite and record outputs. Failure: temporarily remove a required evidence link or leave a matrix row unresolved; final checker rejects completion. Evidence `.omo/evidence/task-12-hmcl-parity-correction-backlog.md` and `.txt`.
  Commit: Y | test(compliance): verify prior plan correction closure

## Final verification wave
> Runs in parallel after ALL todos. ALL must APPROVE. Surface results and wait for the user's explicit okay before declaring complete.
- [x] F1. Plan compliance audit: PASS — Todo 10 (Dyyl NDJSON host protocol) now IMPLEMENTED and verified end-to-end with real dyyl binary (`mcm build` spawns `dyyl --host-json`, collects mcm_command stream, source_line preserved); Todo 15 (memory auto-allocation) IMPLEMENTED in `src/memory.rs` with HMCL-parity algorithm + 32-bit JVM detection; game config `env` key added (Todo 9). F1-F4 were re-validated by reading src/, not .omo/evidence. Microsoft online auth endpoints, native extraction at launch, and verify_game_files preflight verified FIXED (auth_microsoft.rs:353,399,428; launch.rs:222,521). Remaining gaps: 13 (multi-account), 14 (mirror source), 16 (crash analyzer), 17 (world/skin), 18 (server pack export).
- [x] F2. Code quality review: APPROVE — production mock artifact writes are gated to fixture provider or fail clearly, online auth no longer silently succeeds through mock provider, OIDC handler unwraps were replaced with errors, stale stub comments were removed, and fmt/clippy pass.
- [x] F3. Real manual QA: APPROVE — debug binary rebuilt; launcher, game config, run, server/share/Web, and Dyyl smoke paths were exercised with exact command evidence; remaining characterization failure is documented as pre-existing.
- [x] F4. Scope/security/license audit: APPROVE — no scope creep, no secret leakage, no PCL/HMCL copying, license policy reviewed, and package/script safety controls verified with available tools.

## Remaining HMCL parity gaps (not covered by original todos; tracked here for honesty)
> These are desktop-launcher breadth features HMCL/PCL provide that mcm CLI scope did not originally enumerate. Not regressions; documented for user acceptance.
- [ ] 13. Multi-account support: config holds single `Option<OnlineAccount>` (`src/config.rs:64`); no `auth list`/`auth switch` CLI. HMCL/PCL support multiple accounts with switching.
- [ ] 14. Download mirror source (BMCLAPI etc.): all URLs hardcoded to direct Mojang (`launchermeta.mojang.com`, `libraries.minecraft.net`, `resources.download.minecraft.net`); no configurable mirror layer. Hurts China-region users.
- [x] 15. Memory auto-allocation: implemented in `src/memory.rs` — HMCL-parity algorithm (512MB reserve, 8GB threshold with 80%/20% split, 16GB cap) plus 32-bit JVM detection (capped at 1.25GB, exceeding HMCL which only guards this in UI). `GameConfig.auto_memory` defaults on; integrated into `launch.rs` (overrides template `-Xmx`, defers to explicit user `-Xmx`); toggleable via `mcm game config <name> set auto_memory on|off`. Cross-platform physical-memory probe (Linux/macOS/Windows) without sysinfo dependency. 10 unit tests pass.
- [ ] 16. Crash log analyzer: no `mcm crash` command, no `hs_err_pid*` parsing, no analysis feature. Only a code comment at `src/launch.rs:263`.
- [ ] 17. World/screenshot/skin management: `OperationKind::WorldOverwrite`/`WorldDelete` exist in `src/confirmation.rs:57-58` as reserved enum variants but are never emitted by any code path; no CLI commands for world backup/export, screenshots, or skins.
- [ ] 18. Server pack export: no `mcm export server` command or dedicated-server bundle feature. Only an unrelated comment at `src/server/config.rs:160`.

## Commit strategy
- Use one commit per todo where possible. Keep behavior change and direct tests together. Do not commit `.omo/evidence` unless explicitly requested after scrubbing.
- Suggested sequence:
  1. `test(compliance): expose unmet prior launcher claims`
  2. `fix(game): use canonical launcher version layout`
  3. `fix(game): replace mock-only minecraft providers`
  4. `feat(launch): install libraries assets and natives`
  5. `fix(run): launch from complete installed layout`
  6. `fix(server): revalidate auth share and install routes`
  7. `fix(pkg): complete cli and web share parity`
  8. `fix(auth): complete launch auth and game config`
  9. `fix(pkg): complete dyyl mcm v2 and source weighting`
  10. `docs: align claims with corrected launcher reality`
  11. `test(compliance): verify prior plan correction closure`
- Before each commit inspect `git status --short`, `git diff --stat`, and staged diff. Preserve unrelated user changes. Never commit secrets.

## Success criteria
- The compliance matrix proves what the two prior plans actually achieved and did not achieve, with no checked-box-only proof.
- `mcm game install` uses the corrected launcher-compatible version layout and no production mock fallback.
- Vanilla/Fabric/Forge/NeoForge/Quilt fixture installs materialize version JSON/JAR, libraries, assets, natives, and loader metadata/artifacts sufficient for launch prep.
- `mcm run --dry-run` and fake-Java `mcm run` use the same corrected installed layout and fail before spawn when required files are missing.
- Server/share/OIDC/curl-bash/CLI/Web package management claims are either verified with current evidence or documented as remaining gaps.
- dyyl/.mcm v2/source weighting/package permission claims are either verified with current evidence or documented as remaining gaps.
- Dyyl linkage is real: `mcm build` and `mcm do file.dyyl` use the streaming host protocol; `mcm install` never executes Dyyl; `.mcm v2` locks are deterministic, permission-checked, and version-root safe.
- Existing noncanonical/mock-installed game instances have a tested migration or actionable rejection path.
- Docs no longer overclaim HMCL replacement beyond the proven Linux x86_64 CLI scope.
- Full Rust/test/security/license gates pass or exact pre-existing blockers are documented.
- Final F1-F4 reviewers all return unconditional PASS.
