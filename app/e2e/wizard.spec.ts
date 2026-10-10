import type { Page } from "@playwright/test";
import { expect, mockState, pagePosition, test } from "./fixtures";

/** Opens the wizard and picks the Git flow. */
async function gitWizard(app: Page) {
  await app.goto("/#/new");
  await app.getByRole("radio", { name: /^Git-Projekt/ }).click();
  await app.getByRole("button", { name: "Weiter" }).click();
}

test("creates a GitHub project end to end", async ({ app }) => {
  await app.getByRole("link", { name: "Neu", exact: true }).click();
  await expect(app.getByRole("heading", { name: "Wofür ist der Key?" })).toBeVisible();
  await app.getByRole("button", { name: "Weiter" }).click();
  await expect(app.getByRole("alert")).toHaveText("Bitte wählen, wofür der Key ist.");
  await app.getByRole("radio", { name: /^Git-Projekt/ }).click();
  await app.getByRole("button", { name: "Weiter" }).click();
  await expect(app.getByText("Schritt 2 von 7")).toBeVisible();

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

  // Key and preview of the diff.
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

  // Login in the browser, the key goes into the account, then the test runs.
  await expect(app.getByRole("heading", { name: "Key eintragen & testen" })).toBeVisible();
  expect(await mockState<string[]>(app, "openedUrls")).toEqual([]);
  expect(await mockState<string>(app, "clipboard")).toMatch(/^ssh-ed25519 .* tristan@zentrale-kunde-c$/);
  await app.getByRole("button", { name: "Bei GitHub anmelden & eintragen" }).click();
  await expect(app.getByTestId("user-code")).toHaveText("WDJB-MJHT");
  await expect(app.getByText("Warte auf Bestätigung im Browser …")).toBeVisible();
  expect(await mockState<string[]>(app, "openedUrls")).toEqual(["https://github.com/login/device"]);
  await expect(app.getByText("Key bei GitHub eingetragen (@tristan)")).toBeVisible({ timeout: 10_000 });
  await expect(app.getByText("Verbunden als @tristan")).toBeVisible();
  await app.getByRole("link", { name: "Zum Profil" }).click();

  await expect(app.getByRole("heading", { level: 1, name: "Kunde C" })).toBeVisible();
  await expect(app.getByRole("navigation", { name: "Profile" }).getByRole("link", { name: /Kunde C/ })).toBeVisible();
  await expect(app.getByText("Configs aktuell")).toBeVisible();
});

test("creates a server with an existing key from settings", async ({ app }) => {
  await app.getByRole("link", { name: "Einstellungen" }).click();
  await app.getByRole("listitem").filter({ hasText: /^~\/\.ssh\/id_ed25519/ }).getByRole("link", { name: "Als Profil anlegen" }).click();

  await app.getByRole("radio", { name: /^Server/ }).click();
  await app.getByRole("button", { name: "Weiter" }).click();
  await expect(app.getByText("Schritt 2 von 4")).toBeVisible();
  await app.getByLabel("Hostname oder IP").fill("192.168.1.20");
  await app.getByLabel(/SSH-Alias/).fill("backup");
  await app.getByLabel("Benutzer").fill("root");
  await app.getByRole("button", { name: "Weiter" }).click();

  await expect(app.getByRole("heading", { name: "SSH-Key für backup" })).toBeVisible();
  await expect(app.getByRole("radio", { name: /Vorhandenen Key verwenden/ })).toHaveAttribute("aria-checked", "true");
  await expect(app.getByRole("radio", { name: /^~\/.ssh\/id_ed25519 SHA256/ })).toBeChecked();
  await expect(app.getByText("+Host backup 192.168.1.20")).toBeVisible();
  await expect(app.getByText("+    IdentityFile ~/.ssh/id_ed25519")).toBeVisible();
  await app.getByRole("button", { name: "Anwenden", exact: true }).click();

  await expect(app.getByRole("heading", { name: "Key auf dem Server eintragen" })).toBeVisible();
  await app.getByRole("button", { name: "Key auf backup eintragen" }).click();
  await app.getByRole("alertdialog").getByRole("button", { name: "Fingerprint stimmt" }).click();
  await expect(app.getByText("Key auf backup eingetragen")).toBeVisible();
  await app.getByRole("button", { name: "Verbindung testen" }).click();
  await expect(app.getByText("Anmeldung erfolgreich")).toBeVisible();
  expect(await mockState<string[]>(app, "openedUrls")).toEqual([]);

  // The server shows up in its own sidebar group, not as a project.
  await app.getByRole("link", { name: "Zum Profil" }).click();
  await expect(app.getByRole("heading", { level: 1, name: "backup" })).toBeVisible();
  await expect(app.getByRole("region", { name: "Git-Identität" })).toHaveCount(0);
  const sidebarServers = app.getByRole("navigation", { name: "Profile" }).getByRole("group", { name: "Server" });
  await expect(sidebarServers.getByRole("link", { name: /backup/ })).toBeVisible();
  await expect(app.getByRole("group", { name: "Projekte & Konten" }).getByText("backup")).toHaveCount(0);
});

