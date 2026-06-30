# F1: Plan Compliance Audit — mcm-minecraft-manager-expansion

Reviewed: 2026-06-28  
Auditor: Final Verification Wave (read-only)  
Plan: `.omo/plans/mcm-minecraft-manager-expansion.md`  
Evidence dir: `.omo/evidence/`

---

## Build Gates (verified live)

| Gate | Result |
|------|--------|
| `cargo test` | 503 passed, 0 failed |
| `cargo fmt --check` | PASS (no output) |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| Pure LOC >250 production | NONE (all oversized files are test-heavy with SIZE_OK justification) |

---

## Task Compliance Table (Tasks 1–24)

| Task | Status | Evidence File | Key Verification |
|------|--------|---------------|------------------|
| 1. Baseline characterization | ✅ COMPLETE | `task-1-*.txt` | 44 characterization tests, 13 mvp, 2 help, 14 lib — all pinned |
| 2. Split oversized architecture | ✅ COMPLETE | `task-2-*.txt` | 18 focused modules, src/lib.rs is 17-line re-export hub, all 73 tests pass |
| 3. AGPL and license compliance | ✅ COMPLETE | ⚠️ NO evidence file | LICENSE = AGPLv3, deny.toml configured, docs state HMCL/PCL clean-room (verified from source) |
| 4. CLI grammar and help skeleton | ✅ COMPLETE | `task-4-*.txt` | All commands in `--help`: install/upgrade/full-upgrade/source/pkg/game/do/run/config/mods/serve; mc_target parser with 17+9 tests |
| 5. Typed config model | ✅ COMPLETE | `task-5-*.txt` | GameRecord/GlobalConfig, game default/list/info/rename/config/remove, 28 tests |
| 6. .mcm package schema | ✅ COMPLETE | `task-6-*.txt` | McmPackage typed schema, secret rejection, path traversal protection, 30 tests |
| 7. Confirmation policy | ✅ COMPLETE | `task-7-*.txt` | OperationKind enum (15 variants), AUTOREMOVE_WARNING "may break worlds/saves/modded structures", 21 tests |
| 8. Source config CLI | ✅ COMPLETE | `task-8-*.txt` | source add/remove/info/list, zero default sources, 12 tests |
| 9. Source index format | ✅ COMPLETE | ⚠️ NO evidence file | SourceIndex typed schema, 13 tests pass, integrated into source_cmd/source_service (verified from source) |
| 10. Package install/download/make/share | ✅ COMPLETE | `task-10-*.txt` | pkg_install/download/make/share/list, top_install, do_file, 29 tests |
| 11. Import/export modpack formats | ✅ COMPLETE | `task-11-*.txt` | .mrpack and CurseForge manifest import, mrpack export, 14 tests |
| 12. HTTP service shell | ✅ COMPLETE | `task-12-*.txt` | Axum+tokio, share/source/both modes, 127.0.0.1:8950 default, 15 tests |
| 13. Server storage outside /x | ✅ COMPLETE | `task-13-*.txt` | SQLite+filesystem, refuse_under_x enforced, slug uniqueness, 2-day reservation, 16 tests |
| 14. OIDC auth + publish API | ✅ COMPLETE | `task-14-*.txt` | Mock OIDC, AuthedOwner, 5 pkg/user limit, 1 push/day, no admin token/Turnstile, 16 tests |
| 15. Source service routes | ✅ COMPLETE | `task-15-*.txt` | SourceStore, index/meta/blob routes, client source resolution, 12 tests |
| 16. Web UI with getdesign | ✅ COMPLETE | `task-16-*.txt` + PNGs | DESIGN.md exists, SPA login/dashboard/publish/update/delete, browser QA screenshots at 375/768/1280 |
| 17. /install bootstrap route | ✅ COMPLETE | `task-17-*.txt` | POSIX shell script, checksum verification, dry-run, unsupported OS/arch exit, 15 tests |
| 18. /install/pkg/<name> route | ✅ COMPLETE | `task-18-*.txt` | Package install script, validates slug, delegates to mcm install --yes, 7 tests |
| 19. Retry/resume download engine | ✅ COMPLETE | ⚠️ NO evidence file | Download engine with backoff+staging+hash, 6 download tests pass (verified from source) |
| 20. Minecraft version/loader install | ✅ COMPLETE | `task-20-*.txt` | game install with mc/mc1.21.1/mc-neoforge/mc1.21.1-neoforge-21.1.172, 4 loaders, dry-run, 21+28 tests |
| 21. Java runtime discovery/install | ✅ COMPLETE | `task-21-*.txt` | JavaMajor compatibility matrix (8/17/21), discover_java, install_managed_java, 35+12 tests |
| 22. Launch command builder | ✅ COMPLETE | `task-22-*.txt` | launch.rs (202 prod LOC), build_launch_command, mock auth, LaunchOnInstall confirmation wired, 16+10+7 tests |
| 23. Upgrade/full-upgrade | ✅ COMPLETE | `task-23-*.txt` | upgrade.rs + upgrade_deps.rs, owner mismatch refused, dependency-unsatisfied skipped, 10 upgrade tests |
| 24. Deployment docs | ✅ COMPLETE | `task-24-*.txt` | README.md 327 lines, all 12 sections, no secrets/Turnstile in docs |

