# Keybund

**SSH-Keys pro Projekt.** Keybund ordnet jedem Projekt ein Profil zu – Key, Hosts, Ordner, Git-Identität – und schreibt
daraus Standard-Configs für OpenSSH und Git. Danach nehmen Terminal, Git und Dolphin automatisch den richtigen Key,
ohne dass Keybund läuft.

- **Kein eigener Agent, kein eigenes Protokoll.** Keybund erzeugt nur `~/.ssh/config`-Blöcke und Gitconfig-Snippets.
- **Eigene Dateien statt Fremdeingriff.** In deine Configs kommt genau je eine Include-Zeile, mit einmaligem Backup.
- **Private Keys werden nie gelesen.** Keybund kennt nur Pfade, Fingerprints und Public Keys; erzeugt wird per `ssh-keygen`.
- **Vorschau vor jedem Schreiben.** Jede Änderung wird als Diff gezeigt und erst nach Bestätigung atomar geschrieben.
- **Ein Rust-Core, zwei Oberflächen:** CLI und GUI (Tauri). KRunner folgt.

## Wofür ein Key gilt

Ein Profil ist ein Key plus die Stellen, an denen er benutzt wird. Keybund kennt drei Arten, wie der Key gewählt wird:

| Kategorie | Gilt für | gewählt über |
| --- | --- | --- |
| **Standard** | alles, wofür kein anderes Profil passt | `core.sshCommand` in `index.gitconfig` |
| **Projekte & Konten** | Git-Repos (GitHub, GitLab, Bitbucket) in den Projektordnern des Profils | `includeIf "gitdir:…"` |
| **Server** | `ssh`, `scp`, `sftp`, `rsync` auf einen Host oder eine IP | `Host`-Block mit Alias und Hostname |

Ein Profil kann alles zugleich sein – z. B. „Privat“ als Standard, mit GitHub und deinem Heimserver. Ein Server
bekommt im Wizard („Neu“ → „Server“) einen eigenen Key oder den Key eines bestehenden Profils; Keybund trägt ihn
danach auf dem Server ein (wie `ssh-copy-id`: Anmeldung mit Passwort oder einem Key, der dort schon gilt).

## Key bei GitHub und GitLab eintragen

Keybund trägt den Pubkey per API in dein Konto ein („Key eintragen“ im Profil, im Wizard oder
`keybund install-key`). Auf github.com und gitlab.com meldest du dich dafür im Browser an: Keybund zeigt einen Code,
du bestätigst ihn, fertig – kein Passwort, kein Token zum Abtippen. Selbst gehostetes GitLab und GitHub Enterprise
nehmen ein persönliches Zugriffstoken; Keybund öffnet die Seite dafür mit den richtigen Berechtigungen.

Tokens werden nach dem Eintragen verworfen. Nur wenn du „Token im Schlüsselbund merken“ wählst, landen sie per
`secret-tool` im Schlüsselbund (GNOME Keyring, KWallet) – je Profil und Host, weil jedes Profil meist ein anderes Konto
ist. Achte beim Anmelden darauf, im Browser mit dem Konto des Profils angemeldet zu sein; Keybund zeigt danach, bei
welchem Konto der Key gelandet ist. Bitbucket hat keine passende API, dort öffnet sich die Seite zum Eintragen.

Die Anmeldung im Browser braucht registrierte OAuth-Apps: [docs/oauth-apps.md](docs/oauth-apps.md).

## Host-Keys

Keybund vertraut keinem Server stillschweigend. Die veröffentlichten Host-Keys von GitHub, GitLab.com und
Bitbucket sind eingebaut und werden ohne Rückfrage nach `~/.ssh/known_hosts` geschrieben. Bei jedem anderen Host
zeigt Keybund vor der ersten Verbindung den Fingerprint (App: Dialog, CLI: Rückfrage oder `keybund trust`), und
erst nach der Bestätigung verbindet sich ssh – mit `StrictHostKeyChecking=yes`.

Die Host-Keys der Server eines Profils landen in einer eigenen Datei `~/.ssh/known_hosts.d/<profil>` (der
`Host`-Block bekommt `UserKnownHostsFile ~/.ssh/known_hosts.d/<profil> ~/.ssh/known_hosts`) und wandern beim
Archivieren mit nach `~/.ssh/archive/`. Abschaltbar in den Einstellungen bzw. mit
`keybund config known-hosts-per-profile false`.

## Wie es funktioniert

`~/.config/keybund/profiles.toml` ist die einzige Quelle der Wahrheit:

