import { expect, mockState, pagePosition, test } from "./fixtures";

test("navigates to a profile and shows key, hosts and folders", async ({ app }) => {
  await app.getByRole("navigation", { name: "Profile" }).getByRole("link", { name: /Sparkasse/ }).click();
  await expect(app.getByRole("heading", { level: 1, name: "Sparkasse" })).toBeVisible();
  await expect(app.getByRole("link", { name: /Sparkasse/ })).toHaveAttribute("aria-current", "page");
  await expect(app.getByText("~/.ssh/sparkasse_ed25519", { exact: true })).toBeVisible();
  await expect(app.getByText("gitlab.kunde.de", { exact: true })).toBeVisible();
  await expect(app.getByRole("region", { name: "Projektordner" }).getByText("~/dev/sparkasse/", { exact: true })).toBeVisible();
  await expect(app.getByTestId("connection-status")).toHaveText("Verbunden als @tristan");
});

test("copies the public key", async ({ app }) => {
  await app.goto("/#/profile/sparkasse");
  await app.getByRole("button", { name: "Pubkey kopieren" }).first().click();
  await expect(app.getByRole("button", { name: "Kopiert" })).toBeVisible();
  expect(await mockState<string>(app, "clipboard")).toMatch(/^ssh-ed25519 \S+ tristan@zentrale-sparkasse$/);
});

test("connection test shows success and failure", async ({ app }) => {
  await app.goto("/#/profile/privat");
  await app.getByRole("button", { name: "Verbindung testen" }).click();
  await expect(app.getByRole("status", { name: "Teste Verbindung" })).toBeVisible();
  await expect(app.getByTestId("connection-status")).toHaveText("Verbunden als @tristan");

  await app.goto("/#/profile/kunde-b");
  await app.getByRole("button", { name: "Verbindung testen" }).click();
  await expect(app.getByTestId("connection-status")).toHaveText("GitHub kennt diesen Key nicht");
  await expect(app.getByText("Permission denied (publickey)")).toBeVisible();
  // The sidebar reflects the failed test.
  await expect(app.getByRole("navigation", { name: "Profile" }).getByText("Test fehlgeschlagen")).toBeVisible();
});

test("opens the provider page", async ({ app }) => {
  await app.goto("/#/profile/sparkasse");
  await app.getByRole("button", { name: "Bei GitLab hinzufügen" }).click();
  await expect.poll(() => mockState<string[]>(app, "openedUrls")).toEqual(["https://gitlab.kunde.de/-/user_settings/ssh_keys"]);
});

test("shows the generated config", async ({ app }) => {
  await app.goto("/#/profile/privat");
  await app.getByRole("tab", { name: "Generierte Config" }).click();
  await expect(app.getByText("~/.config/keybund/git/privat.gitconfig", { exact: true })).toBeVisible();
  await expect(app.getByText(/sshCommand = ssh -i ~\/.ssh\/privat_ed25519 -o IdentitiesOnly=yes -o AddKeysToAgent=yes/)).toBeVisible();
  await expect(app.getByText(/Host meinserver/)).toBeVisible();
});

test("editing a profile leads to the diff and apply", async ({ app }) => {
  await app.goto("/#/profile/sparkasse");
  await expect(app.getByText("Configs aktuell")).toBeVisible();
  await app.getByRole("switch", { name: "Commits signieren" }).click();
  await expect(app.getByRole("switch", { name: "Commits signieren" })).toHaveAttribute("aria-checked", "true");
  await expect(app.getByText("1 Datei ist nicht aktuell")).toBeVisible();

  await app.getByRole("link", { name: "Vorschau & anwenden" }).click();
  await expect(app.getByRole("heading", { name: "Änderungen anwenden" })).toBeVisible();
  await expect(app.getByText("+\tgpgsign = true").first()).toBeVisible();
  await app.getByRole("button", { name: "Anwenden" }).click();
  await expect(app.getByText("Alle Configs sind aktuell.")).toBeVisible();
  await expect(app.getByText(/Datei\(en\) geschrieben/)).toBeVisible();
});

