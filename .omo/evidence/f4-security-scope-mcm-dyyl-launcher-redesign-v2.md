# F4: Security/Scope/License Audit — MCM DYyl Launcher Redesign v2

**Date:** 2026-06-29
**Scope:** Full codebase security, credential, upload validation, path traversal, curl-bash, license compliance

---

## 1. No OIDC Key Leaks ✅

### Findings

- **No private keys or certificates committed** to any source file. Grep for `BEGIN (RSA|EC|DSA|OPENSSH) PRIVATE KEY` and `BEGIN CERTIFICATE` returns zero matches across all `.rs`, `.ts`, `.js`, `.json`, `.toml`, `.yaml`, `.yml`, `.env` files.

- **OIDC secrets read only from environment variables**, never from files or hardcoded:
  - `src/server/config.rs:141` — `env::var("MCM_OIDC_CLIENT_SECRET")` reads from process env.
  - `src/server/config.rs:139-144` — All four OIDC vars (`MCM_OIDC_ISSUER`, `MCM_OIDC_CLIENT_ID`, `MCM_OIDC_CLIENT_SECRET`, `MCM_OIDC_REDIRECT_URL`) read exclusively from `env::var()`.

- **SecretString redaction wrapper** prevents accidental logging:
  - `src/server/config.rs:64-80` — `SecretString` wraps the OIDC client secret; its `Debug` impl returns `<redacted>`.
  - `src/server/config.rs:113-124` — Manual `Debug` for `ServerConfig` uses the redacted `SecretString` for `oidc_client_secret`.
  - `src/server/config.rs:89-91` — Doc comment confirms: "OIDC client secret is a SecretString whose Debug impl is <redacted>."

- **OIDC client secret never reaches CLI or HTTP responses:**
  - `src/server/auth/oidc.rs:17-19` — Doc: "OIDC client secrets are NEVER logged or included in responses."
  - `src/server/auth/oidc.rs:96` — Secret accessed only server-side via `config.oidc_client_secret.as_ref().unwrap().as_str()` for token exchange.
  - Token exchange happens over HTTPS; CLI never sees the secret.

- **ecosystem.config.js has only commented placeholders** — no real secrets:
  - `ecosystem.config.js:9` — Comment: "Secrets (MCM_OIDC_CLIENT_SECRET) must NOT be stored here."
  - `ecosystem.config.js:25-28` — All four OIDC vars are commented out with placeholder values.

- **No `.env` files exist** in the repository (glob for `**/.env*` returns empty).

- **Git history clean** — `git log -p --all -S 'client_secret'` shows no real secret values; all matches are env var names or documentation.

**Verdict: PASS** — No OIDC keys leaked. Secrets flow through environment variables only, wrapped in SecretString with redacted Debug output.

---

## 2. No Committed Real Credentials ✅

### Findings

- **Grep for hardcoded credentials** (`(client_secret|api_key|apikey|password|passwd|token)\s*[:=]\s*["'][^"']+["']`) across all source files finds:
  - `src/auth.rs` — Test-only mock tokens (`"0"`, `"super-secret-token"`, `"real-token"`, `"token123"`, `"expired-token"`, `"token"`, `"ignored"`, `"real-ms-token"`, `"expired"`). These are test fixtures in `#[cfg(test)]` blocks, not real credentials.
  - `src/launch.rs:535` — `"real-token"` in test data.
  - `src/run_cmd.rs:444` — `"real-token"` in test data.
  - `tests/run.rs:250,287` — `"should-be-ignored"` and `"ms-access-token-123"` in test TOML fixtures.

- **CURSEFORGE_API_KEY** (`src/provider/curseforge.rs:28`) read from `std::env::var("CURSEFORGE_API_KEY")` — never hardcoded.

- **No `.env`, `.pem`, `.key`, or `.secret` files** exist in the repository.

- **`.gitignore` does not explicitly exclude secrets files** (no `.env*` pattern), but no such files exist to be ignored. The gitignore covers: `/target/`, `/.omo/`, `/.codegraph/`, `/.playwright-mcp/`, `*.log`, `*.pid`, `.DS_Store`.

**Verdict: PASS** — All token values in source are test fixtures (inside `#[cfg(test)]`). All real credentials read from environment variables. No sensitive files committed.

---

## 3. Upload Validation — Do-Capable/Shared Lock Rejection ✅

### Findings

- **`StepPermission` enum** (`src/mcm_package.rs:89-109`):
  - Three variants: `Install`, `Do`, `Full`.
  - `is_install_permitted()` returns `true` only for `Install`.

- **`validate_lock_install_only()`** (`src/mcm_package.rs:279-293`):
  - Iterates all steps; rejects any step where `!step.permission.is_install_permitted()`.
  - Error message: "non-install step (permission: {permission}, op: {op}) is not allowed in shared packages; use `mcm do` for full-power execution."

