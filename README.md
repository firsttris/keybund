# Keybund

**Der richtige SSH-Key, automatisch.** In Keybund legst du Keys an und sagst bei jedem, wo er gilt – Git-Ordner,
GitHub/GitLab-Konten, Server. Daraus schreibt Keybund Standard-Configs für OpenSSH und Git. Danach nehmen Terminal, Git und Dolphin automatisch den richtigen Key,
ohne dass Keybund läuft.

- **Kein eigener Agent, kein eigenes Protokoll.** Keybund erzeugt nur `~/.ssh/config`-Blöcke und Gitconfig-Snippets.
- **Eigene Dateien statt Fremdeingriff.** In deine Configs kommt genau je eine Include-Zeile, mit einmaligem Backup.
- **Private Keys werden nie gelesen.** Keybund kennt nur Pfade, Fingerprints und Public Keys; erzeugt wird per `ssh-keygen`.
- **Sofort wirksam, nachvollziehbar.** Die App schreibt jede Änderung gleich (atomar, deine eigenen Dateien vorher mit
  Backup); die generierte Config jedes Keys ist jederzeit einsehbar. Die CLI zeigt vor dem Schreiben den Diff.
- **Ein Rust-Core, zwei Oberflächen:** CLI und GUI (Tauri). KRunner folgt.

## Der Key steht im Mittelpunkt

In Keybund legst du **Keys** an – „Privat“, „Kunde C“, „NAS“ – und sagst bei jedem, **wo er benutzt wird**:

| Verwendung | Bedeutet | so wählt Git/ssh den Key |
| --- | --- | --- |
| **Git-Ordner** | Repos in `~/dev/kunde-c/` nutzen diesen Key und Name/E-Mail für Commits | `includeIf "gitdir:…"` |
| **Git-Konto** | GitHub, GitLab, Bitbucket – dort muss der Pubkey stehen („Key eintragen“) | Ordner oder Alias (`git@github-kunde:…`) |
| **Server** | `ssh`, `scp`, `sftp`, `rsync` auf einen Host oder eine IP | `Host`-Block mit Alias und Hostname |
| **Standard** | Git überall dort, wo kein anderer Key gilt | `core.sshCommand` in `index.gitconfig` |

Ein Key kann alles zugleich sein – z. B. „Privat“ als Standard, mit GitHub und deinem Heimserver. Neuer Key: Name
eingeben, „Key anlegen“ – wo er gilt, legst du danach auf seiner Seite fest. Bei jeder Verwendung gibt es „Testen“
(mit einer verständlichen Schritt-für-Schritt-Erklärung, was ssh gemacht hat) und, wo es geht, „Key eintragen“.
Schlägt ein Test fehl, weil der Server den Key nicht kennt, steht der passende Knopf gleich daneben („Pubkey kopieren
& GitHub-Seite öffnen“, „Key auf nas eintragen“). Gespeichert wird in `~/.config/keybund/profiles.toml`.

## Key bei GitHub, GitLab und Bitbucket eintragen

Keybund verwaltet die Keys auf deinem Rechner und meldet sich nirgends an. „Key eintragen“ beim Git-Konto eines Keys
(oder `keybund install-key`) kopiert den Pubkey in die Zwischenablage und öffnet die Seite, auf der du ihn einfügst –
bei GitHub, GitLab (auch selbst gehostet) und Bitbucket. Dort „New SSH key“ wählen, einfügen, speichern; danach
„Erledigt – jetzt testen“. Achte darauf, im Browser mit dem richtigen Konto angemeldet zu sein: Der Test zeigt, als
wer du angemeldet wurdest.

Auf **Servern** trägt Keybund den Key selbst ein, wie `ssh-copy-id` – der Server fragt dafür einmal nach dem Passwort.

## Host-Keys

Keybund vertraut keinem Server stillschweigend. Die veröffentlichten Host-Keys von GitHub, GitLab.com und
Bitbucket sind eingebaut und werden ohne Rückfrage nach `~/.ssh/known_hosts` geschrieben. Bei jedem anderen Host
zeigt Keybund vor der ersten Verbindung den Fingerprint (App: Dialog, CLI: Rückfrage oder `keybund trust`), und
erst nach der Bestätigung verbindet sich ssh – mit `StrictHostKeyChecking=yes`.

Die Host-Keys der Server eines Keys landen in einer eigenen Datei `~/.ssh/known_hosts.d/<key>` (der
`Host`-Block bekommt `UserKnownHostsFile ~/.ssh/known_hosts.d/<key> ~/.ssh/known_hosts`) und wandern beim
Archivieren mit nach `~/.ssh/archive/`. Abschaltbar in den Einstellungen bzw. mit
`keybund config known-hosts-per-profile false`.

## Dolphin (sftp://)

Dolphin/KIO liest die ssh-Config über libssh. libssh vor 0.10.2 (z. B. Ubuntu 22.04) versteht `~` in `Include`
nicht und sähe Keybunds Host-Blöcke nicht. Der Doctor erkennt das an der installierten libssh und schreibt auf
Wunsch das `Include` mit vollem Pfad (Einstellung „Include mit vollem Pfad“, `keybund config absolute-ssh-include true`).

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
| `~/.config/keybund/git/index.gitconfig` | `core.sshCommand` des Standard-Keys, ein `includeIf "gitdir:…/"` pro Ordner | `[include]` in `~/.gitconfig` |
| `~/.config/keybund/git/<id>.gitconfig` | `user.name`, `user.email`, `core.sshCommand = ssh -i <key> -o IdentitiesOnly=yes -o AddKeysToAgent=yes`, Signing | `index.gitconfig` |
| `~/.config/keybund/allowed_signers` | E-Mail + Pubkey je signierendem Key | `gpg.ssh.allowedSignersFile` |

