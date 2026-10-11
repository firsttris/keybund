# Security and host keys

Keybund manages the files that decide which key ssh and Git use, so it is built to never weaken
what OpenSSH already does well. This page lists what it does, what it never does, and why.

## Private keys

- **Never read.** Keybund only works with paths, fingerprints and public keys (`<key>.pub`). Every
  operation that needs the private key – login, signing – is done by `ssh`, `ssh-keygen` or `git`
  itself.
- **Created by `ssh-keygen`.** New keys are Ed25519 (`ssh-keygen -t ed25519`), stored with mode
  `600`.
- **Passphrases stay off the command line.** A passphrase for a new key is handed to `ssh-keygen`
  through a tiny askpass helper in a private folder (mode `700`) that reads it from an environment
  variable of that one process – it never appears in the process list (`ps`) where other users could
  see it. The folder is removed right after.
- **The agent instead of storing passphrases.** Keybund stores no passphrase anywhere. With
  `AddKeysToAgent yes` you type it once per session and `ssh-agent` keeps the unlocked key in memory;
  the doctor checks that an agent is reachable.
- **Archiving moves, never deletes.** Archived keys go to `~/.ssh/archive/` (mode `700`) with their
  public key and known_hosts file; uninstalling never touches keys.

## Only the right key is offered

Every block and every `core.sshCommand` Keybund writes contains `IdentitiesOnly yes`: ssh offers
exactly that key, not everything in the agent. That matters for security as much as for convenience:

- a client's server never learns which other keys you have;
- GitHub cannot log you in as the wrong account because an unrelated key happened to be tried first.

The connection test goes further: it reads from `ssh -v` *which* key the server accepted and
reports a login with another key as a failure (“Logged in, but with … instead of this key”).
During the test, password and keyboard-interactive logins, Kerberos (GSSAPI), host-based login and
shared master connections (`ControlPath`) are switched off, so nothing but the key itself can make a
test pass.

## Host keys

A host key proves that you are talking to the real server and not to someone in between.
`StrictHostKeyChecking=accept-new` – the convenient default of many tools – trusts the first key it
sees. Keybund does not:

- **GitHub, GitLab.com and Bitbucket**: their published host keys (Ed25519 and ECDSA) are built into
  Keybund and written to `~/.ssh/known_hosts` without a question – there is nothing for you to
  compare. Keybund asks `ssh -G` which files ssh reads: if your own `~/.ssh/config` sets
  `UserKnownHostsFile` (under `Host *`, say), the keys go to the first of those, where ssh will find
  them, and only those files count as "known".
- **Every other host**: before the first connection – a test, adding a key, `keybund trust` – Keybund
  fetches the host keys with `ssh-keyscan` and shows their **SHA256 fingerprints**. Only after you
  confirm (**Fingerprint matches**) are they stored, and ssh connects with
  `StrictHostKeyChecking=yes`. Compare the fingerprint with what the server shows locally:
  `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub`, or your hoster's console.
- **A changed host key** is never accepted silently: ssh refuses the connection and the test says
  so. If the change is expected (a reinstalled server), remove the old entry
  (`ssh-keygen -R <host> -f ~/.ssh/known_hosts.d/<key>`) and confirm the new one.

### known_hosts per key

With **Server fingerprints per key** (on by default), the host keys of a key's servers are stored in
`~/.ssh/known_hosts.d/<key>`, and its `Host` blocks read

```sshconfig
UserKnownHostsFile ~/.ssh/known_hosts.d/<key> ~/.ssh/known_hosts
UpdateHostKeys yes
```

New host keys go to the key's own file; keys already in `~/.ssh/known_hosts` still count. When a
client project ends and you archive its key, its servers' host keys leave with it
(`~/.ssh/archive/known_hosts-<key>`) instead of staying in a shared file forever.
`UpdateHostKeys yes` lets ssh learn rotated host keys for this file too (by default ssh only does
that for `~/.ssh/known_hosts`).

## Values cannot inject configuration

Names, hosts, users, folders and key paths end up in `ssh_config` and gitconfig files. Keybund
validates them before saving (details in [Configuration](configuration.md#validation)): no line
breaks or control characters (which would add directives), no wildcards or spaces in host patterns
(which would match other hosts), no glob characters in Git folders, no `%` or `${` in key paths
(which ssh would expand), no leading `-` (which a program could read as an option). Git values are
quoted where needed. Hostnames and URLs are passed to `ssh`, `ssh-keyscan` and `git clone` after
`--`, so they can never be read as options; the clone helper only accepts SSH, Git and HTTPS URLs
(not `ext::` or local paths).

## Files and permissions

- Writes are atomic (temporary file, `fsync`, rename) and re-checked against the content they were
  planned from, so a file changed meanwhile is never overwritten unseen.
- `~/.ssh/config` and Keybund's files below `~/.ssh` are kept at `600`; `profiles.toml` and Keybund's
  config folder at `600`/`700`.
- The doctor reports a group- or world-writable `~/.ssh` and private keys readable by others, and
  `keybund doctor --fix` corrects them (`700` for `~/.ssh`, `600` for keys).
- Your `~/.ssh/config` and `~/.gitconfig` get a backup before the first change.

## Network

Keybund itself makes no network connections and has no telemetry. The only connections are the
ones you trigger, made by OpenSSH and Git: a connection test, adding a key to a server
(`ssh`), fetching host keys (`ssh-keyscan`) and cloning (`git`). “Key eintragen” for GitHub, GitLab
and Bitbucket opens a page in your browser – Keybund needs no token and no login there.

## Environment variables that switch Keybund off

- **`GIT_SSH_COMMAND`** overrides every `core.sshCommand`, so Git would ignore the folder rules.
- **`GIT_CONFIG_GLOBAL`** makes Git read another file instead of `~/.gitconfig`, so Keybund's include
  is never seen.

They are usually set in a shell profile. A desktop app does not inherit those, so Keybund also asks
your login shell and searches `~/.bashrc`, `~/.zshrc`, `~/.profile`, fish's `config.fish` and similar
files, and the doctor names the line that sets them.

## Reporting a vulnerability

Please do not report security problems in a public issue. Contact the author privately through
[teufel-it.de](https://teufel-it.de) instead.
