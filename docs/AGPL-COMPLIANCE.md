# AGPL-3.0 Compliance for MCM

**This document is not legal advice. Consult a qualified attorney for license compliance questions.**

## License

MCM is licensed under the **GNU Affero General Public License v3.0 or later** (AGPL-3.0-or-later).
The full license text is in [`LICENSE`](../LICENSE) at the project root.

## What AGPLv3 Means

The AGPLv3 is a copyleft license published by the Free Software Foundation.
It is based on the GPLv3 but adds **Section 13: Remote Network Interaction**,
which closes the "application service provider loophole" in the GPLv3.

### Section 13 — Network Server Obligation

> "if you modify the Program, your modified version must prominently offer
> all users interacting with it remotely through a computer network
> (if your version supports such interaction) an opportunity to receive
> the Corresponding Source of your version by providing access to the
> Corresponding Source from a network server at no charge."

— AGPLv3, Section 13

## How This Applies to MCM

### Use Case 1: CLI Tool (Personal/Desktop Use)

Running `mcm` locally to manage Minecraft mods on your own machine does not
trigger the network interaction obligation. You are simply executing the
program — no source distribution is required.

### Use Case 2: Share/Source Server (Network Service)

If MCM is used as or integrated into a **network-accessible service** (for
example, a mod sharing server, a modpack distribution endpoint, or a
Minecraft server wrapper that others interact with over a network), you
**must**:

1. Make the **Corresponding Source** of the running version available to all
   users who interact with it over the network.
2. Provide this source **at no charge** through a network server (e.g.,
   a public Git repository).
3. Include **any modifications** you made to MCM in the Corresponding Source.

The "Corresponding Source" includes:
- All source code for MCM (this repository)
- Scripts used to control compilation and installation
- Any shared libraries the program is specifically designed to require
- Any modifications or patches you applied

### Use Case 3: Modified Distributions

If you distribute modified binary copies of MCM (e.g., via a package manager,
direct download, or physical media), standard GPLv3 distribution terms apply:
you must provide the Corresponding Source alongside the binary or with a
written offer valid for at least three years.

## Practical Compliance Steps

1. **Keep this repository public.** Publishing the source on GitHub (or a
   similar platform) satisfies the source availability requirement for both
   distribution and network interaction use cases.
2. **If you modify MCM**, push your changes to a public fork or publish
   patches alongside the binary/service.
3. **If you incorporate MCM into a larger project**, ensure the combined
   work is licensed under AGPLv3-compatible terms.
4. **Include a copy of this notice** in any user-facing interface if MCM
   is modified and used as a network service.

## Dependency Licenses

MCM's dependencies (Rust crates listed in `Cargo.toml`) are predominantly
permissive (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-DFS-2016). These are
compatible with AGPLv3 distribution. The [`deny.toml`](../deny.toml)
configuration audits dependency licenses on every CI run via `cargo-deny`.

## Resources

- [GNU AGPLv3 FAQ](https://www.gnu.org/licenses/gpl-faq.html)
- [AGPLv3 Official Text](https://www.gnu.org/licenses/agpl-3.0.html)
- [Choose a License: AGPL-3.0](https://choosealicense.com/licenses/agpl-3.0/)
- [SPDX License List: AGPL-3.0-or-later](https://spdx.org/licenses/AGPL-3.0-or-later.html)
- [GNU GPLv3 How-to](https://www.gnu.org/licenses/gpl-howto.html)
