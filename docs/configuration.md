# Configuration

Everything Keybund knows is in one file: **`~/.config/keybund/profiles.toml`** (or
`$XDG_CONFIG_HOME/keybund/profiles.toml`). It is the single source of truth – every generated SSH and
Git file is derived from it, and can be derived again at any time with `keybund apply`. The app and
the CLI both edit it; you can also edit it by hand.

## Example

```toml
# Keybund profiles – source of truth for all generated SSH and Git configs.

default_profile = "privat"

[settings]
projects_dir = "~/work"

[[profile]]
id = "privat"
name = "Privat"
color = "emerald"
key = { path = "~/.ssh/privat_ed25519", comment = "tristan@laptop" }
git = { name = "Tristan", email = "tristan@example.org", sign = true }
folders = ["~/dev/privat/"]
archived = false

[[profile.host]]
hostname = "github.com"
provider = "github"

[[profile.host]]
alias = "nas"
hostname = "192.168.1.20"
user = "admin"
provider = "generic"

[[profile]]
id = "acme"
name = "Acme"
color = "blue"
key = { path = "~/.ssh/acme_ed25519" }
git = { name = "", email = "tristan@acme.example", sign = false }
folders = ["~/dev/acme/"]
archived = false

[[profile.host]]
hostname = "gitlab.acme.example"
provider = "gitlab"
```

(“Profile” is the file format's name for what the app calls a *key*: a key file plus where it is
used.)

## Top level

| Field | Type | |
|---|---|---|
| `default_profile` | string, optional | ID of the default key: Git uses it in every repository outside all Git folders (`core.sshCommand` in `index.gitconfig`). Must be an active (not archived) key. |
| `[settings]` | table, optional | see [Settings](#settings) |
| `[[profile]]` | array of tables | one entry per key |

## `[[profile]]`

| Field | Type | |
|---|---|---|
| `id` | string | Stable ID, used for file names (`<id>.gitconfig`, `known_hosts.d/<id>`). Lowercase letters, digits, `-` and `_`, at most 64 characters. Derived from the name when a key is created (`Kunde C` → `kunde-c`, umlauts become `ae`, `oe`, `ue`). |
| `name` | string | Display name. Must not be empty. |
| `color` | string, optional | Colour of the dot in the app: `emerald`, `blue`, `violet`, `amber`, `rose`, `cyan`, `orange`, `zinc`. |
| `key` | table | `path` of the **private** key (`~` allowed), optional `comment`. The public key is expected at `<path>.pub`. |
| `git` | table, optional | Identity for commits in this key's folders: `name`, `email` (each may be empty: then the value from `~/.gitconfig` applies) and `sign` (sign commits with this key; needs an `email`). |
| `folders` | array of strings | Git folders. Every repository in or below one of them uses this key. Stored normalised: below `~` with `~/`, symlinks resolved, with a trailing `/`. |
| `archived` | bool | Archived keys stay in the file but generate nothing. |
| `[[profile.host]]` | array of tables | Git accounts and servers, see below. |

## `[[profile.host]]`

| Field | Type | |
|---|---|---|
| `hostname` | string | Hostname or IP: `github.com`, `gitlab.acme.example`, `192.168.1.20`. |
| `alias` | string, optional | SSH alias: `nas`, or `github-work` for a second account on the same provider. A host with an alias gets its own `Host` block. |
| `user` | string, optional | Login user. Git providers default to `git`. |
| `port` | integer, optional | SSH port, if not 22. |
| `provider` | string | `github`, `gitlab`, `bitbucket` or `generic` (a server). Decides the default user, the page for adding the key, the pinned host keys and whether a `Host` block is written without an alias. |

**How a host turns into SSH configuration:**

- A **server** (`generic`) always gets a `Host` block that matches its alias *and* its hostname:
  `Host nas 192.168.1.20`.
- A **Git account** gets a `Host` block **only with an alias** (`Host github-work`, `HostName
  github.com`). Without an alias it is reached through the key's Git folders (or the default key) –
  a `Host github.com` block would apply to every key on github.com at once.

## Settings

| Field | Default | |
|---|---|---|
| `projects_dir` | `~/dev` | Where Git folders are suggested for a key (`~/dev/<id>/`). |
| `keys_dir` | `~/.ssh` | Where new keys are created (`<keys_dir>/<id>_ed25519`). |
| `known_hosts_per_profile` | `true` | Host keys of each key's servers go to `~/.ssh/known_hosts.d/<id>` (servers only; GitHub & co. stay in `~/.ssh/known_hosts`). |
| `absolute_ssh_include` | `false` | Write the include in `~/.ssh/config` with the full path instead of `~` – for Dolphin/KIO with libssh before 0.10.2. |

Only values that differ from the default are written. Change them in the app (**Settings**) or
with `keybund config <name> <value>` (names with `-`: `projects-dir`, …). The app's language is
not stored here: it is a setting of the app itself (**Settings → Language**), kept per user in the
app's local storage.

## Validation

Values from `profiles.toml` end up in ssh and Git configuration, so Keybund refuses to save – and
the doctor reports – anything that could change their meaning:

- IDs must be unique. Every `Host` pattern – each alias, and each server's hostname – may belong to
  one key only. Several keys may use the same Git provider (`github.com`), that is the point – but
  the same host twice in one key is refused.
- Aliases and hostnames are stored in lower case: ssh matches `Host` patterns exactly as typed, and
  DNS names are case-insensitive, so `ssh nas` and `ssh NAS.local` must agree.
- Git folders must not overlap – neither within a key nor across keys (`~/dev/` and `~/dev/acme/`):
  `includeIf` would apply both and the later one would win silently.
- Folders must be absolute or start with `~/` and must not contain the glob characters `* ? [ \`
  or `"` (`gitdir:` patterns are globs), nor `.` or `..` components (Git compares real paths, so
  such a pattern would never match; the app and the CLI resolve them before saving).