---

## Task 22 Deep Scrutiny (required by plan)

### Acceptance Criteria Checklist

| Criterion | Status | Evidence |
|-----------|--------|----------|
| `mcm run --dry-run` prints stable launch command | ✅ | Evidence file S1: java path, classpath, main class, args all present |
| Mock Microsoft auth/session contributes expected fields | ✅ | Evidence file S6: uuid=00000000-..., accessToken=mock-access-token, sessionType=Mojang |
| Missing game/runtime/auth errors are actionable | ✅ | Evidence file S2/S3/S4: specific guidance messages |
| Package-requested launch requires confirmation unless --yes | ✅ | Corrective in evidence: `require_confirmation(OperationKind::LaunchOnInstall, yes)` in pkg_install.rs:38-41 |
| LaunchOnInstall tests (RED→GREEN) | ✅ | 5 new tests: pkg_install_with_launch_yes, pkg_download_with_launch_does_not_launch, pkg_install_without_launch_skips, pkg_install_with_launch_no_tty_bails, top_install_with_launch |

### Production Code Verified

- `src/pkg_install.rs:38-41`: `if pkg.launch.is_some() { require_confirmation(OperationKind::LaunchOnInstall, yes)?; println!("launch-on-install confirmed"); }`
- `src/pkg_download()` does NOT trigger LaunchOnInstall (download-only never launches) — tested

---

## Task 23 Deep Scrutiny (required by plan)

### Acceptance Criteria Checklist

| Criterion | Status | Evidence |
|-----------|--------|----------|
| Mock tests upgrade one game only | ✅ | `upgrade_one_game_old_mods_upgraded` passes |
| full-upgrade iterates all games | ✅ | `full_upgrade_two_games_both_upgraded` passes |
| Upgrade without --yes prompts | ✅ | `upgrade_without_yes_prints_plan_and_bails` passes |
| autoremove warning includes "may break worlds/saves" | ✅ | AUTOREMOVE_WARNING constant in confirmation.rs:28-30 |
| Locked manual/auto reasons preserved | ✅ | `upgrade_preserves_install_reasons` passes |
| Incompatible/dependency-unsatisfied updates skipped | ✅ | `incompatible_dep_installed_blocks_upgrade` + `required_dep_missing_skips_upgrade` pass |
| Owner-ID mismatch reported and refused | ✅ | `owner_mismatch_refused` passes |
| Dry-run prints plan | ✅ | `upgrade_without_yes_prints_plan_and_bails` asserts plan output |

### Production Code Verified

- `src/upgrade_deps.rs`: Full `check_dependency_satisfaction` implementation covering Required/Incompatible/Unknown/Embedded/Optional dependency kinds
- `src/upgrade.rs:148`: `check_dependency_satisfaction(&available, &lock, &planned_ids)` — stub removed, real logic
- `src/upgrade.rs`: Two-pass plan building (collect items → filter by dependency satisfaction)
- `src/lock.rs`: `owner_id: Option<String>` on InstalledMod with `#[serde(default, skip_serializing_if)]` for backward compatibility

---

## Must Have Compliance

| Requirement | Status | Notes |
|-------------|--------|-------|
| Preserve current behavior semantics | ✅ | 503 tests pass including 44 characterization |
| Refactor from single-file | ✅ | 18 modules, lib.rs is 17-line hub |
| CLI command taxonomy complete | ✅ | All commands in --help |
| ~/mcm default root | ✅ | GlobalConfig with UserDirs |
| .mcm schema-versioned JSON | ✅ | McmPackage, schema_version=1 |
| Share/source server on 8950 | ✅ | Axum service, share/source/both modes |
| Web UI with DESIGN.md | ✅ | DESIGN.md exists, SPA built, browser QA done |
| OIDC auth for publish | ✅ | Mock OIDC, no admin token/Turnstile |
| Installer routes | ✅ | /install + /install/pkg/{slug} |
| Minecraft version/loader install | ✅ | mc targets, 4 loaders, mock manifests |
| Java runtime discovery/install | ✅ | Compatibility matrix, managed install |
| Launch dry-run | ✅ | build_launch_command, mock auth |
| Retry/resume downloads | ✅ | Download engine with backoff+staging |
| Confirmation policy | ✅ | OperationKind enum, AUTOREMOVE_WARNING |
| AGPLv3 license | ✅ | LICENSE file, AGPLv3 |
| No HMCL/PCL code copied | ✅ | Zero matches in src/ for HMCL/PCL strings |

