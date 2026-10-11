# Modules

A map of the code: what each part does and how the parts fit together. For the big picture see
[Architecture](architecture.md).

```text
crates/core/        keybund-core – all logic, no UI
crates/cli/         keybund – the command-line tool
app/src-tauri/      keybund-app – the Tauri backend of the desktop app
app/src/            the React frontend
packaging/          desktop entry, Fedora toolbox setup
scripts/            screenshots for README and documentation
docs/               this documentation (MkDocs Material)
```

## `keybund-core` (`crates/core`)

The library everything else uses. Every public function takes an `Env`, so it can work on a
temporary home directory in tests.

### Data and storage

| Module | Responsibility |
|---|---|
| `model` | The data model: `Config` (all keys + settings), `Profile` (a key and where it is used), `KeyRef`, `GitIdentity`, `HostEntry`, `Provider`, `Settings`. `HostEntry::ssh_patterns` decides which `Host` patterns a host gets. |
| `env` | `Env`: the home directory, `~/.config/keybund` (respecting `XDG_CONFIG_HOME`), the login shell and every path Keybund uses (`ssh_config()`, `index_gitconfig()`, `profile_gitconfig(id)`, …); `expand`/`contract` for `~`. |
| `store` | Loading, validating and saving `profiles.toml`: the TOML layout (`[[profile]]`, `[[profile.host]]`), all [validation rules](configuration.md#validation), keeping unknown fields, `slugify` for IDs, normalising folders (symlinks, trailing `/`). |
| `error` | One `Error` enum with German messages and a stable `kind()` per variant for the app. |
| `history` | The last connection test per key in `last-tests.json`, shared by app and CLI. |

### Generating and writing configuration

| Module | Responsibility |
|---|---|
| `render` | Pure functions from `Config` to file contents: `ssh_config` (`keybund.conf`), `index_gitconfig`, `profile_gitconfig`, `allowed_signers`, the include line; `git_value` quoting; snapshot-tested. |
| `apply` | `plan` (render, compare with disk, unified diffs), `apply` (re-check, backups, atomic writes, deletions), placing the includes: `ensure_ssh_include` (first directive) and `ensure_git_include` (last entry). |
| `uninstall` | The way back: removes the includes (`without_ssh_include`, `without_git_include`), generated files and optionally `profiles.toml`, or restores the backups; previewed as a `Plan`. |
| `fsutil` *(private)* | Atomic writes that keep permissions and write through symlinks, backups, directory creation with modes. |
| `process` *(private)* | Running external programs with clear errors (`tool_missing`, `tool_failed`), shell quoting. |

### Keys and hosts

| Module | Responsibility |
|---|---|
| `keygen` | Creating Ed25519 keys with `ssh-keygen` (passphrase through a private askpass helper), reading public keys and fingerprints, key age, scanning `~/.ssh` for key pairs. Never reads a private key. |
| `keyinstall` | Adding a public key to a server's `authorized_keys` like `ssh-copy-id`: a small idempotent remote script, login through whatever already works, `SSH_ASKPASS` for the app. |
| `hostkeys` | Host key handling: the pinned keys of GitHub, GitLab.com and Bitbucket, `ssh-keyscan` and fingerprints, the known_hosts file per key, `check` before a connection and `trust` after confirmation. |
| `provider` | Provider specifics: display name, default hostname, `git` user, detection from a hostname, the URL to add a key and to manage keys. |
| `probe` | The connection test: `ssh -T -v` with only the key and everything else switched off, interpreting the output into a result (“Verbunden als @user”, “GitHub kennt diesen Key nicht”, …), the step-by-step explanation and the check *which* key the server accepted. |

### Insight and tools

| Module | Responsibility |
|---|---|
| `which` | “Which key is used here?”: folders through `git config --show-origin` (including worktrees via `git rev-parse --absolute-git-dir`), hosts through `ssh -G`, plus agent status and notes. |
| `doctor` | All [health checks](troubleshooting.md#doctor-checks) and their fixes (`Fix::Permissions`, `MoveSshInclude`, `Apply`, `CleanupSshConfig`, `AbsoluteSshInclude`), including the libssh version check for Dolphin. |
| `gitenv` | Finds `GIT_SSH_COMMAND` and `GIT_CONFIG_GLOBAL` in the process, the login shell and shell config files; reads the global Git identity. |
| `import` | Parsing an existing `~/.ssh/config` into `Host` blocks, proposals for keys, importing them, and the cleanup that removes migrated blocks (keeping blocks with extra directives). |
| `clone` | Parsing and checking repository URLs, finding the keys for a host, the target folder, `git -c core.sshCommand=… clone --`. |
| `archive` | Moving a key pair and its known_hosts file to `~/.ssh/archive/` and marking the key archived – validated before anything moves. |
| `desktop` | Opening a URL from the CLI, an AppImage or a toolbox: a cleaned environment, `flatpak-spawn --host`, `xdg-open`, `gio open`. |

## `keybund` CLI (`crates/cli`)

| File | Responsibility |
|---|---|
| `main.rs` | The command line definition with `clap` (commands, options, help texts) and dispatch. Errors end with exit code 2. |
| `commands.rs` | One function per command: asks the missing questions (`dialoguer`), calls the core, prints results and diffs, decides the exit code. Refuses to ask without a terminal. |
| `ui.rs` | Consistent output: headings, rows, success/warning/error lines, hints. |
| `system.rs` | Clipboard (`wl-copy`, `xclip`, `xsel`) and browser access for the CLI. |

## Tauri backend (`app/src-tauri`)

| File | Responsibility |
|---|---|
| `lib.rs` | Builds the app: plugins (clipboard, dialog), app state, the command registry (`keybund_commands!`) and the bindings export. |
| `commands.rs` | The `#[tauri::command]` functions – thin wrappers that take the `Env` from `AppState` and run the work with `spawn_blocking`. |
| `api.rs` | The app-specific logic, testable without Tauri: view models (`ProfilesView`, `ProfileView` with key info, links and last test), `save_and_apply`, the global write lock `writing()`, creating a key with its first use, imports, generated config per key, settings. |
| `main.rs` | Entry point. |
| `tests/ipc.rs` | Calls commands through Tauri's IPC layer (mock runtime) with JSON, as the frontend does. |
| `tests/bindings.rs` | Fails when `app/src/lib/bindings.ts` is out of date; `UPDATE_BINDINGS=1` rewrites it. |

## Frontend (`app/src`)

| Path | Responsibility |
|---|---|
| `main.tsx` | Starts React; outside Tauri it installs the mock backend first. |
| `app.tsx`, `router.tsx` | Query client, toaster, router with hash history. Routes: `/`, `/profile/$id`, `/new`, `/which`, `/clone`, `/doctor`, `/import`, `/settings`, `/apply`, `/uninstall`. |
| `routes/home.tsx` | Overview: to-do list, “how it works”, key cards. |
| `routes/profile.tsx` | The key page: uses with their forms, tests and “add key” flows, key card, identity, signing, archiving, generated config. |
| `routes/new-key.tsx` | The new-key form. |
| `routes/which.tsx`, `clone.tsx`, `doctor.tsx`, `import.tsx`, `settings.tsx`, `apply.tsx`, `uninstall.tsx` | The tool pages. |
| `components/` | Sidebar, app shell, status line, doctor panel, diff view, connection test details, host key dialog, “how it works”; `components/ui/` holds the shadcn/ui primitives (button, card, dialog, tabs …). |
| `lib/bindings.ts` | Generated by tauri-specta: every command and type. Do not edit. |
| `lib/api.ts` | Unwraps command results and throws `ApiError` with the backend's `kind`. |
| `lib/queries.ts` | TanStack Query hooks for every read and mutation; mutations run in one scope and invalidate all queries afterwards. |
| `lib/host-keys.ts` | Asks for host key confirmation before a test or key installation. |
| `lib/categories.ts`, `lib/utils.ts` | Presentation helpers: what a key is used for, labels, colours, dates, plurals, slug. |
| `lib/open.ts`, `lib/tauri.ts` | Opening links (with a copyable fallback) and the folder picker. |
| `lib/mock/` | `backend.ts`: the in-memory backend implementing every command for browser preview and tests; `install.ts` routes Tauri's IPC to it. |
| `test/` | Vitest setup and a render helper with router and query client. |

## How a click travels

Adding a Git folder on a key page, end to end:

1. `routes/profile.tsx` calls `useUpdateProfile(id)` from `lib/queries.ts` with an updater
   `(current) => ({ ...current, folders: [...current.folders, "~/dev/acme/"] })`.
2. The mutation fetches the current profiles, applies the updater and calls `api.updateProfile`,
   which goes through `lib/bindings.ts` to the Tauri command `update_profile`.
3. `commands.rs` runs `api::update_profile` on a blocking thread. It takes the write lock, loads
   `profiles.toml`, normalises the folder, replaces the profile and calls `save_and_apply`.
4. `store::save` validates (nested folders? glob characters?) and writes `profiles.toml`;
   `apply::apply_config` renders, plans and writes `index.gitconfig` and `acme.gitconfig`.
5. The frontend invalidates all queries; the key page, the sidebar and the status line show the new
   state, and a toast says “Gespeichert”. A validation error comes back with its `kind` and message
   and is shown to the user.
