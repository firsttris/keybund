# Command line

`keybund` does everything the app does, from the terminal. It works on the same
`~/.config/keybund/profiles.toml`, so you can switch between app and CLI at any time. Its messages
are in German for now (the app speaks English and German); `keybund --help` and
`keybund <command> --help` list every option.

```text
keybund list [--all]                       keys with fingerprint, Git folders and hosts
keybund show <KEY>                         one key in detail, with its public key
keybund new [options]                      create a key (asks in the terminal, or all via options)
keybund edit <KEY> [options]               change name, folders, hosts, identity, signing, key file
keybund copy <KEY> [--print]               public key to the clipboard
keybund install-key <KEY> [--host H]       add the key: on servers like ssh-copy-id, at GitHub/GitLab/Bitbucket copy & open page
keybund trust <KEY> [--host H] [--yes]     check and confirm the host keys of a key's servers
keybund test <KEY> [--host H]              connection test
keybund which [PATH|HOST]                  which key is used here – and why?
keybund clone <URL> [--key KEY] [--dest D] clone into the Git folder of the matching key
keybund apply [--dry-run] [--yes]          show the diff of the generated files and write them
keybund doctor [--fix]                     run all checks, optionally fix what can be fixed
keybund keys                               key pairs in ~/.ssh
keybund import [--all] [--yes] [--keep-old]   take over an existing ~/.ssh/config
keybund default <KEY> | --unset            set or remove the default key
keybund config [NAME] [VALUE]              show or change settings
keybund archive <KEY> [--yes]              move a key to ~/.ssh/archive, remove its configuration
keybund uninstall [--restore-backups] [--purge] [--yes]   remove everything Keybund wrote
```

`<KEY>` is a key's ID (`client-c`), shown by `keybund list`. Hosts (`--host`) are given by alias or
hostname/IP.

## Exit codes and scripting

| Code | Meaning |
|---|---|
| `0` | success; for `test`, `install-key` and `trust`: every host worked; for `doctor`: no problems |
| `1` | `test`, `install-key`, `trust`: at least one host failed; `doctor`: at least one problem |
| `2` | error (invalid input, missing file, a refused write …), with a message on stderr |

Commands that write show what will change and ask first. `--yes` (`-y`) skips the question. Without
a terminal – in a script, a cron job, CI – a question cannot be asked: such a command then needs
`--yes`, and `new` needs all of its answers as options. Without `--yes` the key and the profile are
still created, only your own files stay untouched; `keybund apply --yes` finishes the job.

## Keys

### `keybund list`

Lists the active keys with fingerprint, Git folders, hosts and whether one is the default. `--all`
also shows archived keys.

### `keybund show <KEY>`

Everything about one key: file, fingerprint, age, comment, Git identity, signing, folders, hosts, the
last connection test of each host and the public key line, ready to paste.

### `keybund new`

Creates a key. Without options it asks in the terminal: name, generate or take an existing key, a
passphrase, provider and host, Git folder and identity. With options it runs without questions:

| Option | |
|---|---|
| `--name NAME` | display name, e.g. `"Client C"` |
| `--id ID` | ID, default: derived from the name (`client-c`); lowercase letters, digits, `-`, `_` |
| `--color COLOR` | colour in the app: `emerald`, `blue`, `violet`, `amber`, `rose`, `cyan`, `orange`, `zinc` |
| `--provider github\|gitlab\|bitbucket\|generic` | where the key is used |
| `--host HOST` | hostname, e.g. `github.com`, `gitlab.company.com`, `192.168.1.20` |
| `--alias ALIAS` | SSH alias for a server (`nas`) or a second account (`github-work`) |
| `--user USER`, `--port PORT` | login user and port of a server |
| `--folder DIR` | Git folder (repeatable; created if missing) |
| `--git-name`, `--git-email` | identity for commits in the folders; empty: as in `~/.gitconfig` |
| `--sign` | sign commits with this key |
| `--key PATH` | take an existing key instead of generating one |
| `--comment TEXT` | comment of a new key |
| `--default` | make it the default key for Git |
| `--no-apply` | only save it in `profiles.toml`, do not write configuration |
| `--no-browser` | do not open the provider's page to add the key |
| `--yes`, `-y` | no questions |

```sh
keybund new --name "Client C" --provider github --folder ~/dev/client-c \
  --git-email tristan@client-c.com --yes
keybund new --name NAS --key ~/.ssh/id_ed25519 --host 192.168.1.20 --alias nas --user admin --yes
```

A new key is generated with `ssh-keygen -t ed25519`. With a passphrase, it is handed to
`ssh-keygen` through a private askpass helper, never on the command line (where other users could see
it in the process list).

### `keybund edit <KEY>`

Changes an existing key. Everything not given stays as it is.

| Option | |
|---|---|
| `--name`, `--color` | display name and colour |
| `--add-folder DIR`, `--remove-folder DIR` | Git folders (repeatable) |
| `--git-name`, `--git-email` | identity for commits |
| `--sign`, `--no-sign` | commit signing on or off |
| `--add-host HOST` with `--alias`, `--user`, `--port`, `--provider` | add a Git account or server |
| `--remove-host HOST` | remove a host (alias or hostname; repeatable) |
| `--key PATH` | use another existing key file |
| `--no-apply` | only change `profiles.toml` |
| `--yes`, `-y` | write without asking |

```sh
keybund edit client-c --add-host gitlab.client-c.com --provider gitlab
keybund edit privat --sign
```

