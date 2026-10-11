# Troubleshooting

Start with the **doctor** – **Doctor** in the app, `keybund doctor` in the terminal. It checks
everything Keybund depends on and fixes what can be fixed safely (`keybund doctor --fix`, or
**Fix** in the app). For a single folder or host, ask **which key** applies: type it into the
search field in the sidebar, or run `keybund which <folder|host>`.

## Doctor checks

| Check | What it means | Fix |
|---|---|---|
| **N keys loaded** / **profiles.toml is invalid** | `profiles.toml` could not be read or breaks a [validation rule](configuration.md#validation). | Correct the named value; the message says which one. |
| **ssh is not installed** (or ssh-keygen, git) | `ssh`, `ssh-keygen` or `git` is missing. | Install OpenSSH client / Git. |
| **~/.ssh has permissions … instead of 700** | `~/.ssh` is writable by group or others, who could swap keys or the config. | `--fix`: `chmod 700 ~/.ssh` |
| **Private keys with too open permissions** | A private key is readable by others; ssh refuses to use it (`0400` is fine). | `--fix`: `chmod 600` on those keys |
| **ssh does not read this config** | `~/.ssh/config` or `keybund.conf` belongs to someone else (edited with `sudo`) or is writable by group or others. ssh refuses such a file with “Bad owner or permissions” – then no `Host` block applies at all. | `--fix` clears the write bits; a wrong owner needs `chown`. |
| **Key files are missing** | A key's file was moved or deleted. | Restore it, or point the key to another file (`keybund edit <key> --key <path>`). |
| **The Include is not at the top** | Keybund's `Include` is not the first directive of `~/.ssh/config` – then it may only apply to one `Host` block, and earlier values win. | `--fix` moves it to the top. |
| **The Include is missing in ~/.ssh/config** (or ~/.gitconfig) | The include in `~/.ssh/config` or `~/.gitconfig` is missing (file replaced, restored from a backup …). | `--fix` writes it again. |
| **Keybund is no longer at the end of ~/.gitconfig** | Something was added to `~/.gitconfig` after Keybund's include (e.g. by `git config --global`) and may override the folder rules. | Any write moves it back: `keybund apply`. |
| **N file(s) not up to date** | A generated file differs from what `profiles.toml` says – edited or deleted by hand. | `--fix` / `keybund apply` writes it again. |
| **GIT_SSH_COMMAND overrides Keybund** | `GIT_SSH_COMMAND` or `GIT_CONFIG_GLOBAL` is set – in this process, your login shell or a shell config file. Git then ignores Keybund's rules. | Remove the line the doctor names, open a new terminal / log in again. |
| **No ssh-agent reachable** | Without an agent, a key with a passphrase asks on every connection. | Enable your desktop's agent (KDE: ksshaskpass + `ssh-agent`, GNOME: gcr-ssh-agent). |
| **ssh-agent only runs in the terminal** | Your shell profile starts an agent that programs from the menu – Keybund, Dolphin, the IDE – do not see; they ask for the passphrase every time, and a key the app loads is not the one your terminal uses. | Use the desktop's agent instead (see above) and drop the line from the shell profile. |
| **AddKeysToAgent missing** | A generated file lacks `AddKeysToAgent`, so the agent would not remember the key. | `keybund apply`. |
| **Old entry for github.com in ~/.ssh/config** | A block in your own `~/.ssh/config` that Keybund now covers still offers its key – a login can succeed with the wrong key. | `--fix` removes these blocks (with a backup). |
| **Global / Duplicate entry for … in ~/.ssh/config** | A `Host` block in your own config mixes another key into the connection, or duplicates one Keybund writes. | Take it over in Keybund (**Take over existing SSH config**), then remove it. |
| **IdentityFile for all hosts in ~/.ssh/config** | An `IdentityFile` under `Host *`, a `Match` or outside any block is offered to *every* host, in addition to Keybund's key. | Move it into a `Host` block, or take it over as a key. |
| **The default key replaces your core.sshCommand** | You set `core.sshCommand` in `~/.gitconfig` yourself, and the default key overrides it. | Decide which one you want: remove yours, or remove the default key (`keybund default --unset`). |
| **The key of “…” is N years old** | The key is older than two years. | Create a new key, add it at the provider/server, archive the old one. |
| **Git folders do not exist** | A Git folder does not exist (yet). | Create it, or remove it from the key. |
| **Dolphin does not see Keybund's config** | Dolphin/KIO uses libssh, which before 0.10.2 ignores `~` in `Include`. | `--fix` switches to the include with the full path (**Include mit vollem Pfad**). |

## Typical problems

### `git push` says “Permission denied (publickey)”

1. `keybund which` in the repository – is a key set? If not, the repository is outside every Git
   folder and there is no default key: add the folder to the right key.
2. **Test** on the key's Git account (or `keybund test <key>`). “GitHub does not know this key” →
   **Add key** and paste it into the right account.
3. The doctor: is `GIT_SSH_COMMAND` set? It overrides everything.

### `git push` works, but with the wrong account

`ERROR: Permission to org/repo.git denied to other-user` means GitHub recognised a different key.

- The repository is outside the key's folders, or a block in your own `~/.ssh/config` offers another
  key: the doctor reports an **old** or **global entry** – remove it.
- The repository has its own `core.sshCommand` in `.git/config` (`git config --local
  core.sshCommand`): “which key?” then notes that the folder belongs to another key. Remove it with
  `git config --local --unset core.sshCommand`.
- The connection test shows “Logged in, but with … instead of this key”: the same key file is
  registered in two GitHub accounts, or the agent offers another key first – Keybund's
  `IdentitiesOnly` prevents the latter for everything it writes.

### Commits have the wrong e-mail

`git config --show-origin user.email` in the repository shows where it comes from. If it is not
Keybund's `<key>.gitconfig`: the repository is outside the key's folders, the key has no e-mail set
(then `~/.gitconfig` applies), or something after Keybund's include in `~/.gitconfig` overrides it
(the doctor reports that). A repository-local `user.email` always wins.

### “Too many authentication failures”

The server closed the connection after too many keys were offered. Every block Keybund writes has
`IdentitiesOnly yes`; if it still happens, the host is not covered by Keybund – add it as a server,
or check `keybund which <host>`.

### “Host key verification failed” / the test says the host key changed

ssh refuses because the server presents a different host key than the one stored. If you know why
(reinstalled server, new IP), remove the old entry and confirm the new fingerprint:

```sh
ssh-keygen -R <host> -f ~/.ssh/known_hosts.d/<key>   # or ~/.ssh/known_hosts
keybund trust <key> --host <host>
```

If you do not know why, do not connect – ask whoever runs the server.

### The first `git clone` uses the wrong key

`includeIf "gitdir:"` can only apply once the repository exists. Use **Clone repository** or
`keybund clone <url>`, which pass the key directly.

### A passphrase is asked on every push

No agent is running, or the key is not added to it, or the agent only exists in your terminal (started
from `.bashrc`/`.zshrc`) while the app, Dolphin and the IDE run without one. The doctor shows
**No ssh-agent reachable** or **ssh-agent only runs in the terminal**.
With an agent, `AddKeysToAgent yes` adds the key after the first use; **Loaded in the agent** on the key
page confirms it.

### The test says “Wrong passphrase”, “Passphrase prompt cancelled” or “Could not ask for the passphrase”

ssh asks for the passphrase only after the server accepted the key, so these are never about the
server. **Wrong passphrase**: ssh asked up to three times and was refused. **Cancelled**: the
dialog was closed – run the test again. **Could not ask**: no askpass program is installed and the
app has no terminal; install `ksshaskpass` (KDE) or `ssh-askpass`, or load the key once with
`ssh-add` in a terminal, then the agent answers for the app too.

### The test says “Login successful – but the server runs no commands”

The server accepted the key and then exited with an error: the account only runs a forced command
(`command="…"` in `authorized_keys`, `git-shell`, a backup user, Gitea's `git` user). That is a
working login for what the account is meant to do.

### The app cannot open the browser

From a toolbox/distrobox Keybund uses `flatpak-spawn --host xdg-open`; from an AppImage it starts the
browser with a clean environment, with `gio open` as a fallback. If nothing works, the app shows the
link to copy.

### Dolphin: `sftp://nas` asks for a password or does not find the host

See the doctor's Dolphin check above. Dolphin also needs the host's key in a known_hosts file it
reads; test the server once in Keybund so the host key is confirmed.

## Starting over

- `keybund apply` writes every generated file again from `profiles.toml`.
- `keybund uninstall --restore-backups` restores `~/.ssh/config` and `~/.gitconfig` as they were
  before Keybund and removes all generated files; `profiles.toml` stays, so `keybund apply` sets
  everything up again.
- Still stuck? [Open an issue](https://github.com/firsttris/keybund/issues/new) with the output of
  `keybund doctor` and `keybund which` (they contain paths and fingerprints, no private data).
