## 2026-06-28T21:33:26+08:00 Session Start - mcm-dyyl-launcher-redesign-v2
- Plan has 18 implementation tasks + 4 final verification tasks
- OIDC: issuer=https://auth.dyyapp.com, redirect=https://mc.dyyapp.com/api/auth/oidc/callback
- Current PM2 server cwd issue: static files return 404
- MCM is Rust project with cargo, web assets in web/ dir

## 2026-06-28 Task 2: Release artifact and curl-bash route repair
- Release filenames changed from mcm-x86_64-linux.tar.gz to mcm-linux-x86_64 (bare binary)
- Bootstrap script uses #!/bin/bash + set -euo pipefail (not #!/bin/sh)
- SHA-256 verification via sha256sum -c (with shasum/openssl fallbacks)
- No auto-sudo: script prints exact sudo command for unwritable paths
- Package slug validated via validate_package_name (only [a-z0-9-])
- __test_router_full had pre-existing bug: web_dir not in scope → fixed with resolve_web_dir()
- parse_auth_mode had pre-existing bug: invalid values silently accepted → now rejected
- characterization.rs has 1 pre-existing snapshot mismatch (warning format change)


## Task 4: Real YY-ID/Casdoor OIDC flow

### Key decisions:
- **LoginStore** replaces PendingStates for both mock and real modes. Single shared store tracks login_id → (state, status) with reverse index state → login_id.
- **Route dispatch by AuthMode**: `auth::routes(auth_mode)` builds different router trees for Mock vs Real. Clean separation.
- **JWT validation without signature verification**: Decodes payload only (base64url + JSON). Issuer/audience/expiry validated. Signature verification delegated to HTTPS transport (token endpoint is TLS).
- **is_pending() as guard**: Callback uses `is_pending()` check before `complete()` to prevent wasted session tokens on replayed states.
- **One-shot poll consumption**: Complete/Expired/Denied results consumed on first read. Pending returns without consuming.

### Gotchas:
- `SessionStore::issue()` was `fn` (private) — needed to make it `pub(super)` for oidc.rs callback to issue sessions.
- Mock callback must complete login in LoginStore (for CLI poll), not just return session directly.
- axum `.nest()` not `.route()` for mounting sub-routers.
- `find_by_state()` doesn't check consumed flag — use `is_pending()` for callback gate.
- Pre-existing `bail!` compilation errors in `storage/helpers.rs` are NOT from this task.

### Patterns:
- Test helper functions must be `async` when they use `tokio::task::spawn_blocking`.
- `#[allow(dead_code)]` with `reason` for reserved API methods.
- `SecretString` debug redaction pattern is critical for security.

## Task 5: Share API Completeness (2026-06-28)

### Key Patterns
- `validate_payload()` in helpers.rs is the central validation gate for publish/update. Changed return type from `Result<()>` to `Result<ContentMetadata>` to extract name/description without re-parsing.
- SQLite migration: use individual ALTER TABLE statements with `let _ = conn.execute_batch(stmt)` to tolerate "duplicate column" errors gracefully.
- PackageMeta install_command is computed from slug, not stored in DB.
- sha256 computed via `crate::util::sha256_hex()` (pub(crate) from util.rs).

### Gotchas
- Task 4 (OIDC) runs in parallel — `tests/server_auth.rs` has compilation errors from their changes. My tests only run in `tests/server_storage.rs`.
- `bail!` macro needs explicit import: `use anyhow::{anyhow, bail, Context, Result};`
- `ContentMetadata` is used only internally — don't export from `mod.rs`.

### Architecture
- Storage layer (mod.rs) is the single source of truth for all metadata operations.
- Share handler (share.rs) is thin — delegates to storage, adds HTTP concerns only.
- DB schema uses `CREATE TABLE IF NOT EXISTS` + ALTER TABLE migration pattern.

## Task 9: Web pkg share management UI

### Testid verification (all present on correct pages)
- Login page: `login-yyid`, `error-banner`
- Dashboard: `session-owner`, `logout`, `public-packages`, `my-packages`
- Publish page: `package-upload`, `error-banner`
- Detail page: `copy-install-command`, `package-update`, `package-delete`

### Responsive design
- No horizontal overflow at 375px, 768px, 1280px
- Package items wrap vertically on mobile (max-width: 640px)
- Install snippet shows inline on mobile