- **Server-side `validate_payload()`** (`src/server/storage/helpers.rs:23-46`):
  - Called on every publish/update.
  - `scan_for_secrets(&value)?` — rejects packages containing secret-like fields.
  - `validate_install_only(&value)?` — rejects non-install steps.

- **`validate_install_only()`** (`src/server/storage/helpers.rs:75-123`):
  - Rejects v1 schema with actionable error message.
  - Rejects any step with `permission != "install"`.
  - Rejects non-empty `actions` array.
  - Rejects non-null `launch` config.
  - Rejects non-null `local` data.

- **Test coverage** (`src/server/storage/helpers.rs:166-179`):
  - `validate_payload_rejects_non_install_steps` — payload with `"permission":"do"` is rejected.
  - `validate_payload_rejects_actions` — payload with non-empty actions is rejected.

- **Client-side** (`src/pkg_install.rs:113`) — `is_install_permitted()` check before executing any step.

**Verdict: PASS** — Do-capable (`Do`, `Full` permission) steps are rejected at both server-side `validate_payload()` and client-side `validate_lock_install_only()`. Actions, launch config, and local data also rejected on upload.

---

## 4. curl-bash Verification Artifacts ✅

### Findings

- **curl-bash routes documented and implemented:**
  - Bootstrap: `curl -fsSL https://mc.dyyapp.com/install | bash` (README.md:474).
  - Package install: `curl -fsSL https://mc.dyyapp.com/install/pkg/<package-name> | bash` (README.md:482).

- **Slug validation prevents injection** (`src/server/install/pkg.rs:82-86`):
  - Doc: "The slug has already been validated by `validate_package_name` so only `[a-z0-9-]` characters reach this function."
  - Slug embedded inside single quotes: `SLUG='{slug}'` — no shell metacharacter injection possible.
  - No raw untrusted input reaches shell execution context.

- **`generate_pkg_script()`** (`src/server/install/pkg.rs:87-129`):
  - Uses `set -euo pipefail`.
  - Slug assigned to shell variable via single-quoted literal.
  - Package URL constructed from validated slug + `PACKAGE_BASE_URL`.
  - Bootstrap step uses `curl -fsSL "${PACKAGE_BASE_URL}/install" | bash` with quoted URL.

- **`web/app.js`** uses `encodeURIComponent(p.slug)` and `encodeURIComponent(slug)` before embedding in curl commands — preventing XSS in the web UI.

- **Test coverage** (`tests/server_install.rs:133`): Comment confirms "Must NOT pipe unverified binary execution: curl | sh, wget -O- | sh, etc."

**Verdict: PASS** — curl-bash artifacts are correctly generated with slug validation, single-quoting, and `set -euo pipefail`. Web UI uses `encodeURIComponent()`.

---

## 5. Path Traversal Rejection ✅

### Findings

