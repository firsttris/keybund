import { expect, mockState, test } from "./fixtures";

test("creates a GitHub project end to end", async ({ app }) => {
  await app.getByRole("link", { name: "Neues Projekt" }).click();
  await expect(app.getByText("Schritt 1 von 6")).toBeVisible();

  // Validation: name is required.
  await app.getByRole("button", { name: "Weiter" }).click();
  await expect(app.getByRole("alert")).toHaveText("Bitte einen Projektnamen eingeben.");

  await app.getByLabel("Projektname").fill("Kunde C");
  await expect(app.getByText("Profil-ID: kunde-c")).toBeVisible();
  await app.getByRole("button", { name: "Weiter" }).click();

  await expect(app.getByRole("radio", { name: "GitHub" })).toHaveAttribute("aria-checked", "true");
  await expect(app.getByLabel("Hostname")).toHaveValue("github.com");
  await app.getByRole("button", { name: "Weiter" }).click();

  await expect(app.getByLabel("Ordner")).toHaveValue("~/dev/kunde-c/");
  await app.getByRole("button", { name: "Weiter" }).click();

  await app.getByRole("button", { name: "Weiter" }).click();
  await expect(app.getByRole("alert")).toHaveText("Bitte einen Git-Namen eingeben.");
  await app.getByLabel("Name").fill("Tristan");
  await app.getByLabel("E-Mail").fill("kein-mail");
  await app.getByRole("button", { name: "Weiter" }).click();
  await expect(app.getByRole("alert")).toHaveText("Bitte eine gültige E-Mail-Adresse eingeben.");
  await app.getByLabel("E-Mail").fill("tristan@kunde-c.de");
  await app.getByRole("button", { name: "Weiter" }).click();

  // Step 5: key and preview of the diff.
  await expect(app.getByRole("heading", { name: "SSH-Key für Kunde C" })).toBeVisible();
  await expect(app.getByText("ed25519 · ~/.ssh/kunde-c_ed25519")).toBeVisible();
  await expect(app.getByLabel("Kommentar")).toHaveValue("tristan@zentrale-kunde-c");
  await app.getByLabel("Passphrase (optional)").fill("geheimnis");
  await expect(app.getByText('+[includeIf "gitdir:~/dev/kunde-c/"]')).toBeVisible();
  await expect(app.getByText("+\temail = tristan@kunde-c.de")).toBeVisible();
  await expect(app.getByText("Vorschau der Änderungen")).toBeVisible();

  // The completed steps show their values.
  const steps = app.getByRole("list", { name: "Schritte" });
  await expect(steps).toContainText("Kunde C");
  await expect(steps).toContainText("~/dev/kunde-c/");

  await app.getByRole("button", { name: "Anwenden & Pubkey kopieren" }).click();

  // Step 6: clipboard, provider page, connection test.
  await expect(app.getByRole("heading", { name: "Key eintragen & testen" })).toBeVisible();
  expect(await mockState<string[]>(app, "openedUrls")).toEqual(["https://github.com/settings/ssh/new"]);
  expect(await mockState<string>(app, "clipboard")).toMatch(/^ssh-ed25519 .* tristan@zentrale-kunde-c$/);
  await app.getByRole("button", { name: "Ich hab ihn eingetragen" }).click();
  await expect(app.getByText("Verbunden als @tristan")).toBeVisible();
  await app.getByRole("link", { name: "Zum Profil" }).click();

  await expect(app.getByRole("heading", { level: 1, name: "Kunde C" })).toBeVisible();
  await expect(app.getByRole("navigation", { name: "Profile" }).getByRole("link", { name: /Kunde C/ })).toBeVisible();
  await expect(app.getByText("Configs aktuell")).toBeVisible();
});

test("creates a server with an existing key from settings", async ({ app }) => {
  await app.getByRole("link", { name: "Einstellungen" }).click();
  await app.getByRole("link", { name: "Als Projekt anlegen" }).click();

  await app.getByLabel("Projektname").fill("Backup Server");
  await app.getByRole("button", { name: "Weiter" }).click();
  await app.getByRole("radio", { name: "Server" }).click();
  await app.getByLabel("Hostname").fill("192.168.1.20");
  await app.getByRole("button", { name: "Weiter" }).click();
  await expect(app.getByRole("alert")).toHaveText("Server brauchen einen Alias, z. B. meinserver.");
  await app.getByLabel("SSH-Alias").fill("backup");
  await app.getByLabel("Benutzer").fill("root");
  await app.getByRole("button", { name: "Weiter" }).click();
  // Folder and Git identity are optional for servers.
  await app.getByLabel("Ordner").fill("");
  await app.getByRole("button", { name: "Weiter" }).click();
  await app.getByRole("button", { name: "Weiter" }).click();

  await expect(app.getByRole("radio", { name: /Vorhandenen Key verwenden/ })).toHaveAttribute("aria-checked", "true");
  await expect(app.getByRole("radio", { name: /^~\/.ssh\/id_ed25519 SHA256/ })).toBeChecked();
  await expect(app.getByText("+Host backup")).toBeVisible();
  await expect(app.getByText("+    IdentityFile ~/.ssh/id_ed25519")).toBeVisible();
  await app.getByRole("button", { name: "Anwenden & Pubkey kopieren" }).click();

  await expect(app.getByText(/ssh-copy-id/)).toBeVisible();
  await app.getByRole("button", { name: "Ich hab ihn eingetragen" }).click();
  await expect(app.getByText("Anmeldung erfolgreich")).toBeVisible();
  expect(await mockState<string[]>(app, "openedUrls")).toEqual([]);
});

test("rejects a project name that already exists", async ({ app }) => {
  await app.goto("/#/new");
  await app.getByLabel("Projektname").fill("Sparkasse");
  await expect(app.getByText("Profil-ID: sparkasse")).toBeVisible();
  await app.getByRole("button", { name: "Weiter" }).click();
  await expect(app.getByRole("alert")).toHaveText("Ein Profil „sparkasse“ gibt es schon.");
});

test("shows an error when folders overlap", async ({ app }) => {
  await app.goto("/#/new");
  await app.getByLabel("Projektname").fill("Unterprojekt");
  await app.getByRole("button", { name: "Weiter" }).click();
  await app.getByRole("button", { name: "Weiter" }).click();
  await app.getByLabel("Ordner").fill("~/dev/privat/unter");
  await app.getByRole("button", { name: "Weiter" }).click();
  await app.getByLabel("Name").fill("T");
  await app.getByLabel("E-Mail").fill("t@x.de");
  await app.getByRole("button", { name: "Weiter" }).click();
  await expect(app.getByText(/Projektordner überschneiden sich/)).toBeVisible();
  await expect(app.getByRole("button", { name: "Anwenden & Pubkey kopieren" })).toBeDisabled();
});

test("back keeps entered values", async ({ app }) => {
  await app.goto("/#/new");
  await app.getByLabel("Projektname").fill("Kunde D");
  await app.getByRole("button", { name: "Weiter" }).click();
  await app.getByRole("radio", { name: "GitLab" }).click();
  await app.getByLabel("Hostname").fill("gitlab.kunde-d.de");
  await app.getByRole("button", { name: "Zurück" }).click();
  await expect(app.getByLabel("Projektname")).toHaveValue("Kunde D");
  await app.getByRole("button", { name: "Weiter" }).click();
  await expect(app.getByLabel("Hostname")).toHaveValue("gitlab.kunde-d.de");
});