### Key implementation details
- Delete uses custom modal (not browser confirm())
- Detail page fetches list + package data in parallel to determine owner
- File upload zone supports drag-and-drop and click-to-select
- Session check determines ownership for My Packages vs Public Packages

### Server setup for testing
- Need MCM_WEB_DIR, MCM_AUTH_MODE=mock, MCM_SHARE_DATA_DIR env vars
- Server runs on port 8950

## Task 15: .mcm v2 JSON lock schema and build/make/install/do split

### Key Design Decisions
- v2 lock uses `kind: "mcm-lock"` field to distinguish from v1
- Schema version check happens BEFORE serde deserialization to give actionable v1 error
- `permissions` uses `#[serde(rename = "do")]` for the do_permitted field (Rust keyword avoidance)
- Step args use `serde_json::Value` for flexibility across different step operations
- `parse_mcm_package()` wraps `parse_mcm_lock()` for backward compat during migration
- Server `validate_payload` extracts metadata from `identity.name` with fallback to top-level `name`
- Server validates v2 step permissions for shared packages (rejects non-install steps)

### Files Changed
- `src/mcm_package.rs` - Complete replacement: v1 types → v2 lock types
- `src/cli.rs` - Added Build/Make commands
- `src/app.rs` - Wired build/make dispatch
- `src/pkg_cmd.rs` - Complete rewrite for v2 lock operations
- `src/pkg_install.rs` - Complete rewrite for v2 step execution
- `src/source_resolve.rs` - Creates McmLock v2 instead of McmPackage v1
- `src/server/storage/helpers.rs` - Updated validation for v2 format
- `src/lib.rs` - Updated exports
- `src/i18n.rs` - Added new messages

### Gotchas
- v1 rejection must happen BEFORE serde deserialization (v1 JSON lacks required v2 fields)
- `StepPermission` is not exported by default from lib.rs - need explicit export for tests
- `ContentMetadata` needed `#[derive(Debug)]` for test assertions using `unwrap_err()`
- Server metadata extraction must handle both v2 `identity.name` and fallback to v1 `name`
- `now_rfc3339()` function must be `pub(crate)` for use by source_resolve.rs

### Test Pattern
- All v1 format fixtures replaced with v2 format across 9 test files
- Server validation tests updated to v2 JSON structure
- Pre-existing failures: characterization.rs snapshot mismatch, parse_auth_mode_invalid (now passing)

## Task 16: Permission model, upload validation, and version-root file/network semantics

### Key Design Decisions
- Path validation runs at parse time (in `parse_mcm_lock`) via `validate_lock_step_paths()`
- `VersionContext` struct tracks game.choose scope during lock execution
- `game.choose` resolves to `~/mcm/{game}/{version}` as the version root
- `do_lock()` executes ALL step types (install + do + full), root.system requires NonBypassable confirmation
- `apply_lock()` silently skips non-install steps (no error, no warning)
- Server upload validation in `validate_payload()` → `validate_install_only()` rejects non-install steps with 400

### Files Changed
- `src/mcm_package.rs` - Added `validate_step_dest_path()`, `validate_lock_step_paths()`, `validate_lock_install_only()`, `StepPermission::as_str()`, Display trait
- `src/pkg_install.rs` - Added `VersionContext`, `execute_step()`, `execute_game_choose()`, `execute_file_copy()`, `execute_file_write()`, `execute_net_download()`, `execute_config_set()`, `execute_root_system()`, `resolve_version_root()`; rewrote `apply_lock()` and `do_lock()`
- `src/lib.rs` - Exported new public functions
- `tests/mcm_package.rs` - Added 20+ tests for path validation, permission matrix, version-root resolution, CLI surface

### Gotchas
- `dirs` crate is NOT a dependency; use `directories::UserDirs` instead
- CLI-surface tests need `home.profile()` before running install/do commands (requires active profile)
- `pkg install` path requires active profile; `install` (top-level) also requires it
- Pre-existing `upgrade.rs` errors from Task 17 (source_weights) already fixed in file
- Pre-existing `user_cmd.rs` errors from Task 17 (missing i18n functions) already fixed in file

### Test Results
- 55 mcm_package tests pass
- 229 lib tests pass
- 25 server_storage tests pass
- 30 pkg_cmd tests pass
- clippy clean (no warnings)