- **`validate_asset_path()`** (`src/mcm_package.rs:257-269`):
  - Rejects: empty paths, null bytes, `..` traversal, absolute paths (`/` prefix), backslashes (`\`).
  - Validates each path component against Windows reserved names (CON, PRN, AUX, NUL, COM1-9, LPT1-9).

- **`validate_step_dest_path()`** (`src/mcm_package.rs:275-276`):
  - Delegates to `validate_asset_path()`.

- **`validate_lock_step_paths()`** (`src/mcm_package.rs:298-327`):
  - Validates `dest` for `file.copy`, `file.write`, `net.download` steps.
  - Validates `url` for `net.download` (must be non-empty).
  - Validates `cwd` for `shell.run` steps.

- **Release file handler** (`src/server/mod.rs:216-224`):
  - `ALLOWED_RELEASE_FILES` whitelist: only `mcm-linux-x86_64` and `mcm-linux-x86_64.sha256`.
  - Unknown filenames return 404 immediately — no path joining with user input.

- **Blob slug validation** (`src/server/source_store.rs:96-112`):
  - Rejects: empty, `/`, `\`, `..`, null bytes, absolute paths, Windows drive letters.

- **Native jar path traversal check** (`src/launch.rs:409-421`):
  - Canonicalizes both jar path and natives dir.
  - Rejects if jar resolves inside natives dir (prevents symlink traversal).

- **Filename sanitization** (`src/safety.rs:119-137`):
  - Test: `sanitize_filename_rejects_traversal_and_non_jars` — rejects `../evil.jar`, `nested/evil.jar`, `nested\\evil.jar`, `/evil.jar`, `C:evil.jar`, `.txt` files.

**Verdict: PASS** — Path traversal protection is comprehensive: asset paths, step destinations, release file serving, blob slugs, native jar extraction, and filename sanitization all reject traversal attempts.

---

## 6. HMCL-Derived Code — GPLv3 Provenance & AGPL Compatibility ✅

### Findings

- **No HMCL code in the codebase.** Grep for `HMCL|hmcl` across all `.rs` files returns zero matches. No Java-style code, no HMCL-specific data structures, no HMCL-specific algorithms.

- **`docs/CLEAN-ROOM-POLICY.md`** explicitly documents:
  - HMCL license: "GPL-3.0 with additional terms" (HMCL AUTHORS addition to Section 7).
  - Policy: "Direct code reuse (copying any HMCL source file, function, or snippet) requires a separate license review and is FORBIDDEN in this project unless explicitly authorized in writing by the HMCL authors."
  - AGPL/GPLv3+extra compatibility note: "even though AGPLv3 and GPLv3 share a compatibility mechanism (AGPLv3 §13 allows linking/combining with GPLv3 works), the additional terms in HMCL's GPLv3+extra make any direct code incorporation a legal risk without explicit permission."
  - Permitted: "Conceptual reference is allowed. You may read HMCL's UI/UX patterns to understand what a launcher looks like, but you must not reproduce its implementation."

- **`README.md`** states: "HMCL and PCL are conceptual UX and product references only. No HMCL or PCL code, UI text, assets, icons, strings, or implementation structure is copied."

**Verdict: PASS** — No HMCL-derived code present. GPLv3 provenance documented with AGPL compatibility assessment in `docs/CLEAN-ROOM-POLICY.md`.

---

## 7. No PCL Code/Assets Copied Without Explicit Approval ✅

### Findings

- **No PCL code in the codebase.** Grep for `PCL|pcl|Plain.?Craft` across all `.rs` files returns zero matches.

- **`docs/CLEAN-ROOM-POLICY.md`** explicitly documents:
  - PCL license: "Custom restricted license" that "explicitly forbids: Redistribution of the source or binary; Copying of code, assets, strings, icons, or any other component; Creating derivative works."
  - Policy: "NO code, assets, strings, icons, UI layout files, or implementation structure from PCL/PCL2 may be copied into MCM under any circumstances."
  - "PCL/PCL2 may be used as a conceptual UX reference only."
  - Contributor rules: "Do NOT copy-paste any HMCL/PCL source code into MCM files."

- **No PCL-style C#/.NET patterns** in the Rust codebase — no WPF/XAML patterns, no .NET-specific serialization, no PCL-specific algorithms.

**Verdict: PASS** — No PCL code, assets, or implementation patterns present. Clean-room policy documented and enforced.

---

## 8. AGPL/Source Documentation Exists ✅

### Findings

- **`LICENSE`** — Full AGPL-3.0 text present at project root (661 lines).

- **`Cargo.toml:5`** — `license = "AGPL-3.0-or-later"`.

- **`docs/AGPL-COMPLIANCE.md`** (85 lines):
  - Explains AGPLv3 Section 13 network interaction obligation.
  - Covers three use cases: CLI (no source required), network service (source required), modified distributions (standard GPLv3 terms).
  - Lists practical compliance steps: keep repo public, push changes to public fork, ensure AGPLv3-compatible combined works.
  - Links to resources: GNU AGPLv3 FAQ, official text, Choose a License, SPDX.

- **`deny.toml`** — cargo-deny license audit configuration:
  - Only permissive OSI-approved licenses allowed for dependencies (MIT, Apache-2.0, ISC, Zlib, BSD, etc.).
  - Includes `AGPL-3.0-or-later` for the workspace crate itself.
  - Comments: "Only permissive OSI-approved licenses are allowed. Any license not in this list (including GPL-family copyleft) will be rejected."
  - No copyleft exceptions currently configured.

- **`README.md`** states: "The project is AGPLv3 licensed (see `LICENSE`). Source availability is required for hosted services under AGPLv3 section 13."

**Verdict: PASS** — AGPL documentation complete: LICENSE text, compliance guide, dependency audit config, and README statement all present.

---

## Summary

| Check | Result | Evidence |
|-------|--------|----------|
| No OIDC key leaks | ✅ PASS | env-only reads, SecretString redaction, no private keys |
| No committed real credentials | ✅ PASS | test fixtures only, env vars for production, no .env files |
| Upload rejects do-capable shared locks | ✅ PASS | validate_payload() rejects permission≠install, actions, launch, local |
| curl-bash verification | ✅ PASS | slug validation, single-quoting, encodeURIComponent, set -euo pipefail |
| Path traversal rejected | ✅ PASS | validate_asset_path, release whitelist, blob slug, native jar canonicalization |
| HMCL provenance/attribution | ✅ PASS | CLEAN-ROOM-POLICY.md, no code in source, AGPL compatibility documented |
| No PCL code copied | ✅ PASS | CLEAN-ROOM-POLICY.md, no PCL references in source |
| AGPL/source documentation | ✅ PASS | LICENSE, AGPL-COMPLIANCE.md, deny.toml, README |

**Overall: ALL CHECKS PASS**
