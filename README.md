# mcm

`mcm` is an apt-like Minecraft manager CLI for Linux x86_64. It started as a mod manager and is growing toward broader Minecraft management: game versions, loaders, Java runtimes, modpacks, custom sources, a sharing service, and one-command install routes. See [Current status](#current-status) for what works today.

The project is AGPLv3 licensed (see `LICENSE`). Source availability is required for hosted services under AGPLv3 section 13.

## Overview

MCM manages Minecraft the way `apt` manages a Linux system:

- **Game instances** live under `~/mcm` by default, with configurable paths.
- **Mods** are resolved against providers (Modrinth, CurseForge, mock) and custom sources.
- **Packages** (`.mcm` files) bundle mods, shaders, resource packs, configs, and optional scripts.
- **Sources** are manually imported indexes you trust after import.
- **Sharing** happens through a Rust HTTP service with `share`, `source`, and `both` modes.
- **Install routes** at `https://mc.dyyapp.com/install` bootstrap MCM itself, and `https://mc.dyyapp.com/install/pkg/<name>` installs a named package.

Old mod-manager commands (`profile`, `search`, `install <modid>`, `list`, `status`, `remove`, `autoremove`) have moved under the `mods` (alias `mod`) command group. Old top-level spelling is not preserved.

## Current status

MCM is under active development. The table below reflects the real implementation state as of June 2026.

**Implemented:**
- Game install with canonical HMCL layout (`versions/<resolved-id>/<resolved-id>.json` + jar)
- Version resolution for vanilla, Fabric, Forge, NeoForge, and Quilt targets
- Offline authentication and launch pipeline (dry-run and real spawn)
- Java runtime discovery, managed install, and compatibility matrix
- `.mcm` v2 package schema, build, make, and do execution
- Mod provider integration (Modrinth, CurseForge, mock) with source weighting
- Game config write (`game config set` supports `java_path`, `jvm_args`, `extra_args`)
- Source weighting formula (`source_weight * max(raw_download_count, 1)`, threaded through all call sites, 5 unit tests)
- Upgrade and full-upgrade (plan-build-apply pipeline with owner-ID and dependency checks, 10 integration tests)
- Real Mojang manifest fetch for non-mock providers (`HttpGameManifestSource` hits Mojang, Fabric, Quilt, NeoForge, Forge HTTP endpoints; mock gated to `FixtureGameManifestSource` only)
- Package sharing service (publish, update, delete, list) with OIDC auth
- Source management (add, remove, list, info)
- Curl-bash install routes for bootstrap and package install
- PM2 deployment and server modes (share, source, both)
- Confirmation policy, path traversal protection, secret-field rejection
- Dyyl build and simplified text parser for package source

**Partial / In progress:**
- Dyyl host protocol (simplified text parser, not full NDJSON streaming)
- CurseForge modpack export (import works; export returns not-implemented error)

**Implemented (real, not mock):**
- Online Microsoft authentication via OAuth2 Device Code flow → Xbox Live → XSTS → Minecraft token (`mcm auth login/status/logout`). Tokens are persisted and auto-refreshed on launch.
- Real Mojang version JSON fetch from `launchermeta.mojang.com` per-version URLs (real library list, asset index reference, native classifiers, client jar SHA-1/size).
- Real client jar, library artifact, native classifier jar, and asset object downloads through the retry/resumable download engine with SHA-1 + size verification matching Mojang's published hashes.
- Native jar extraction (`.so`/`.dylib`/`.dll`) from real classifier jars into the per-version `natives/` directory using the `zip` crate.
- Real loader jar downloads (Fabric, Quilt, NeoForge, Forge) from each loader's canonical Maven repository.

MCM aims to be a strong Linux x86_64 CLI alternative to HMCL and PCL for specific workflows. It does not claim full parity with either launcher. Features like GUI mod management and Windows/macOS support are outside the current scope.

## CLI grammar

Global flags (accepted before the subcommand):

- `--config-dir <DIR>` or `MCM_CONFIG_DIR`: override config directory.
- `--state-dir <DIR>` or `MCM_STATE_DIR`: override lock state directory.
- `--provider <all|mock|modrinth|curseforge>` or `MCM_PROVIDER` (default `all`): mod provider for search, info, and install.

Top-level commands:

```bash
mcm install [target] [-y]           # low-power .mcm installer (path or URL)
mcm upgrade                         # upgrade current/default game only
mcm full-upgrade [-y]               # upgrade all configured games
mcm source <subcommand>             # manage manually imported sources
mcm pkg <subcommand>                # package download/share/install/make/info/auth
mcm game <subcommand>               # game/version/instance management
mcm build <in.dyyl> [-o out.mcm]   # compile .dyyl source to .mcm v2 lock
mcm make <out.dyyl>                 # export current instance as .dyyl source
mcm do [file] [-y]                  # execute an .mcm v2 lock file (higher-power)
mcm run [--dry-run]                 # launch the default game
mcm config                          # interactive global config editor
mcm user <subcommand>               # global user configuration
mcm auth <subcommand>               # Microsoft account login/status/logout
mcm mods <subcommand>               # mod-manager group (alias: mod)
mcm serve --mode <mode> [--bind]    # run HTTP service (share/source/both)
```

### `install [target] [-y]`

Low-power package installer. Accepts a local `.mcm` path or URL plus optional `-y`/`--yes`. Does not accept raw mod names or `mc...` smart targets. Without a target, selects the lexicographically smallest `*.mcm` file in the current directory.

```bash
mcm install ./sample.mcm --yes
mcm install https://example.test/sample.mcm --yes
mcm install                          # auto-selects the only .mcm in cwd
```

### `upgrade` and `full-upgrade [-y]`

`upgrade` upgrades the current or default game. `full-upgrade` upgrades all configured games. Both run a plan-build-apply pipeline with owner-ID and dependency satisfaction checks. Confirmation is required unless `--yes` is supplied.

### `source` subcommands

Manually manage imported sources. A fresh install has zero sources.

```bash
mcm source add https://example.test/index.json --yes
mcm source list
mcm source info https://example.test/index.json
mcm source remove https://example.test/index.json
```

`source add` requires confirmation unless `--yes` is supplied. Adding the same source twice returns a conflict message.

### `pkg` subcommands

Package flows around `.mcm` files and share URLs. `dl` is an alias for `download`.

```bash
mcm pkg info ./sample.mcm
mcm pkg install ./sample.mcm --yes
mcm pkg download ./sample.mcm --yes     # or: mcm pkg dl ./sample.mcm --yes
mcm pkg make --yes                      # create a .mcm from current game state
mcm pkg share ./sample.mcm --yes        # publish via OIDC flow
mcm pkg list                            # list local packages
mcm pkg list --server https://mc.dyyapp.com                # list public packages from server
mcm pkg list --server https://mc.dyyapp.com --mine         # list my owned packages
mcm pkg update <slug> ./updated.mcm --server https://mc.dyyapp.com --yes
mcm pkg delete <slug> --server https://mc.dyyapp.com --yes
mcm pkg download <slug> --server https://mc.dyyapp.com    # download by slug
mcm pkg install <slug> --server https://mc.dyyapp.com --yes  # install by slug
```

**Publish login flow** (via `pkg share`):

```bash
# 1. Start the publish flow. The CLI prints an OIDC auth URL.
mcm pkg share ./my-pack.mcm

# 2. Open the URL in a browser, log in. The service redirects to
#    https://mc.dyyapp.com/api/auth/oidc/callback and the CLI polls for
#    a short session result.

# 3. Once authenticated, the package is published. The CLI prints a
#    copyable one-command install snippet:
#    curl -fsSL https://mc.dyyapp.com/install/pkg/my-pack | bash
```

**Auth management** (`pkg auth`):

```bash
# Log in interactively via browser OIDC flow.
mcm pkg auth login --server https://mc.dyyapp.com

# Check whether you have an active session.
mcm pkg auth status --server https://mc.dyyapp.com

# Log out and invalidate the session.
mcm pkg auth logout --server https://mc.dyyapp.com
```

The auth session is stored locally under `<state-dir>/pkg-auth/`. Each server URL gets its own session file. Publishing, updating, and deleting packages requires an active session. The CLI never stores OIDC provider tokens, only the MCM session token.

`pkg info` is read-only and never prompts. `pkg install`, `download`, `share`, `update`, and `delete` require confirmation unless `--yes` is supplied. `pkg make` defaults to excluding secrets and personal settings or history. Explicit flags are required to export local or private data.

### `game` subcommands

Game, version, and instance management.

```bash
mcm game default                        # show default game
mcm game default dev                    # set default game
mcm game list                           # list all games, mark default with *
mcm game info dev                       # show game details
mcm game rename old-name new-name       # rename a game
mcm game config dev                     # show version-scoped config
mcm game remove dev --yes               # remove game record
mcm game install dev mc1.21.1-neoforge-21.1.172 --yes
mcm game install dev mc --dry-run       # dry-run latest vanilla MC
```

Smart targets for `game install`:

- `mc`: latest vanilla Minecraft.
- `mc1.21.1`: vanilla Minecraft 1.21.1.
- `mc-neoforge`: latest MC supporting latest compatible NeoForge.
- `mc1.21.1-neoforge`: MC 1.21.1 with latest compatible NeoForge.
- `mc1.21.1-neoforge-21.1.172`: MC 1.21.1 with NeoForge 21.1.172.
- Same grammar for `fabric`, `forge`, `quilt`. No `@latest` suffix (omission already means latest).

Top-level `mcm install mc-neoforge` is rejected. Minecraft smart targets belong only under `game install` or package contents.

**Canonical install layout:** `game install` creates a version directory under `versions/` in the game root. The resolved ID is used as the directory name. Inside, it writes `<resolved-id>.json` (the version manifest) and the game jar. Loader installs (Fabric, Forge, NeoForge, Quilt) create flat version directories like `1.21.1-neoforge-21.1.172/` alongside vanilla versions.

```
~/mcm/games/<game-name>/
  versions/
    1.21.1/
      1.21.1.json          # vanilla version manifest
      1.21.1.jar            # vanilla client jar
    1.21.1-neoforge-21.1.172/
      1.21.1-neoforge-21.1.172.json   # loader version manifest
      1.21.1-neoforge-21.1.172.jar    # loader client jar
```

**Runtime management** (`game runtime`):

```bash
mcm game runtime info dev              # show Java runtime info for a game
mcm game runtime install dev --yes     # install managed Java runtime
mcm game runtime install dev --yes --system   # install system-wide (requires root)
```

### `build <in.dyyl> [-o out.mcm]`

Compile a `.dyyl` source file into a `.mcm` v2 JSON lock. Defaults output to `<input>.mcm` if `-o` is omitted.

```bash
mcm build ./my-pack.dyyl                       # writes ./my-pack.mcm
mcm build ./my-pack.dyyl -o ./output.mcm       # custom output path
```

The `.dyyl` source is parsed and transformed into the v2 lock format with install steps, permissions, and artifact references. The resulting `.mcm` file can be installed with `mcm install`, published with `mcm pkg share`, or executed with `mcm do`.

### `make <out.dyyl>`

Export the current game instance state as a `.dyyl` source file.

```bash
mcm make ./my-instance.dyyl
```

This reads the active profile's lock state and writes a `.dyyl` source that can be built back into a `.mcm` lock with `mcm build`.

### `do [file] [-y]`

Higher-power executor for `.mcm` v2 lock files. Without an argument, uses the single `*.mcm` in the current directory (errors if zero or multiple). Executes the full step graph declared in the lock, including install steps and declared actions.

```bash
mcm do ./sample.mcm --yes
mcm do                                     # auto-selects the only .mcm in cwd
```

`.dyyl` source files cannot be executed directly. Build them first with `mcm build`:

```bash
mcm build ./my-pack.dyyl -o ./my-pack.mcm
mcm do ./my-pack.mcm --yes
```

### `run [--dry-run]`

Launch the default game instance. `--dry-run` prints the launch command without executing it.

```bash
mcm run                                    # launch the default game
mcm run --dry-run                          # print the launch command only
```

The launcher resolves Java, classpath, assets, and natives from the game instance, then executes the Minecraft client with the correct arguments. Authentication supports offline mode and online Microsoft authentication. Online mode uses the real OAuth2 Device Code flow (`mcm auth login`) and auto-refreshes expired access tokens using the stored refresh token before launch; the refreshed account is persisted back to `config.toml`.

### `auth` subcommands

Microsoft account management. `login` runs the OAuth2 Device Code flow (opens a browser at microsoft.com/link), exchanges the resulting token through Xbox Live → XSTS → Minecraft, and persists the account (username, UUID, access token, refresh token, expiry) to `config.toml` under `[launch_auth.online]` with `mode = "online"`.

```bash
mcm auth login      # start device-code login flow
mcm auth status     # show current account + session validity + refresh token availability
mcm auth logout     # reset to offline mode and clear stored account
```

The well-known public Microsoft client ID `00000000402b5348` is used (no client secret required for the device-code flow). To switch back to offline mode without removing the account, set `mode = "offline"` in `config.toml`.

### `user` subcommands

Global user configuration. Set per-key values that apply across profiles.

```bash
mcm user config source.weight.modrinth 2.0    # set source weight
```

### `config`

Interactive global config editor. Non-interactive subcommands may be added later.

### `mods` (alias: `mod`) subcommands

The mod-manager command group. Old top-level mod commands live here.

```bash
mcm mods add dev --mods-dir ./minecraft/mods --mc-version 1.20.1 --loader fabric --side client
mcm mods use dev
mcm mods profile-list
mcm mods show                          # show active profile
mcm mods show dev                      # show named profile

mcm --provider mock mods search root
mcm --provider mock mods info rootmod
mcm mods info ./some-local.jar

mcm --provider mock mods install rootmod --dry-run
mcm --provider mock mods install rootmod --yes
mcm --provider mock mods install --file mods.txt --yes

mcm mods list
mcm mods status
mcm mods remove rootmod --yes
mcm mods uninstall rootmod --yes       # alias for remove
mcm mods autoremove --yes
```

A mod list file contains one mod ID or query per non-empty line. `#` starts a comment.

```text
# mods.txt
rootmod
standalone
```

The resolver selects the latest stable compatible artifact by default. Required dependencies are installed automatically. Optional, incompatible, embedded, and unknown dependencies are surfaced as warnings and are not installed by default.

`list` prints installed logical mods and exact artifact identities. `status` reports owned jars that are missing or changed and shows untracked jars, but it never claims or deletes untracked jars.

`remove` and `uninstall` remove manual roots and their owned jar only. Auto-installed dependencies remain until `autoremove`, which removes auto packages that are no longer reachable through required dependency edges from any remaining manual root.

Use `--config-dir` / `MCM_CONFIG_DIR` and `--state-dir` / `MCM_STATE_DIR` to isolate configuration and lock state, which is useful for tests and disposable profiles.

The mock provider includes deterministic data for a root mod, a required dependency, optional, incompatible, embedded, and unknown dependency warnings, duplicate source candidates for the same logical mod ID, beta and alpha artifacts excluded by default, and a missing-download error case.

## `.mcm` package schema

A `.mcm` file is JSON, schema-versioned (currently version 2), size-limited (10 MB), and depth-limited (64). It can contain:

- Identity: package name (normalized to `[a-z0-9-]`, 1 to 64 chars, alphanumeric start and end, no consecutive hyphens, no reserved names), version, description.
- Game version and loader.
- Dependencies: required, optional, incompatible, embedded, unknown.
- Mods, shaderpacks, resourcepacks, datapacks.
- Saves, NBT, and structure files.
- Configs and version-scoped configs.
- Optional actions (Linux shell scripts only in the first implementation).
- Optional launch request.
- Install steps, permissions, and artifact references (v2 lock format).
- Explicit local or private settings and history (excluded from public sharing by default).

**Secret-field rejection:** the parser recursively scans all JSON keys (case-insensitive) and rejects fields named `token`, `secret`, `password`, `credential`, or `api_key`. This runs before typed parsing, so secrets hidden inside opaque containers are caught.

**Path traversal protection:** asset paths are validated to reject empty, null, `..`, absolute paths, backslashes, and Windows-reserved name components.

**Package import and export** supports the native `.mcm` format plus import from Modrinth `.mrpack` and CurseForge manifests.

## Custom sources

Sources are manually imported indexes. A fresh install has zero custom sources. No source, including the author source, is preloaded by default.

```bash
mcm source add https://example.test/index.json --yes
mcm source list
mcm source info https://example.test/index.json
mcm source remove https://example.test/index.json
```

**Trust model:** a source is trusted once you manually import it. Schema and hash validation still run to catch corruption or bugs. A source can declare capabilities such as `mods`, `packages`, `games`, `loaders`, and `java`. The client uses a source only according to its declared capabilities.

Install, download, delete, and other state-changing actions from a source still require confirmation, even though the source itself is trusted.

## Confirmation policy

MCM centralizes confirmation through a single policy:

- **Read-only actions never prompt.** This includes `list`, `info`, `status`, `search`, `--dry-run`, and `help`.
- **Bypassable actions** (install, download, delete, package install, runtime install, source actions, script execution, launch-on-install, game remove, `autoremove`) require a second confirmation by default. Pass `-y`/`--yes` to bypass these in non-interactive use.
- **Non-bypassable actions** (`RootSystemChange`) always require typed "yes" confirmation, even with `--yes`. In non-TTY mode, they print the exact `sudo`/`pkexec` command instead of failing generically.

**`autoremove` is MC-critical.** It prints a strong warning that removing apparently unused mods or resources may break worlds, saves, or modded structures, then requires second confirmation. With `--yes`, the warning is emitted to stderr and the operation proceeds.

**Package scripts:** if a `.mcm` package declares shell scripts or actions, MCM shows a strong warning unless `--yes` is supplied. Scripts run with the working directory set to the game version or instance root, not the user's current shell directory. If a script needs root, the script itself may invoke `sudo`. MCM does not wrap script execution in automatic sudo.

**Web install exception:** the `curl | bash` package install route at `https://mc.dyyapp.com/install/pkg/<name>` intentionally runs with `--yes` or non-interactive semantics, so package install and declared launch can proceed without prompts.

## Server modes

MCM includes a Rust HTTP service that can run in three modes:

- **`share` mode:** public download of `.mcm` packages plus authenticated publish, update, and delete. Download is public. Upload requires OIDC login.
- **`source` mode:** serve a manually imported source index and metadata or artifact blobs. Any computer can run it.
- **`both` mode:** enable share and source routes in one process.

The default bind address is `127.0.0.1:8950`. The service does not bind `0.0.0.0` by default.

```bash
# Local development (share + source):
mcm serve --mode both

# Production (share only, publicly reachable):
mcm serve --mode share --bind 0.0.0.0:8950
```

The `serve` subcommand runs as a blocking foreground process suitable for PM2 or systemd.

### PM2 deployment

The service runs behind PM2. An `ecosystem.config.js` is provided in the repository root. **Never commit secret values** — use placeholders or a secret file.

#### Start

```bash
pm2 start ecosystem.config.js
# or if the process already exists and was saved:
pm2 resurrect
```

#### Restart (after binary rebuild)

```bash
# Rebuild the release binary, then restart the PM2 process:
cargo build --release
pm2 restart mcm
```

#### Rollback (revert to previous binary)

```bash
# Stop the current process:
pm2 stop mcm

# Restore the previous binary (e.g., from git stash, backup, or previous release):
# Then restart:
pm2 start ecosystem.config.js
# or:
pm2 restart mcm
```

#### View logs

```bash
pm2 logs mcm --lines 100
pm2 logs mcm --err --lines 50   # errors only
```

#### Inspect process

```bash
pm2 show mcm          # full description (args, env keys, cwd, uptime)
pm2 env 20            # full environment (process id 20)
```

#### Process description (redacted example)

```
script path  : /path/to/mcm/target/release/mcm
script args  : serve --mode share --bind 0.0.0.0:8950
exec cwd     : /path/to/project/dyyl
env keys     : MCM_SHARE_DATA_DIR, (MCM_OIDC_* if configured)
```

The service binds `0.0.0.0:8950` in production behind a reverse proxy (nginx, Caddy) that terminates TLS. For local-only binding use `--bind 127.0.0.1:8950`.

## OIDC authentication

Package publish, update, and delete use OIDC login. The CLI prints an auth URL, the user logs in through a browser, the service redirects to `https://mc.dyyapp.com/api/auth/oidc/callback`, and the CLI receives a short session to perform publish, update, or delete.

OIDC configuration uses environment variable names only. No secret values are committed to the repository or documented here:

- `MCM_OIDC_ISSUER`: OIDC provider base URL (for example `https://auth.dyyapp.com`).
- `MCM_OIDC_CLIENT_ID`: OIDC client ID.
- `MCM_OIDC_CLIENT_SECRET`: OIDC client secret. Provide through environment or a secret file. Never commit this value.
- `MCM_OIDC_REDIRECT_URL`: callback URL for the OIDC flow (for example `https://mc.dyyapp.com/api/auth/oidc/callback`).

**No admin token or Turnstile is required for publish/update/delete.** Authentication is OIDC only.

### Where to inject secrets

Secret values (OIDC client secret, API keys) must never be committed to the repository. There are three supported injection methods:

**1. PM2 environment variables (recommended for production):**

Set secrets in the shell before starting PM2, or use PM2's `--env` flag:

```bash
# Export secrets in the terminal, then start PM2:
export MCM_OIDC_CLIENT_ID="your-client-id"
export MCM_OIDC_CLIENT_SECRET="your-client-secret"
pm2 start ecosystem.config.js
```

**2. Secret files (for Docker, systemd, or CI):**

Write secrets to a file outside the repository with restrictive permissions:

```bash
echo -n "your-client-secret" > /etc/mcm/oidc-client-secret
chmod 600 /etc/mcm/oidc-client-secret
```

Then load the file in your deployment wrapper:

```bash
# systemd EnvironmentFile or wrapper script:
MCM_OIDC_CLIENT_SECRET=$(cat /etc/mcm/oidc-client-secret)
```

**3. Ecosystem config placeholders (never real values):**

The `ecosystem.config.js` file contains commented placeholders. Uncomment and fill in values locally, but never commit real secrets:

```javascript
env: {
  MCM_SHARE_DATA_DIR: "/home/usr/.mcm/share",
  // MCM_OIDC_ISSUER: "https://auth.dyyapp.com",
  // MCM_OIDC_CLIENT_ID: "<your-client-id>",
  // MCM_OIDC_CLIENT_SECRET: provide via env or secret file, never commit.
  // MCM_OIDC_REDIRECT_URL: "https://mc.dyyapp.com/api/auth/oidc/callback",
},
```

The service reads these from the process environment at startup. For local development without OIDC, set `MCM_AUTH_MODE=mock` to use the built-in mock auth provider.

### Auth mode

The server supports two auth modes:

- **`mock`** (default when OIDC env vars are absent): deterministic test user, no browser login required. Suitable for local development and testing only.
- **`real`** (when `MCM_OIDC_ISSUER`, `MCM_OIDC_CLIENT_ID`, and `MCM_OIDC_CLIENT_SECRET` are all set): full OIDC browser login flow.

The server fails clearly at startup if real auth is requested but OIDC env vars are missing.

## Data directory

Server package and blob storage defaults outside `/x`. The default data directory is `/var/lib/mcm-share` or a user-specified path via `MCM_SHARE_DATA_DIR`. The service refuses to start if the default data directory is under `/x`.

Local client storage uses the normal MCM user data paths under `~/mcm`.

## One-command install routes

Two `curl | bash` routes are available:

**Bootstrap MCM itself:**

```bash
curl -fsSL https://mc.dyyapp.com/install | bash
```

This downloads and installs the MCM binary for Linux x86_64, verifying checksums or pinned hashes before installing. Other OS or arch combinations are detected and exit with an explicit unsupported-platform message in the first implementation.

**Install a named package permanently:**

```bash
curl -fsSL https://mc.dyyapp.com/install/pkg/<package-name> | bash
```

This ensures MCM is installed, then delegates to the low-power `mcm install <downloaded-or-url .mcm> --yes` flow. Package names are validated and safely quoted. Missing packages return 404. The web install script intentionally runs in yes or non-interactive mode so package install and declared launch can proceed without prompts.

## Publish policy

Authenticated users can publish, update, and delete packages through the CLI. The policy is:

- **One publish or update push per day per user.** Both new publish and update count as the daily push.
- **Max 5 existing packages per user** at the same time.
- **Delete does not count as a push** but also does not reset the daily push limit.
- **2-day slug reservation:** after delete, the slug is reserved for the deleting owner for 2 days, then released. Another user cannot claim it during reservation.
- **Overwrite on update:** updates overwrite the current package. Old package backups are not retained on the server.
- **Owner check on upgrade:** local installs record the package author's user ID. Upgrade refuses and warns if the remote package slug now belongs to a different user ID.
- **Globally unique, case-insensitive slugs.** A duplicate slug returns 409.

## License

MCM is licensed under the GNU Affero General Public License v3 or later (see `LICENSE`). Under AGPLv3 section 13, anyone running a modified version as a network service must offer users the Corresponding Source through a standard means at no charge.

Dependency licenses are audited through `cargo deny check licenses` (see `deny.toml`). Only permissive OSI-approved licenses are allowed for dependencies, avoiding copyleft compatibility questions.

**HMCL and PCL are conceptual UX and product references only.** No HMCL or PCL code, UI text, assets, icons, strings, or implementation structure is copied. Direct HMCL code reuse is forbidden unless a separate explicit license review accepts GPLv3 plus extra-term obligations. PCL and PCL2 code and assets are no-copy due to their custom restricted license.

## Providers

`--provider all` is the default and queries Modrinth plus CurseForge together, merging candidates by logical mod ID. If `CURSEFORGE_API_KEY` is not set, CurseForge is skipped with a warning while Modrinth remains usable. `--provider mock` is deterministic and requires no network. `--provider modrinth` uses the public Modrinth v2 API and works without credentials. `--provider curseforge` uses the CurseForge v1 API and requires `CURSEFORGE_API_KEY` in the environment.

Search and cloud info use the active profile's Minecraft version, loader, and side. Results are grouped by logical mod ID, so the same mod ID from Modrinth, CurseForge, or another source is shown as provider or source candidates rather than distinct packages.

Local jar info reads `fabric.mod.json`, `META-INF/mods.toml`, and legacy `mcmod.info` when present. If metadata is unavailable, it prints basic file information and a SHA-256 hash without inventing provider identity.