test("adds a server to the private profile", async ({ app }) => {
  await app.goto("/#/new");
  await app.getByRole("radio", { name: /^Server/ }).click();
  await app.getByRole("button", { name: "Weiter" }).click();
  await app.getByLabel("Hostname oder IP").fill("10.9.9.9");
  await app.getByLabel(/SSH-Alias/).fill("pi");
  await app.getByRole("button", { name: "Weiter" }).click();
  await app.getByRole("radio", { name: /Key eines Profils verwenden/ }).click();
  await app.getByLabel("Profil", { exact: true }).selectOption("privat");
  await expect(app.getByText("+Host pi 10.9.9.9")).toBeVisible();
  await app.getByRole("button", { name: "Anwenden", exact: true }).click();
  await app.getByRole("button", { name: "Key auf pi eintragen" }).click();
  await app.getByRole("alertdialog").getByRole("button", { name: "Fingerprint stimmt" }).click();
  await expect(app.getByText("Key auf pi eingetragen")).toBeVisible();
  await app.getByRole("link", { name: "Zum Profil" }).click();

  await expect(app.getByRole("heading", { level: 1, name: "Privat" })).toBeVisible();
  await expect(app.getByRole("region", { name: "Server" }).getByText("pi", { exact: true })).toBeVisible();
  await expect(app.getByText(/Gilt für/)).toContainText("Server meinserver, pi");
});

test("rejects a project name that already exists", async ({ app }) => {
  await gitWizard(app);
  await app.getByLabel("Projektname").fill("Sparkasse");
  await expect(app.getByText("Profil-ID: sparkasse")).toBeVisible();
  await app.getByRole("button", { name: "Weiter" }).click();
  await expect(app.getByRole("alert")).toHaveText("Ein Profil „sparkasse“ gibt es schon.");
});

test("shows an error when folders overlap", async ({ app }) => {
  await gitWizard(app);
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
  await gitWizard(app);
  await app.getByLabel("Projektname").fill("Kunde D");
  await app.getByRole("button", { name: "Weiter" }).click();
  await app.getByRole("radio", { name: "GitLab" }).click();
  await app.getByLabel("Hostname").fill("gitlab.kunde-d.de");
  await app.getByRole("button", { name: "Zurück" }).click();
  await expect(app.getByLabel("Projektname")).toHaveValue("Kunde D");
  await app.getByRole("button", { name: "Weiter" }).click();
  await expect(app.getByLabel("Hostname")).toHaveValue("gitlab.kunde-d.de");
});

test("typing the project name does not move the form", async ({ app }) => {
  await gitWizard(app);
  const color = app.getByText("Farbe", { exact: true });
  const before = await pagePosition(color);
  await app.getByLabel("Projektname").fill("Kunde C");
  await expect(app.getByText("Profil-ID: kunde-c")).toBeVisible();
  expect(await pagePosition(color)).toEqual(before);
  await app.getByLabel("Projektname").fill("");
  expect(await pagePosition(color)).toEqual(before);
});

test("validation messages do not move the buttons", async ({ app }) => {
  await gitWizard(app);
  const next = app.getByRole("button", { name: "Weiter" });
  const before = await pagePosition(next);
  await next.click();
  await expect(app.getByRole("alert")).toHaveText("Bitte einen Projektnamen eingeben.");
  expect(await pagePosition(next)).toEqual(before);

  await app.getByLabel("Projektname").fill("Kunde X");
  await next.click();
  await expect(app.getByRole("alert")).toHaveCount(0);
  const onHost = await pagePosition(next);
  await app.getByLabel("Hostname").fill("");
  await next.click();
  await expect(app.getByRole("alert")).toHaveText("Bitte einen Hostnamen eingeben.");
  expect(await pagePosition(next)).toEqual(onHost);
});

test("renaming the project updates the folder", async ({ app }) => {
  await gitWizard(app);
  await app.getByLabel("Projektname").fill("Kunde C");
  await app.getByRole("button", { name: "Weiter" }).click();
  await app.getByRole("button", { name: "Weiter" }).click();
  await expect(app.getByLabel("Ordner")).toHaveValue("~/dev/kunde-c/");
  await app.getByRole("button", { name: "Zurück" }).click();
  await app.getByRole("button", { name: "Zurück" }).click();
  await app.getByLabel("Projektname").fill("Kunde E");
  await app.getByRole("button", { name: "Weiter" }).click();
  await app.getByRole("button", { name: "Weiter" }).click();
  await expect(app.getByLabel("Ordner")).toHaveValue("~/dev/kunde-e/");
});
