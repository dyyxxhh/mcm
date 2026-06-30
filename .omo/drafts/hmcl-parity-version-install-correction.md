# hmcl-parity-version-install-correction draft

status: awaiting-approval
pending_action: scaffold and write `.omo/plans/hmcl-parity-version-install-correction.md` after explicit approval

## Components ledger

- C1 HMCL replacement gap audit — status: evidence-found — outcome: identify minimum CLI/launcher behaviors that currently block “HMCL replacement” positioning without copying HMCL code/assets/implementation.
- C2 Game version installation format correction — status: evidence-found — outcome: make `game install` produce the version layout required by the first plan / Minecraft launcher convention.
- C3 Launch/runtime compatibility proof — status: evidence-found — outcome: ensure installed layout is consumable by `mcm run --dry-run` and future launch path.
- C4 Legal clean-room guardrail — status: evidence-found — outcome: HMCL/PCL may be conceptual UX/product references only; no direct code/text/assets/internal structures copied. Evidence: `README.md:505`, `docs/CLEAN-ROOM-POLICY.md:71-85`.

## Findings

- Initial direct glob missed hidden `.omo` contents, but explorer found two existing completed plans: `.omo/plans/mcm-minecraft-manager-expansion.md` and `.omo/plans/mcm-dyyl-launcher-redesign-v2.md`.
- First plan requires smart targets under `game install`, not top-level install (`.omo/plans/mcm-minecraft-manager-expansion.md:25-37`), default local root under `~/mcm` (`:38`), and “Version install model using Mojang version manifests” with loader install model, Java runtime, launch builder, and mocked auth tests (`:65-70`).
- Second plan is stricter: PCL/HMCL replacement feature parity must include real instance lifecycle, Vanilla/Fabric/Forge/NeoForge/Quilt installs, Java/runtime resolution, version manifest fetch, assets/libraries/natives/classpath, offline auth default, Microsoft/Mojang online auth switch, launch generation/execution, package handling, and dyyl/.mcm flows (`.omo/plans/mcm-dyyl-launcher-redesign-v2.md:38`).
- Second plan explicitly says no deferrals for listed Linux x86_64 features (`.omo/plans/mcm-dyyl-launcher-redesign-v2.md:39,44`) and completion requires downloaded or fixture-resolved client jar, version JSON, libraries, asset index/assets, loader libraries, and natives (`:150-158`).
- Git history is unavailable: current `master` has no commits and the whole tree is untracked, so plan provenance cannot be recovered from commits.
- README defines `game install` smart target grammar at `README.md:125-150`, including `mc`, `mc1.21.1`, `mc-neoforge`, `mc1.21.1-neoforge`, `mc1.21.1-neoforge-21.1.172`, and same grammar for fabric/forge/quilt; no `@latest` suffix.
- README states MCM is an apt-like Minecraft manager and game instances live under `~/mcm` by default (`README.md:1-17`), not explicitly under `.minecraft`.
- Current `src/game_install.rs:54-75` creates `<global root>/<game>/versions/<mc_version>/<mc_version>.json` and `<mc_version>.jar`, but uses mock manifests and mock jar bytes (`src/game_install.rs:166-180`, `src/version_manifest.rs:87-291`).
- Current loader install layout is nested under the vanilla version directory: `<root>/<game>/versions/<mc_version>/<loader>/<loader>-<loader_version>.jar` (`src/game_install.rs:77-102`), rather than a distinct combined version id directory such as `<version-id>/<version-id>.json` + jar.
- Official/HMCL-compatible layout uses `versions/<id>/<id>.json`, `versions/<id>/<id>.jar`, shared `libraries/`, `assets/indexes/<assetIndexId>.json`, `assets/objects/<hash-prefix>/<hash>`, and natives under a launcher-chosen per-version natives dir; HMCL also supports `jar` field fallback, local per-version libraries, `inheritsFrom`/patch merging, and version settings.
- Current launch verifier expects the same nested loader jar layout (`src/launch.rs:188-228`) and classpath appends that nested jar (`src/launch.rs:246-252`), so version-format correction must update launch checks too.
- `src/version_json.rs` can parse Mojang-style version JSON, library rules, arguments, asset index refs, native classifiers, classpath building, and interpolation (`src/version_json.rs:20-355`), but current install does not download real libraries/assets/natives.
- `tests/game_install.rs:210-294` asserts the current vanilla and nested loader layout, so tests currently encode the non-compliant format.
- Current HMCL parity blockers found by implementation mapping: real Mojang manifest fetch is mock-only, client jar is mock-only, loader jars are mock-only, libraries/assets downloads are not implemented, natives extraction is partial/copy-only, Microsoft OAuth is mock-only, managed Java install is mock-only, and `game config` is read-only.
- README clean-room/legal constraint: HMCL/PCL are conceptual references only; no HMCL/PCL code, UI text, assets, icons, strings, or implementation structure copied (`README.md:505`).

## Open decisions

- Decision D1 default: keep the first plan's MCM root (`~/mcm` / configured `global.root_dir`), not literal `~/.minecraft`, but make each instance subtree Minecraft/HMCL-compatible internally: `versions/<resolved-version-id>/<resolved-version-id>.json`, `versions/<resolved-version-id>/<resolved-version-id>.jar`, shared `libraries`, `assets`, per-version natives.
- Decision D2 default: this repair plan focuses on the launcher-critical subset that blocks the previous plans from truthfully claiming HMCL replacement on Linux x86_64: real/fixture-resolved manifests, client jar, loader metadata/artifacts, libraries, assets, natives, launch/run compatibility, and explicit gap reporting for broader HMCL features; no desktop GUI expansion.
- Decision D3 default: do not copy HMCL source code in this correction plan; use official Mojang/wiki.vg format and clean-room behavioral facts, because existing docs require conceptual-only HMCL/PCL use and the current repair is small enough to avoid provenance complexity.

## Verification strategy draft

- TDD: add failing integration tests first for canonical version-id directory layout for vanilla and loader installs; add tests proving libraries/assets/natives are materialized or fixture-resolved, not merely referenced.
- Surface QA: run `mcm --provider mock game install dev mc1.21.1-neoforge-21.1.172 --yes`, inspect exact files under temp `--config-dir` root, run `mcm game info dev`, run `mcm run --dry-run`, and run fake-Java non-dry-run launch evidence where current harness permits.
- Full checks: targeted `cargo test --test game_install`, `cargo test --test run`, launcher/version unit tests, then `cargo test --all-targets --all-features`; `cargo fmt --check`; `cargo clippy --all-targets --all-features -- -D warnings` if the repo is clippy-clean or document pre-existing blockers.

## Approval brief

Recommended approach: write a corrective implementation plan that treats prior plan 2 as binding, fixes the mocked/non-compliant launcher install core, and records broader HMCL parity blockers instead of pretending they are done. The plan will require tests to fail first against the current nested-loader/mock-artifact layout, then make install produce a Minecraft/HMCL-compatible instance subtree under MCM's configured root, with launch verification updated accordingly. Out of scope: desktop GUI, PCL copying, HMCL source copying, and unrelated package/share/server features unless needed to prove launcher install/run parity.