```toml
default_profile = "privat"

[[profile]]
id = "privat"
name = "Privat"
color = "emerald"
key = { path = "~/.ssh/privat_ed25519", comment = "tristan@zentrale" }
git = { name = "Tristan", email = "tristan@privat.de", sign = true }
folders = ["~/dev/privat/"]

[[profile.host]]
alias = "meinserver"
hostname = "1.2.3.4"
user = "tristan"
provider = "generic"

[[profile]]
id = "sparkasse"
name = "Sparkasse"
key = { path = "~/.ssh/sparkasse_ed25519" }
git = { name = "Tristan", email = "tristan@kunde.de" }
folders = ["~/dev/sparkasse/"]

[[profile.host]]
hostname = "gitlab.kunde.de"
provider = "gitlab"
```

Daraus schreibt Keybund:

| Datei | Inhalt | eingebunden über |
| --- | --- | --- |
| `~/.ssh/config.d/keybund.conf` | `Host`-Blöcke für Server mit Alias, je `IdentityFile` + `IdentitiesOnly yes` | `Include` ganz oben in `~/.ssh/config` |
| `~/.config/keybund/git/index.gitconfig` | `core.sshCommand` des Standard-Profils, ein `includeIf "gitdir:…/"` pro Ordner | `[include]` in `~/.gitconfig` |
| `~/.config/keybund/git/<id>.gitconfig` | `user.name`, `user.email`, `core.sshCommand = ssh -i <key> -o IdentitiesOnly=yes -o AddKeysToAgent=yes`, Signing | `index.gitconfig` |
| `~/.config/keybund/allowed_signers` | E-Mail + Pubkey je signierendem Profil | `gpg.ssh.allowedSignersFile` |

## Umstieg mit bestehender `~/.ssh/config`

Hast du schon Einträge wie `Host github.com` mit `IdentityFile`, übernimmt Keybund sie: in der App unter
**Bestehende SSH-Config übernehmen** (die Startseite weist darauf hin) oder mit `keybund import`.

1. Aus jedem `Host`-Block mit eigenem Key wird ein Profil mit genau diesem Key – nichts wird verschoben.
   Ein `Host github.com` wird zum Standard-Profil, Aliase wie `Host pflegeabc` bleiben Aliase.
2. Keybund schreibt seine Configs.
3. Die übernommenen Blöcke werden aus `~/.ssh/config` entfernt (mit Diff und Backup). Das ist wichtig: Solange dort
   ein `IdentityFile` für github.com steht, bietet ssh diesen Key zusätzlich zum Profil-Key an, und eine Anmeldung kann
   mit dem falschen Key gelingen. Der Verbindungstest prüft deshalb, welcher Key tatsächlich akzeptiert wurde, und der
   Doctor meldet solche Einträge.

## CLI

```text
keybund list                     Profile mit Fingerprint, Ordnern, Hosts
keybund show <profil>            Details und Pubkey
keybund edit <profil> …          Name, Ordner, Hosts, Git-Identität, Signieren, Key ändern (siehe --help)
keybund new                      Wizard im Terminal (oder komplett über Optionen, siehe --help)
keybund copy <profil> [--print]  Pubkey in die Zwischenablage (wl-copy, xclip oder xsel)
keybund test <profil> [--host h] Verbindungstest, Exit-Code 0 bei Erfolg
keybund install-key <profil> [--host h] [--token-stdin] [--remember]
                                 Pubkey eintragen: Server wie ssh-copy-id, GitHub/GitLab per API
keybund trust <profil> [--host h] Host-Keys prüfen und bestätigen (Fingerprint)
keybund config [key] [wert]      Einstellungen: projects-dir, keys-dir, known-hosts-per-profile, …
keybund which [pfad|host]        Welcher Key wird hier benutzt – und warum?
keybund clone <url> [-p id]      Klont ins richtige Profil, mit dem richtigen Key
keybund apply [--dry-run] [-y]   Diff zeigen bzw. schreiben
keybund doctor [--fix]           Alle Checks, Exit-Code ≠ 0 bei Problemen
keybund keys                     Vorhandene Keys in ~/.ssh
keybund import [--all] [-y]      Bestehende ~/.ssh/config übernehmen, alte Einträge danach entfernen
keybund uninstall [--restore-backups] [--purge]   Alles entfernen, was Keybund geschrieben hat (Keys bleiben)
keybund default <profil>         Standard-Profil setzen
keybund archive <profil>         Key nach ~/.ssh/archive, Configs entfernen
```

Beispiel ohne Rückfragen:

