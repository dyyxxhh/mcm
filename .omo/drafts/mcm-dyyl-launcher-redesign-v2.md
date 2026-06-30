---
slug: mcm-dyyl-launcher-redesign-v2
status: plan-written-awaiting-user-next-step
intent: clear
pending-action: write .omo/plans/mcm-dyyl-launcher-redesign-v2.md
approach: one architecture-scale plan covering real launcher core, dyyl streaming host + .mcm v2, MCM share server static/deploy fixes, real YY-ID/Casdoor/OIDC auth, CLI and Web pkg share management, curl-bash online install repair, and concrete PCL/HMCL replacement feature parity.
---

# Draft: mcm-dyyl-launcher-redesign-v2

## Components (topology ledger)
<!-- Lock the SHAPE before depth. One row per top-level component that can succeed or fail independently. -->
<!-- id | outcome (one line) | status: active|deferred | evidence path -->
- server-deploy | MCM server serves health, Web static assets, share API, install routes, and release artifacts independent of PM2 cwd | active | `.omo/plans/mcm-dyyl-launcher-redesign-v2.md`
- yyid-auth | Production YY-ID/Casdoor OIDC replaces mock auth while keeping mock for tests/dev only | active | `.omo/plans/mcm-dyyl-launcher-redesign-v2.md`
- share-management | CLI and Web both support pkg publish/list/update/delete/download/install-link management with identical server policy | active | `.omo/plans/mcm-dyyl-launcher-redesign-v2.md`
- launcher-core | MCM becomes a concrete PCL/HMCL product replacement for real install/launch management on Linux x86_64 | active | `.omo/plans/mcm-dyyl-launcher-redesign-v2.md`
- dyyl-mcm-v2 | dyyl streaming host and .mcm v2 JSON lock build/install/do/make model replace old package format | active | `.omo/plans/mcm-dyyl-launcher-redesign-v2.md`

## Open assumptions (announced defaults)
<!-- Record any default you adopt instead of asking, so the user can veto it at the gate. -->
<!-- assumption | adopted default | rationale | reversible? -->

## Findings (cited - path:lines)
- Live check: PM2 `mcm` online on `0.0.0.0:8950`; `/health` and `/api/share/list` work; `/`, `/index.html`, `/app.js`, and `/styles.css` return 404 because PM2 cwd is dyyl while server reads relative `web/...` paths.
- `src/server/mod.rs:145-149` serves `web/app.js`, `web/styles.css`, `web/index.html` via relative paths; `src/server/mod.rs:162-166` fallback reads `web/index.html` relative to cwd.
- `src/server/auth.rs:217-222` mounts `/oidc/start`, `/oidc/callback`, `/oidc/session` to mock handlers.
- `src/server/auth/mock.rs:63-159` shows mock OIDC start/callback/session issuing fake sessions.
- `src/server/config.rs:99-116` reads `MCM_OIDC_ISSUER`, `MCM_OIDC_CLIENT_ID`, `MCM_OIDC_CLIENT_SECRET`; `SecretString` redacts debug.
- Completed prior plan `.omo/plans/mcm-minecraft-manager-expansion.md:54-55` set provider base `https://auth.dyyapp.com` and callback `https://mc.dyyapp.com/api/auth/oidc/callback` with client id/secret from env/secret only.

## Decisions (with rationale)
- Do not write the user-provided OIDC secret literal to the plan; plan only names env vars and redacted verification requirements.
- Treat first user-provided OIDC value as client id and second as client secret, per explicit user confirmation.
- Include CLI and Web pkg share management, curl-bash online install repair, and concrete PCL/HMCL replacement capabilities in this plan.
- Use Linux x86_64 as first verified platform; other OS/arch fail explicitly in first implementation.

## Scope IN
- MCM server deploy/static fix, release/install routes, real OIDC auth, CLI pkg share management, Web pkg share management, real launcher core, dyyl streaming host, `.mcm` v2 JSON lock, permission model, source weighting, docs/operator handoff.

## Scope OUT (Must NOT have)
- Desktop GUI in this phase.
- HMCL/PCL code/assets/text/icon/string copying.
- OIDC secret/client secret literal in repo, plan, evidence, logs, screenshots, or test output.
- Mock OIDC as production success.
- Old `.mcm` v1 compatibility.

## Open questions
- None blocking. User approved plan generation.

## Approval gate
status: plan-written-awaiting-user-next-step
<!-- When exploration is exhausted and unknowns are answered, set status: awaiting-approval. -->
<!-- That durable record is the loop guard: on a later turn read it and resume at the gate instead of re-running exploration. -->
plan: `.omo/plans/mcm-dyyl-launcher-redesign-v2.md`
next: user chooses `$start-work .omo/plans/mcm-dyyl-launcher-redesign-v2.md` or asks for high-accuracy review first.
