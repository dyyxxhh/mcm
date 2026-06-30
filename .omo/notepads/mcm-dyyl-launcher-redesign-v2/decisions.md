## 2026-06-28T21:33:26+08:00 Execution Strategy
- Wave 1: Tasks 1 (no deps) -> Tasks 2,3 (blocked by 1)
- Wave 2: Tasks 4,5 (blocked by 1,2)
- Wave 3: Tasks 6,7,8,9
- Wave 4: Tasks 10,11,12,13
- Wave 5: Tasks 14,15,16,17
- Wave 6: Task 18


## Task 5 Decisions

### Metadata Extraction
**Decision**: Extract name/description from `body.content` (serde_json::Value) in the handler, pass to storage.
**Rationale**: Avoids double-parsing the content bytes. validate_payload already parses for secrets; returning ContentMetadata reuses that parse.

### Install-only Validation
**Decision**: Reject packages with non-empty `actions`, non-null `launch`, or non-null `local` fields.
**Rationale**: Task requirement says "only accept install-only .mcm lock files". Scripts and launch config are not install directives.

### DB Schema Migration
**Decision**: Add columns via ALTER TABLE with error tolerance.
**Rationale**: SQLite's CREATE TABLE IF NOT EXISTS doesn't add new columns. ALTER TABLE fails silently on duplicate columns, which we ignore.

### install_command Format
**Decision**: Hardcoded `curl -fsSL https://mc.dyyapp.com/install/pkg/<slug> | bash`.
**Rationale**: Matches the install script in pkg.rs. Server doesn't have a configurable base URL yet.