## Umstieg mit bestehender `~/.ssh/config`

Hast du schon Einträge wie `Host github.com` mit `IdentityFile`, übernimmt Keybund sie: in der App unter
**Bestehende SSH-Config übernehmen** (die Startseite weist darauf hin) oder mit `keybund import`.

In der App ist das ein Klick („2 Keys übernehmen“), der alle drei Schritte macht:

1. Aus jedem `Host`-Block mit eigenem Key wird ein Key in Keybund – die Key-Dateien bleiben, wo sie sind.
   Ein `Host github.com` wird zum Standard-Key, Aliase wie `Host pflegeabc` bleiben Aliase.
2. Keybund schreibt seine Configs.
3. Die übernommenen Blöcke werden aus `~/.ssh/config` entfernt (vorher mit Backup; abwählbar). Das ist wichtig: Solange
   dort ein `IdentityFile` für github.com steht, bietet ssh diesen Key zusätzlich zum Key aus Keybund an, und eine Anmeldung kann
   mit dem falschen Key gelingen. Der Verbindungstest prüft deshalb, welcher Key tatsächlich akzeptiert wurde, und der
   Doctor meldet solche Einträge.

## CLI

```text
keybund list                     Keys mit Fingerprint, Git-Ordnern, Hosts
keybund show <key>               Details und Pubkey
keybund edit <key> …             Name, Git-Ordner, Hosts, Git-Identität, Signieren, Key-Datei ändern (siehe --help)
keybund new                      Neuen Key anlegen, Fragen im Terminal (oder alles über Optionen, siehe --help)
keybund copy <key> [--print]     Pubkey in die Zwischenablage (wl-copy, xclip oder xsel)
keybund test <key> [--host h]    Verbindungstest, Exit-Code 0 bei Erfolg
keybund install-key <key> [--host h]
                                 Pubkey eintragen: Server wie ssh-copy-id; GitHub/GitLab/Bitbucket:
                                 Pubkey kopieren und die Seite zum Einfügen öffnen
keybund trust <key> [--host h]   Host-Keys prüfen und bestätigen (Fingerprint)
keybund config [key] [wert]      Einstellungen: projects-dir, keys-dir, known-hosts-per-profile, absolute-ssh-include
keybund which [pfad|host]        Welcher Key wird hier benutzt – und warum?
keybund clone <url> [--key id]   Klont in den Git-Ordner des passenden Keys
keybund apply [--dry-run] [-y]   Diff zeigen bzw. schreiben
keybund doctor [--fix]           Alle Checks, Exit-Code ≠ 0 bei Problemen
keybund keys                     Vorhandene Keys in ~/.ssh
keybund import [--all] [-y]      Bestehende ~/.ssh/config übernehmen, alte Einträge danach entfernen
keybund uninstall [--restore-backups] [--purge]   Alles entfernen, was Keybund geschrieben hat (Keys bleiben)
keybund default <key>            Standard-Key setzen
keybund archive <key>            Key nach ~/.ssh/archive, Configs entfernen
```

Beispiel ohne Rückfragen:

```sh
keybund new --name "Kunde C" --provider github --folder ~/dev/kunde-c \
  --git-name Tristan --git-email tristan@kunde-c.de --yes
```

## GUI

Seitenleiste mit deinen Keys (und wofür sie gelten) und einer Suche, die auch beantwortet, welcher Key für einen
Ordner oder Server gilt (Enter). Die Startseite zeigt, was zu tun ist – fehlgeschlagene Tests, Hinweise vom Doctor,
unbenutzte Keys – oder „Alles in Ordnung“, dazu eine kurze Erklärung, wie Keybund funktioniert. „Neuer Key“ ist ein
einziges kurzes Formular, die Key-Seite zeigt „Wo wird dieser Key benutzt?“ (Git-Ordner, Git-Konten, Server, Standard –
jeweils mit Testen und Key eintragen), Git-Identität und generierte Config. Änderungen gelten sofort. Dazu Doctor,
Clone-Helfer, Übernehmen einer bestehenden `~/.ssh/config` und Einstellungen (Spezielles unter „Erweitert“).

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
| Snapshot | `render` ist rein funktional, jede Key-Kombination hat eine Golden-File (insta) | `crates/core/src/snapshots/` |
| Integration (Rust) | `apply` gegen ein temporäres HOME, Auflösung mit echtem `git` und `ssh -G`, Doctor, Archiv | `crates/core/tests/` |
| Integration (sshd) | Echter `sshd` auf localhost: Passphrase einmal, danach hält der Agent den Key (Test und `git`), falscher Key erkannt, Key auf Server eintragen | `crates/core/tests/sshd.rs` |
| Integration (CLI) | Jeder Befehl gegen ein temporäres HOME, Fake-`ssh`/`xdg-open` | `crates/cli/tests/cli.rs` |
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
- [x] Key eintragen: Pubkey kopieren und Seite öffnen (GitHub, GitLab, Bitbucket), auf Servern wie ssh-copy-id
- [x] Host-Keys bestätigen, known_hosts pro Key, letzter Test bleibt gespeichert, Ordner einstellbar
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