- Hostnames, aliases and users must not contain spaces, wildcards (`* ? !`), quotes, `# % , [ ]`,
  control characters or a leading `-`.
- Key paths must not contain `"`, `%`, `${` or `\` (ssh expands or swallows those in
  `IdentityFile`). A path with a space or `'` is quoted in the generated files.
- No value may contain a line break or another control character.
- A key that signs commits needs its own e-mail, without spaces or `, * ? !` (it is the principal
  in `allowed_signers`, where those are pattern syntax).
- The key file must exist when a key is added or its path changes. (A key file that disappears later
  does not block other changes; the doctor reports it.)

These rules are checked on every save and again before anything is written: a hand-edited
`profiles.toml` that breaks one of them is refused by `keybund apply` and the app, and the doctor
names the value.

## Hand edits and unknown fields

`profiles.toml` is written atomically with permissions `600`. While a process changes it and the
generated files, it holds `~/.config/keybund/.lock`, so the app and the CLI (or two CLI runs) never
overwrite each other. Fields Keybund does not know – from a
newer version, or your own notes – survive a save: they are copied over at the top level, in
`[settings]` and per key (matched by `id`). Comments are not kept.

After editing the file by hand, run `keybund apply` (or open the app, which shows the pending
changes) so the generated files follow.

## Other files in `~/.config/keybund/`

| File | |
|---|---|
| `profiles.toml` | the configuration above |
| `git/index.gitconfig`, `git/<id>.gitconfig` | generated Git configuration, see [What Keybund writes](generated-files.md) |
| `allowed_signers` | generated, for verifying SSH-signed commits |
| `last-tests.json` | the last connection test per host (`<key>/<host>`), shared by app and CLI |
| `.askpass-*/` | a private helper (mode `700`) that hands a new key's passphrase to `ssh-keygen`; exists only while a key is being created |

Keybund does not use environment variables of its own. It respects `HOME`, `XDG_CONFIG_HOME` and
`SSH_ASKPASS`, and it warns when `GIT_SSH_COMMAND` or `GIT_CONFIG_GLOBAL` are set, because those
switch off the configuration it writes (see [Troubleshooting](troubleshooting.md)).
