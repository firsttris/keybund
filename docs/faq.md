# FAQ

## How do I use two GitHub accounts on the same computer?

Create one key per account and give each a Git folder (`~/dev/private/`, `~/dev/work/`). Keybund
writes an `includeIf "gitdir:…"` per folder with that key's `core.sshCommand`, so every repository
below a folder pushes with its key – with the normal `git@github.com:…` URL. Alternatively give an
account an alias and use `git@github-work:org/repo.git`. Step by step:
[Two GitHub accounts on one computer](guides.md#two-github-accounts-on-one-computer).

## How do I use a different Git e-mail per folder or client?

Set the e-mail (and optionally the name) in the key's **Git-Identität**. It applies in the key's Git
folders; everywhere else your identity from `~/.gitconfig` stays. See
[A different Git e-mail per client](guides.md#a-different-git-e-mail-per-client).

## Does Keybund have to keep running?

No. It writes standard OpenSSH and Git configuration and is done. ssh and Git read those files on
their own – in the terminal, the IDE, Dolphin, cron jobs. Keybund only runs while you change
something.

## Does Keybund replace ssh-agent, or upload my keys anywhere?

No and no. Keybund has no agent, daemon or protocol of its own and makes no network connections
apart from those you trigger with OpenSSH and Git. It never reads private keys. The public key is
added at GitHub, GitLab or Bitbucket by you, in your browser – Keybund copies it and opens the page.

## Which key type does Keybund create? Do I need RSA?

Ed25519: short, fast, secure, and accepted by GitHub, GitLab, Bitbucket and every OpenSSH server of
the last ten years. You do not need RSA unless a very old server requires it; you can still take
over an existing RSA or ECDSA key (**Vorhandenen Key nehmen**).

## What happens to my existing `~/.ssh/config` and `~/.gitconfig`?

Each gets one include line – at the top of `~/.ssh/config`, at the end of `~/.gitconfig` – after a
backup. Everything else in them stays untouched. Existing `Host` blocks with their own key can be
taken over into Keybund; removing them afterwards is offered (and recommended), again with a backup.
Details: [What Keybund writes](generated-files.md).

## Can I go back?

Yes: **Einstellungen → Keybund entfernen** or `keybund uninstall` removes every generated file and
both include lines, optionally restoring the two files from their backups. Your keys stay.

## Can I edit the generated files?

Not usefully – they are overwritten on the next change (that is what the “Managed by Keybund” header
says). Put your own settings into `~/.ssh/config` and `~/.gitconfig` as usual; Keybund's rules are
included from there. Settings you need per key belong in `profiles.toml`.

## Why does Keybund not write `Host github.com`?

A `Host github.com` block with an `IdentityFile` adds that key to *every* connection to GitHub, also
when another key should be used – the classic cause of “pushed with the wrong account”. Keybund
chooses the key per folder through Git's `core.sshCommand` instead, and writes `Host` blocks for Git
providers only with an alias (`Host github-work`).

## Does it work with GitLab self-hosted, Gitea, Forgejo, Azure DevOps?

Self-hosted GitLab: yes, as a GitLab account with its hostname (the “add key” page opens there).
Gitea, Forgejo and others: add the host as a **Server** – folder rules, aliases and tests work the
same, only the page for adding the key is not opened for you.

## Does it work with Dolphin, VS Code, JetBrains IDEs, `rsync`, `scp`?

Yes – they all use OpenSSH's or Git's configuration. Dolphin reads it through libssh; for libssh
before 0.10.2 the doctor offers the include with the full path.

## Does Keybund support FIDO2 / YubiKey keys (`ed25519-sk`)?

Existing `-sk` keys can be taken over like any other key file. Creating them in Keybund is not
supported yet.

## Is there a Windows or macOS version?

Not yet. Keybund is developed on Fedora/Bazzite and tested on Ubuntu in CI. macOS and then Windows
are planned.

## Why is the app in German?

For now it is German only. The documentation is in English, and an English interface is
planned – contributions are welcome.

## How is Keybund licensed?

GPL-3.0. You may use it for anything, also commercially. If you pass on a changed version, it has
to stay under the GPL with its source code. A commercial license without these obligations is
available via [teufel-it.de](https://teufel-it.de).