### `keybund copy <KEY> [--print]`

Copies the public key to the clipboard (`wl-copy` on Wayland, `xclip` or `xsel` on X11). `--print`
writes it to stdout instead – handy for `keybund copy nas --print | ssh admin@nas 'cat >> …'`.

### `keybund keys`

Every key pair in `~/.ssh` with fingerprint, comment and age, and which Keybund key uses it.

### `keybund default <KEY>` / `keybund default --unset`

Sets the default key: Git uses it in every repository outside your Git folders. `--unset` removes it,
so ssh chooses on its own there again. The doctor warns if a default key would override a
`core.sshCommand` you set yourself in `~/.gitconfig`.

### `keybund archive <KEY>`

Moves the key pair to `~/.ssh/archive/`, removes the key's Git folders and `Host` blocks from the
generated files, moves its known_hosts file along and prints the pages where you should remove the
key at the provider. A key file that another active key still uses stays where it is.

## Connections

### `keybund test <KEY> [--host H]`

Tests the connection to each host of the key (or only `H`) with exactly this key
(`IdentitiesOnly=yes`, no password login). Prints the result per host – “Verbunden als @user” on
GitHub/GitLab/Bitbucket, “Anmeldung erfolgreich” on a server – and, on failure, the steps of the
connection, the command to try yourself and the next step (e.g. `keybund install-key …`). Each
host's result is remembered (also with `--host`) and shown on its row in the app.

A login that succeeds with a *different* key (one from your own `~/.ssh/config` or the agent) counts
as a failure: Keybund reads from `ssh -v` which key the server accepted.

### `keybund install-key <KEY> [--host H]`

- **Server:** like `ssh-copy-id` – logs in the way that already works (another key, or the password,
  asked in the terminal) and appends the public key to `~/.ssh/authorized_keys`, creating `~/.ssh`
  with the right permissions if needed. A key that is already there is not added twice.
- **GitHub, GitLab, Bitbucket:** copies the public key and opens the page where you paste it.

### `keybund trust <KEY> [--host H] [--yes]`

Fetches the host keys of the key's servers with `ssh-keyscan`, shows their fingerprints and, after
you confirm, stores them in `~/.ssh/known_hosts.d/<key>` (or `~/.ssh/known_hosts`). `test` and
`install-key` ask the same on first contact. `--yes` accepts without asking – only when you checked
the fingerprint some other way.

### `keybund which [PATH|HOST]`

Answers “which key is used here?” by asking Git and ssh directly:

```text
$ keybund which nas
nas
  Key            ~/.ssh/nas_ed25519
  Fingerprint    SHA256:…
  Key            NAS
  Quelle         ~/.ssh/config.d/keybund.conf · via Host nas
  Ziel           admin@192.168.1.20:22
  IdentitiesOnly ja, nur dieser Key wird angeboten
  Agent          geladen
```

Without an argument it uses the current directory. Folders are resolved through `git config
--show-origin` (including linked worktrees), hosts through `ssh -G`.

### `keybund clone <URL> [--key KEY] [--dest DIR]`

Clones with the key that has the URL's host – asking if several keys fit, or use `--key` – into
`<Git folder>/<repository name>` (or `--dest`). The key is passed with
`git -c core.sshCommand=… clone`, so it is not stored in the repository; from then on the folder rule
applies.

## Configuration files

### `keybund apply [--dry-run] [--yes]`

Computes the generated files from `profiles.toml` and shows a diff of every change. `--dry-run` stops
there; otherwise it asks and writes. `new`, `edit`, `default`, `import` and `archive` apply on their
own; `apply` is for after editing `profiles.toml` by hand, after an update, or when a generated file
was changed outside Keybund.

### `keybund doctor [--fix]`

Runs every check (see [Troubleshooting](troubleshooting.md#doctor-checks)) and exits with `1` if
something needs attention. `--fix` applies the safe fixes: permissions of `~/.ssh` and the keys,
moving the include line to the top, writing pending changes, removing old entries that a key now
covers, and the full-path include for Dolphin.

### `keybund import [--all] [--yes] [--keep-old]`

Takes over `Host` blocks with their own `IdentityFile` from `~/.ssh/config` as keys. Asks per
proposal, or takes all with `--all` (a block whose key file no longer exists is skipped with a note). Afterwards it offers to remove the taken-over blocks (with a
backup); `--keep-old` leaves them.

### `keybund config [NAME] [VALUE]`

Without arguments it shows all settings; with a name it shows one; with a value it changes it:

| Name | Default | |
|---|---|---|
| `projects-dir` | `~/dev` | parent folder for suggested Git folders (`~/dev/<key>/`) |
| `keys-dir` | `~/.ssh` | where new keys are created |
| `known-hosts-per-profile` | `true` | host keys of each key's servers in `~/.ssh/known_hosts.d/<key>` |
| `absolute-ssh-include` | `false` | include with the full path instead of `~` (Dolphin with libssh < 0.10.2) |

Booleans accept `true`/`false`, `on`/`off`, `1`/`0` (and the German `ja`/`nein`, `an`/`aus`).

```sh
keybund config projects-dir ~/work
```

### `keybund uninstall [--restore-backups] [--purge] [--yes]`

Shows and then removes everything Keybund wrote: its generated files and the include lines in
`~/.ssh/config` and `~/.gitconfig`. `--restore-backups` instead restores those two files from the
backups taken before Keybund's first change. `--purge` also deletes `profiles.toml`. Keys are never
deleted.