test("adds and removes a folder, rejects nested folders", async ({ app }) => {
  await app.goto("/#/profile/kunde-b");
  await app.getByRole("button", { name: "+ Ordner" }).click();
  await app.getByLabel("Neuer Ordner").fill("~/dev/sparkasse/sub");
  await app.getByRole("button", { name: "Speichern" }).click();
  await expect(app.getByText(/Projektordner überschneiden sich/).first()).toBeVisible();

  await app.getByLabel("Neuer Ordner").fill("~/work/kunde-b");
  await app.getByRole("button", { name: "Speichern" }).click();
  await expect(app.getByText("~/work/kunde-b/", { exact: true })).toBeVisible();
  await app.getByRole("button", { name: "Ordner ~/work/kunde-b/ entfernen" }).click();
  await expect(app.getByText("~/work/kunde-b/", { exact: true })).toHaveCount(0);
});

test("adds a server host", async ({ app }) => {
  await app.goto("/#/profile/kunde-b");
  await app.getByRole("button", { name: "+ Host" }).click();
  const form = app.getByRole("form", { name: "Host hinzufügen" });
  await form.getByLabel("Provider").selectOption("generic");
  await form.getByLabel("Hostname").fill("10.0.0.7");
  await form.getByLabel("Alias").fill("kundenserver");
  await form.getByRole("button", { name: "Host speichern" }).click();
  await expect(app.getByText("kundenserver", { exact: true })).toBeVisible();
  await expect(app.getByRole("navigation", { name: "Profile" }).getByText("2 Hosts")).toHaveCount(2);
});

test("archives a profile and removes its configs", async ({ app }) => {
  await app.goto("/#/profile/kunde-b");
  await app.getByRole("button", { name: "Archivieren" }).click();
  await expect(app.getByRole("alertdialog")).toContainText("„Kunde B“ archivieren?");
  await app.getByRole("alertdialog").getByRole("button", { name: "Archivieren" }).click();

  await expect(app.getByRole("heading", { name: "Änderungen anwenden" })).toBeVisible();
  await expect(app.getByText("~/.config/keybund/git/kunde-b.gitconfig", { exact: true })).toBeVisible();
  await expect(app.getByText("· gelöscht")).toBeVisible();
  await app.getByRole("button", { name: "Anwenden" }).click();
  await expect(app.getByText("Alle Configs sind aktuell.")).toBeVisible();

  await expect(app.getByRole("navigation", { name: "Profile" }).getByText("Kunde B")).toHaveCount(0);
  await expect(app.getByRole("navigation", { name: "Archiv" }).getByText("Kunde B")).toBeVisible();
});

test("sidebar search filters profiles", async ({ app }) => {
  await app.getByLabel("Profile durchsuchen").fill("spar");
  const nav = app.getByRole("navigation", { name: "Profile" });
  await expect(nav.getByRole("link")).toHaveCount(1);
  await app.getByLabel("Profile durchsuchen").fill("xyz");
  await expect(nav.getByText("Kein Treffer")).toBeVisible();
});

test("unknown profile shows a not-found page", async ({ app }) => {
  await app.goto("/#/profile/gibtsnicht");
  await expect(app.getByRole("heading", { name: "Profil nicht gefunden" })).toBeVisible();
});

test("the e-mail warning does not move the signing switch", async ({ app }) => {
  await app.goto("/#/profile/sparkasse");
  const sign = app.getByRole("switch", { name: "Commits signieren" });
  const before = await pagePosition(sign);
  await app.getByLabel("E-Mail").fill("kaputt");
  await expect(app.getByRole("alert")).toHaveText("Bitte eine gültige E-Mail-Adresse eingeben.");
  expect(await pagePosition(sign)).toEqual(before);
  await expect(app.getByRole("button", { name: "Identität speichern" })).toBeDisabled();
});

test("a failing browser start is explained, not swallowed", async ({ app }) => {
  await app.goto("/#/profile/sparkasse");
  await app.evaluate(() => {
    (window as unknown as { __keybundMock: { state: { openFails: boolean } } }).__keybundMock.state.openFails = true;
  });
  await app.getByRole("button", { name: "Bei GitLab hinzufügen" }).click();
  await expect(app.getByText("Browser konnte nicht geöffnet werden")).toBeVisible();
  await expect(app.getByRole("button", { name: "Link kopieren" })).toBeVisible();
});
