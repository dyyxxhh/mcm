---
slug: hmcl-parity-correction-backlog
status: drafting
approved_scope: user confirmed the new plan must include many unmet items from both previous plans, not only the game version installation format.
plan_path: .omo/plans/hmcl-parity-correction-backlog.md

## Findings integrated

- Prior plans are `.omo/plans/mcm-minecraft-manager-expansion.md` and `.omo/plans/mcm-dyyl-launcher-redesign-v2.md`.
- User explicitly says many items in both plans were not achieved; the correction plan must write those in.
- Plan 1 launcher-related promises include Mojang version manifests, loader install model, Java/runtime compatibility, launch builder, Microsoft/Mojang auth testability, retry/resume downloads, package/source/share/OIDC/curl-bash flows, and final real QA.
- Plan 2 stricter promises include real Minecraft instances, Vanilla/Fabric/Forge/NeoForge/Quilt installs, version manifest fetch, assets/libraries/natives/classpath, offline/online auth, real run, OIDC/share/Web/curl-bash/dyyl/.mcm v2/source weighting, and no deferral for Linux x86_64.
- Current evidence shows game install remains mock-only for manifest/client/loader artifacts and uses a noncanonical nested loader layout.

## Decisions

- The correction plan is XL/high risk and begins with a compliance matrix instead of assuming the current scope.
- MCM keeps configured root (`~/mcm` default) while internal instance layout becomes Minecraft/HMCL-compatible.
- Fixture mode is allowed for deterministic tests, but production paths must not silently use mocks.
- Clean-room HMCL by default; HMCL code copying only with provenance/NOTICE; PCL remains no-copy.

## Status

- Formal plan scaffolded and filled: `.omo/plans/hmcl-parity-correction-backlog.md`.
- Awaiting Metis gap review result; incorporate if it finds missing constraints.
intent: clear
pending-action: write .omo/plans/hmcl-parity-correction-backlog.md
approach: <fill: the approach you intend to plan>
---

# Draft: hmcl-parity-correction-backlog

## Components (topology ledger)
<!-- Lock the SHAPE before depth. One row per top-level component that can succeed or fail independently. -->
<!-- id | outcome (one line) | status: active|deferred | evidence path -->

## Open assumptions (announced defaults)
<!-- Record any default you adopt instead of asking, so the user can veto it at the gate. -->
<!-- assumption | adopted default | rationale | reversible? -->

## Findings (cited - path:lines)

## Decisions (with rationale)

## Scope IN

## Scope OUT (Must NOT have)

## Open questions

## Approval gate
status: drafting
<!-- When exploration is exhausted and unknowns are answered, set status: awaiting-approval. -->
<!-- That durable record is the loop guard: on a later turn read it and resume at the gate instead of re-running exploration. -->
