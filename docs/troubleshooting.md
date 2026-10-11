# Troubleshooting

Start with the **doctor** – **Doctor** in the app, `keybund doctor` in the terminal. It checks
everything Keybund depends on and fixes what can be fixed safely (`keybund doctor --fix`, or
**Beheben** in the app). For a single folder or host, ask **which key** applies: type it into the
search field in the sidebar, or run `keybund which <folder|host>`.

## Doctor checks

| Check (German title) | What it means | Fix |
|---|---|---|
| **profiles.toml** – “N Keys geladen” / “ist ungültig” | `profiles.toml` could not be read or breaks a [validation rule](configuration.md#validation). | Correct the named value; the message says which one. |
| **Tools** – “ssh ist nicht installiert” | `ssh`, `ssh-keygen` or `git` is missing. | Install OpenSSH client / Git. |
| **~/.ssh hat Rechte …** | `~/.ssh` is writable by group or others. ssh then refuses keys and configs. | `--fix`: `chmod 700 ~/.ssh` |
| **Private Keys haben Rechte 600** / “zu offenen Rechten” | A private key is readable by others; ssh refuses to use it. | `--fix`: `chmod 600` on the keys |
| **Key-Dateien fehlen** | A key's file was moved or deleted. | Restore it, or point the key to another file (`keybund edit <key> --key <path>`). |
| **Include steht ganz oben** / “nicht ganz oben” | Keybund's `Include` is not the first directive of `~/.ssh/config` – then it may only apply to one `Host` block, and earlier values win. | `--fix` moves it to the top. |
| **Include fehlt** | The include in `~/.ssh/config` or `~/.gitconfig` is missing (file replaced, restored from a backup …). | `--fix` writes it again. |
| **Keybund steht nicht mehr am Ende von ~/.gitconfig** | Something was added to `~/.gitconfig` after Keybund's include (e.g. by `git config --global`) and may override the folder rules. | Any write moves it back: `keybund apply`. |
| **Configs sind aktuell** / “N Datei(en) nicht aktuell” | A generated file differs from what `profiles.toml` says – edited or deleted by hand. | `--fix` / `keybund apply` writes it again. |
| **Keine Umgebungsvariable übersteuert Git** / “GIT_SSH_COMMAND übersteuert Keybund” | `GIT_SSH_COMMAND` or `GIT_CONFIG_GLOBAL` is set – in this process, your login shell or a shell config file. Git then ignores Keybund's rules. | Remove the line the doctor names, open a new terminal / log in again. |
| **ssh-agent läuft** / “Kein ssh-agent erreichbar” | Without an agent, a key with a passphrase asks on every connection. | Enable your desktop's agent (KDE: ksshaskpass + `ssh-agent`, GNOME: gcr-ssh-agent) or start one in your shell profile. |
| **AddKeysToAgent aktiv** / “fehlt” | A generated file lacks `AddKeysToAgent`, so the agent would not remember the key. | `keybund apply`. |
| **Alter Eintrag für github.com in ~/.ssh/config** | A block in your own `~/.ssh/config` that Keybund now covers still offers its key – a login can succeed with the wrong key. | `--fix` removes these blocks (with a backup). |
| **Globaler / Doppelter Eintrag für … in ~/.ssh/config** | A `Host` block in your own config mixes another key into the connection, or duplicates one Keybund writes. | Take it over in Keybund (**Bestehende SSH-Config übernehmen**), then remove it. |
| **IdentityFile für alle Hosts in ~/.ssh/config** | An `IdentityFile` under `Host *`, a `Match` or outside any block is offered to *every* host, in addition to Keybund's key. | Move it into a `Host` block, or take it over as a key. |
| **Standard-Key ersetzt dein core.sshCommand** | You set `core.sshCommand` in `~/.gitconfig` yourself, and the default key overrides it. | Decide which one you want: remove yours, or remove the default key (`keybund default --unset`). |
| **Key von „…“ ist N Jahre alt** | The key is older than two years. | Create a new key, add it at the provider/server, archive the old one. |
| **Git-Ordner existieren nicht** | A Git folder does not exist (yet). | Create it, or remove it from the key. |
| **Dolphin sieht die Keybund-Config nicht** | Dolphin/KIO uses libssh, which before 0.10.2 ignores `~` in `Include`. | `--fix` switches to the include with the full path (**Include mit vollem Pfad**). |

## Typical problems

### `git push` says “Permission denied (publickey)”

1. `keybund which` in the repository – is a key set? If not, the repository is outside every Git
   folder and there is no default key: add the folder to the right key.
2. **Testen** on the key's Git account (or `keybund test <key>`). “GitHub kennt diesen Key nicht”
   (GitHub does not know this key) → **Key eintragen** and paste it into the right account.
3. The doctor: is `GIT_SSH_COMMAND` set? It overrides everything.

### `git push` works, but with the wrong account

`ERROR: Permission to org/repo.git denied to other-user` means GitHub recognised a different key.

- The repository is outside the key's folders, or a block in your own `~/.ssh/config` offers another
  key: the doctor reports **Alter/Globaler Eintrag** – remove it.
- The repository has its own `core.sshCommand` in `.git/config` (`git config --local
  core.sshCommand`): “which key?” then notes that the folder belongs to another key. Remove it with
  `git config --local --unset core.sshCommand`.
- The connection test shows “Angemeldet, aber mit … statt mit diesem Key”: the same key file is
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

`includeIf "gitdir:"` can only apply once the repository exists. Use **Repo klonen** or
`keybund clone <url>`, which pass the key directly.

### A passphrase is asked on every push

No agent is running, or the key is not added to it. The doctor shows **Kein ssh-agent erreichbar**.
With an agent, `AddKeysToAgent yes` adds the key after the first use; **Im Agent geladen** on the key
page confirms it.

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