```sh
keybund new --name "Kunde C" --provider github --folder ~/dev/kunde-c \
  --git-name Tristan --git-email tristan@kunde-c.de --yes
```

## GUI

Profile nach Kategorie (Standard, Projekte & Konten, Server) mit Detailseite (Key, Verbindungstest, Git-Konten,
Server mit „Key eintragen“ und Test pro Server, Ordner, Git-Identität, generierte Config), Wizard „Neu“ für
Git-Projekte (Pubkey landet in der Zwischenablage, die Provider-Seite öffnet sich, danach Verbindungstest) und Server
(Key eintragen, Test), „Welcher Key?“ mit Doctor, Clone-Helfer, Diff-Ansicht zum Anwenden und Einstellungen mit
Import vorhandener Keys.

## Aufbau

```text
crates/core/        keybund-core: store, keygen, render, apply, doctor, which, probe, clone, provider, archive
crates/cli/         keybund (clap) – dünne Schicht über dem Core
app/src-tauri/      Tauri-Commands (api.rs testbar, commands.rs dünne Wrapper)
app/src/            React + TanStack Router/Query + shadcn/ui (Zinc)
app/src/lib/bindings.ts   von tauri-specta generiert (`just bindings`)
app/src/lib/mock/   In-Memory-Backend mit allen Commands (Browser-Vorschau, Tests)
packaging/          .desktop-Datei, Fedora-Toolbox-Setup
```

## Entwickeln

Unter Bazzite/Fedora am besten in einer Toolbox, damit das Host-System sauber bleibt:

```sh
packaging/toolbox.sh            # einmalig: Toolbox mit webkit2gtk, Rust, Node, pnpm, just
toolbox enter keybund-dev
just setup                      # Frontend-Abhängigkeiten
just dev                        # App mit Hot Reload
just dev-mock                   # nur das Frontend im Browser, gegen das Mock-Backend
just install                    # CLI + AppImage nach ~/.local/bin, .desktop-Datei
```

## Tests

Jede Schicht hat eigene Tests, alle laufen in der CI (`.github/workflows/ci.yml`):

| Schicht | Was | Wo |
| --- | --- | --- |
| Unit (Rust) | Parsing, Validierung, Diff, Include-Logik, Provider-Ausgaben, URL-Parsing | `crates/*/src/**` |
| Snapshot | `render` ist rein funktional, jede Profil-Kombination hat eine Golden-File (insta) | `crates/core/src/snapshots/` |
| Integration (Rust) | `apply` gegen ein temporäres HOME, Auflösung mit echtem `git` und `ssh -G`, Doctor, Archiv | `crates/core/tests/` |
| Integration (sshd) | Echter `sshd` auf localhost: Passphrase einmal, danach hält der Agent den Key (Test und `git`), falscher Key erkannt, Key auf Server eintragen | `crates/core/tests/sshd.rs` |
| Integration (API) | Device Flow und Key-Upload gegen eine nachgebaute GitHub-/GitLab-API (mockito) | `crates/core/tests/upload.rs` |
| Integration (CLI) | Jeder Befehl gegen ein temporäres HOME, Fake-`ssh`/`secret-tool`, Mock-API | `crates/cli/tests/cli.rs` |
| E2E (CLI) | Kompletter Lebenszyklus: zwei Kunden auf github.com, Server, Clone, Doctor, Archiv | `crates/cli/tests/e2e.rs` |
| IPC (Tauri) | Commands über die Tauri-IPC mit Mock-Runtime und echten Capabilities | `app/src-tauri/tests/ipc.rs` |
| Vertrag | `bindings.ts` muss zu den Rust-Commands passen; Mock-Backend verhält sich wie der Core | `app/src-tauri/tests/bindings.rs`, `app/src/lib/mock/*.test.ts` |
| Unit/Komponenten (Frontend) | Vitest + Testing Library, ganze App gegen das Mock-Backend, Coverage-Schwellen | `app/src/**/*.test.ts(x)` |
| E2E (Frontend) | Playwright, alle Flows auf Desktop- und schmalem Viewport | `app/e2e/` |

```sh
just            # lint + alle Tests
just test-rust  # nur Rust
just e2e        # nur Playwright
just coverage   # Rust-Coverage (cargo-llvm-cov)
```

Vor jedem Release zusätzlich von Hand: `ssh meinserver`, `sftp://meinserver` in Dolphin, Push in zwei Kundenordnern,
Clone über den Helfer, `keybund doctor` grün.

## Release