---

## Must NOT Have Compliance

| Guardrail | Status | Evidence |
|-----------|--------|----------|
| No HMCL/PCL code/assets copied | ✅ | grep for HMCL/Plain Craft Launcher/PCL2 in src/ = 0 matches |
| No real secrets committed | ✅ | grep for sk-/xox-/ghp_/AIza/BEGIN PRIVATE KEY/password in src/tests/Cargo.toml = 0 real secrets |
| No Turnstile/admin-token requirement for publish | ✅ | Only documentation comments mentioning "no Turnstile required"; no code requiring admin token |
| No unsafe source defaults | ✅ | Fresh install has zero custom sources; config.rs:31 documents "no author source preinstalled" |
| No auto-overwrite worlds/saves | ✅ | WorldOverwrite/WorldDelete are OperationKind variants with MC-critical warnings and confirmation |
| No server storage under /x | ✅ | refuse_under_x() enforced in config + storage init; test proves /x/mcm-share rejected |
| No curl|bash unverified binaries | ✅ | Bootstrap script verifies SHA-256 via sha256sum/shasum/openssl fallback |
| No moderation/admin dashboards/payment/GUI | ✅ | No such code in src/ |
| No real-account auth testing | ✅ | Mock Microsoft/Mojang auth in src/auth.rs |
| No oversized files without justification | ✅ | All files >250 LOC have SIZE_OK comments; production-only LOC under 250 for all |

---

## Final Verification Wave (F1-F4) Dependencies

- **F1 (this audit)**: APPROVE
- F2 (code quality): Separate reviewer
- F3 (real QA): Separate reviewer
- F4 (scope/security/license): Separate reviewer

---

## Notes

1. **Task 22 corrective (launch-on-install)**: The original Task 22 worker implemented the launch builder but did not wire `OperationKind::LaunchOnInstall` to `pkg_install()`. This was identified by Atlas, and a corrective worker added the 4-line gate in `pkg_install.rs:38-41` plus 5 tests. Evidence: `task-22-*.txt` lines 117-184.

2. **Task 23 corrective (dependency satisfaction)**: The original Task 23 worker implemented upgrade/full-upgrade with a stub `check_dependency_satisfaction`. A corrective worker created `src/upgrade_deps.rs` (47 LOC) with real logic for Required/Incompatible/Unknown/Embedded/Optional kinds, plus 2 new tests. Evidence: `task-23-*.txt` lines 107-174.

3. **Missing evidence files (Tasks 3, 9, 19)**: These three tasks lack dedicated `.omo/evidence/task-N-*.txt` files. However, their implementations are verified from source code:
   - Task 3 (AGPL/license): LICENSE is AGPLv3 (verified), deny.toml exists with permissive-license-only policy, README documents HMCL/PCL clean-room rule.
   - Task 9 (source index): `src/source_index.rs` (383 lines, typed schema, boundary parser), 13 source_index tests pass, integrated into source_cmd and source_service flows.
   - Task 19 (download engine): `src/download/mod.rs` (243 lines, retry+backoff+resume+staging+hash), 6 download tests pass (flaky-server-retry, hash-mismatch-cleanup, resume-from-part, etc.).
   
   These implementations are not stubs — they are real, tested, and integrated. The evidence files are a documentation gap, not an implementation gap. The functionality is also covered by downstream tasks' evidence (Task 12 uses download engine, Task 15 uses source index, Task 24 confirms license docs).

4. **Test count**: 503 total tests pass across 22 test files + lib unit tests. All deterministic (mock provider, temp dirs, no network). fmt clean. clippy clean.

5. **LOC compliance**: 8 source files exceed 250 total LOC (confirmation.rs 272, game_install.rs 295, install.rs 435, launch.rs 339, runtime.rs 616, source_index.rs 259, version_manifest.rs 279, version_resolver.rs 276). ALL are test-heavy: production-only LOC under 250 for every file. All have SIZE_OK justification or are inherently test-fixture files.

---

VERDICT: APPROVE
