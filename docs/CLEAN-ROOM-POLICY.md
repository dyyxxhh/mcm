# Clean-Room Policy for MCM

**This document is not legal advice. Consult a qualified attorney for copyright or licensing questions.**

## Purpose

MCM is a standalone Minecraft mod manager. Its design is informed by the
general concepts of Minecraft launcher/mod manager UX, but MCM must not
incorporate code, assets, strings, icons, or implementation structure from
existing launcher projects.

This policy documents the boundaries for two reference projects frequently
discussed in the Minecraft community: **HMCL** (Hello Minecraft! Launcher)
and **PCL/PCL2** (Plain Craft Launcher).

---

## HMCL (Hello Minecraft! Launcher)

| Property | Value |
|---|---|
| License | GPL-3.0 with additional terms (see below) |
| Repository | https://github.com/HMCL-dev/HMCL |
| Language | Java |

HMCL is licensed under GPLv3 with additional terms (the "HMCL AUTHORS"
addition to Section 7 of the GPLv3). This means:

- **Direct code reuse** (copying any HMCL source file, function, or snippet)
  requires a separate license review and is **FORBIDDEN** in this project
  unless explicitly authorized in writing by the HMCL authors.
- MCM is licensed under **AGPL-3.0-or-later** — even though AGPLv3 and GPLv3
  share a compatibility mechanism (AGPLv3 §13 allows linking/combining with
  GPLv3 works), the additional terms in HMCL's GPLv3+extra make any direct
  code incorporation a legal risk without explicit permission.
- **Conceptual reference is allowed.** You may read HMCL's UI/UX patterns
  to understand what a launcher looks like, but you must not reproduce its
  implementation.

## PCL / PCL2 (Plain Craft Launcher)

| Property | Value |
|---|---|
| License | Custom restricted license (see below) |
| Repository | https://github.com/Hex-Dragon/PCL2 (private) |
| Language | C# (PowerBuilder / .NET) |

PCL and PCL2 use a **custom restricted license** that explicitly forbids:

- Redistribution of the source or binary
- Copying of code, assets, strings, icons, or any other component
- Creating derivative works

**Therefore:**

- **NO code, assets, strings, icons, UI layout files, or implementation
  structure** from PCL/PCL2 may be copied into MCM under any circumstances.
- PCL/PCL2 may be used as a **conceptual UX reference only** — e.g.,
  "PCL has a mod management screen with search and filter; MCM could benefit
  from similar functionality." This is an idea, not an implementation.
- Contributors must not paste PCL/PCL2 source code, configuration files,
  or screenshots containing protected content into issues, pull requests,
  or commit messages.

---

## Contributor Rules

### DO ✅

- Use PCL/HMCL as **conceptual inspiration** ("this is a feature users expect").
- Design your own implementation based on MCM's own architecture and
  the Minecraft launcher protocol documentation.
- Ask a maintainer if you are unsure about a reference or pattern.
- Link to HMCL/PCL in discussions as a **reference for UX concepts**
  (e.g., "HMCL supports profile switching, MCM should too").

### DO NOT ❌

- **Copy-paste any HMCL/PCL source code** into MCM files.
- **Port HMCL/PCL implementation logic** into MCM's codebase.
- **Use HMCL/PCL assets** (icons, images, strings, translation files).
- **Replicate HMCL/PCL internal data structures or algorithms** from their
  source code.
- **Paste HMCL/PCL source code excerpts** into GitHub issues, PRs, or commits.
- **Claim MCM is "based on" or "derived from" HMCL or PCL** — it is not.

### If in doubt

Ask before looking at HMCL/PCL source code. A maintainer can answer
whether a reference is safe without you needing to examine the source
directly.

---

## Summary

| Project | License | Can copy code? | Can copy assets? | Conceptual reference? |
|---|---|---|---|---|
| HMCL | GPL-3.0 + extra terms | ❌ (requires permission) | ❌ | ✅ |
| PCL/PCL2 | Custom restricted | ❌ | ❌ | ✅ |

Any violation of this policy should be reported to the maintainers immediately.
Committed infringing content will be removed and the commit history rewritten.