Die Version steht nur in `Cargo.toml` (`[workspace.package]`); `app/package.json` wird mitgezogen, die Tauri-App
übernimmt sie aus Cargo. Ein Release startet ausschließlich über einen Tag `vX.Y.Z`:

1. **Actions → Bump version → Run workflow**, `patch`, `minor` oder `major` wählen. Der Workflow erhöht die Version,
   committet `Release vX.Y.Z` auf `main`, setzt den Tag und startet `release.yml`.
2. `release.yml` führt die komplette CI aus (`ci.yml` per `workflow_call`), baut auf Ubuntu 22.04 das CLI-Binary,
   das AppImage und das `.deb` und legt das GitHub-Release mit generierten Notes und `SHA256SUMS` an.
   Eine Version mit `-` (z. B. `0.2.0-rc.1`) wird ein Pre-Release.

Alternativ von einem Checkout: Version in `Cargo.toml` und `app/package.json` anpassen, `cargo check` (aktualisiert
`Cargo.lock`), committen, `git tag v0.2.0 && git push --follow-tags`. Stimmen Tag und `Cargo.toml` nicht überein,
bricht das Release ab. `release.yml` von Hand auf einem Branch gestartet baut nur, ohne zu veröffentlichen – gut, um
das Packaging zu testen.

Die Workflows kommen aus [firsttris/workflows](https://github.com/firsttris/workflows) (`bump-version.yml`,
`github-release.yml`, jeweils `@v1`).

## Stand

- [x] Phase 0 – Gerüst (Workspace, Tauri 2, React, TanStack, shadcn, tauri-specta, CI)
- [x] Phase 1 – Core + CLI
- [x] Phase 2 – GUI-MVP
- [x] Phase 3 – Clone-Helfer, „Welcher Key?“, Archivieren, SSH-Signing, Warnung bei alten Keys
- [x] Key-Upload per API (GitHub, GitLab; Device Flow und Token), Key auf Server eintragen
- [x] Host-Keys bestätigen, known_hosts pro Profil, letzter Test bleibt gespeichert, Ordner einstellbar
- [ ] Phase 4 – KRunner, Flatpak
- [ ] Phase 5/6 – macOS und Windows

## Stolperfallen, die Keybund abfängt

- `Include` unter einem `Host`-Block gilt nur für diesen Host → Keybund setzt es als erste Zeile, `doctor` prüft die Position.
- „Too many authentication failures“ → `IdentitiesOnly yes` überall.
- `includeIf` greift beim ersten `git clone` nicht → `keybund clone` übergibt `core.sshCommand` direkt.
- `gitdir:` ohne abschließenden `/` matcht keine Unterordner → Ordner werden normalisiert (Slash, Symlinks aufgelöst).
- `ssh -T` bei GitHub endet auch bei Erfolg mit Exit-Code 1 → Erfolg wird an der Ausgabe erkannt.
- Keys mit Passphrase → `AddKeysToAgent=yes` in jedem `core.sshCommand` und Host-Block: einmal eingeben (Terminal
  oder ksshaskpass), danach hält der Agent den Key. Der Verbindungstest nutzt deshalb kein `BatchMode`; `doctor` prüft
  die geschriebenen Dateien.
- Erster Kontakt blockiert im BatchMode → Test mit `StrictHostKeyChecking=accept-new`.
- Browser aus einer Toolbox oder einem AppImage öffnen → `flatpak-spawn --host xdg-open` bzw. eine von
  AppImage-Pfaden bereinigte Umgebung, Fallback `gio open`; klappt es nicht, zeigt die App den Link zum Kopieren.
- Zu offene Rechte → `doctor --fix` setzt 700/600.
- Ein globaler `Host github.com` mit `IdentityFile` mischt sich ein → `doctor` warnt.

---

<div align="center">

⭐ Gefällt dir Keybund? Ein [Stern auf GitHub](https://github.com/firsttris/keybund) hilft anderen, es zu finden.<br>
🐛 [Fehler melden](https://github.com/firsttris/keybund/issues/new) · 💡 [Idee vorschlagen](https://github.com/firsttris/keybund/issues/new)

<sub>Lizenz: <a href="LICENSE">GPL-3.0</a> · © Tristan Teufel und Mitwirkende<br>
Wer eine veränderte Version weitergibt, muss sie unter der GPL und mit ihrem Quellcode weitergeben; eine kommerzielle Lizenz ohne diese Pflichten gibt es über <a href="https://teufel-it.de">teufel-it.de</a>.<br>
Keybund steht in keiner Verbindung zu GitHub, GitLab, Atlassian (Bitbucket) oder dem OpenSSH-Projekt.</sub>

</div>
