# OAuth-Apps für den Key-Upload registrieren

Keybund trägt Public Keys per API bei GitHub und GitLab ein. Angemeldet wird mit dem **OAuth Device Flow**: Keybund
zeigt einen kurzen Code, du bestätigst ihn im Browser (dort bist du ohnehin angemeldet), und Keybund bekommt ein Token
mit genau der Berechtigung für SSH-Keys. Ein Passwort sieht Keybund nie, und das Token wird nach dem Eintragen
verworfen, außer du lässt es im Schlüsselbund speichern.

Dafür braucht Keybund je eine registrierte OAuth-App auf github.com und gitlab.com. Die **Client-ID** ist dabei
kein Geheimnis: Device-Flow-Apps sind „öffentliche Clients“, die ID steckt in jedem Binary. Ein Client-Secret wird
nicht gebraucht und darf auch nicht ins Repo.

Ohne registrierte App funktioniert alles weiter, nur eben mit einem persönlichen Zugriffstoken statt der Anmeldung
im Browser. Selbst gehostete GitLab-Instanzen und GitHub Enterprise nutzen immer ein Token, weil die App nur auf
github.com bzw. gitlab.com registriert ist.

## GitHub

1. **Settings → Developer settings → OAuth Apps → New OAuth App** öffnen
   (<https://github.com/settings/applications/new>, für eine Organisation unter deren Settings).
2. Ausfüllen:
   - Application name: `Keybund`
   - Homepage URL: `https://github.com/firsttris/keybund`
   - Authorization callback URL: `https://github.com/firsttris/keybund` (Pflichtfeld, beim Device Flow ungenutzt)
3. **Register application**, danach auf der App-Seite **Enable Device Flow** anhaken und speichern.
4. Die **Client ID** kopieren. Kein Client-Secret erzeugen.

Keybund fragt die Scopes `write:public_key` an, bei Profilen mit Commit-Signierung zusätzlich
`admin:ssh_signing_key`.

## GitLab (gitlab.com)

1. **Edit profile → Applications → Add new application** öffnen (<https://gitlab.com/-/user_settings/applications>,
   für eine Gruppe unter deren Settings → Applications).
2. Ausfüllen:
   - Name: `Keybund`
   - Redirect URI: `https://github.com/firsttris/keybund` (Pflichtfeld, beim Device Flow ungenutzt)
   - **Confidential: nicht anhaken.** Nur nicht-vertrauliche Apps dürfen den Device Flow ohne Secret nutzen.
   - Scopes: `api`
3. **Save application** und die **Application ID** kopieren.

GitLab hat keinen eigenen Scope nur für SSH-Keys; `api` ist der kleinste, mit dem `POST /user/keys` geht. Deshalb
speichert Keybund Tokens nur, wenn du das ausdrücklich willst. Der Device Flow braucht GitLab 17.2 oder neuer
(gitlab.com hat ihn).

## Client-IDs eintragen

Im Repo unter **Settings → Secrets and variables → Actions → Variables** zwei *Repository variables* anlegen
(Variables, nicht Secrets):

| Variable | Wert |
| --- | --- |
| `KEYBUND_GITHUB_CLIENT_ID` | Client ID der GitHub-App |
| `KEYBUND_GITLAB_CLIENT_ID` | Application ID der GitLab-App |

`release.yml` übergibt sie beim Bauen als Umgebungsvariablen, Keybund übernimmt sie zur Compile-Zeit. Ab dem nächsten
Release zeigen CLI und App für github.com und gitlab.com „Bei GitHub/GitLab anmelden“.

## Lokal ausprobieren

Dieselben Variablen gelten auch zur Laufzeit und überschreiben die eingebauten IDs:

```sh
KEYBUND_GITHUB_CLIENT_ID=Ov23li… keybund install-key privat
KEYBUND_GITHUB_CLIENT_ID=Ov23li… pnpm tauri dev    # im Ordner app/
```

Ohne Device Flow, mit Token:

```sh
keybund install-key sparkasse --token-stdin < token.txt
keybund install-key sparkasse --token-stdin --remember   # Token im Schlüsselbund (secret-tool) merken
```
