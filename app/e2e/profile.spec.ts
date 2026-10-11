import type { Page } from "@playwright/test";
import { expect, mockState, pagePosition, test } from "./fixtures";

const usage = (app: Page) => app.getByRole("region", { name: "Wo wird dieser Key benutzt?" });
const row = (app: Page, text: string | RegExp) =>
  usage(app).getByRole("list", { name: "Verwendungen" }).getByRole("listitem").filter({ hasText: text }).first();

test("navigates to a key and shows where it is used", async ({ app }) => {
  await app.getByRole("navigation", { name: "Keys" }).getByRole("link", { name: /Acme/ }).click();
  await expect(app.getByRole("heading", { level: 1, name: "Acme" })).toBeVisible();
  await expect(app.getByRole("navigation", { name: "Keys" }).getByRole("link", { name: /Acme/ })).toHaveAttribute("aria-current", "page");
  await expect(app.getByText("~/.ssh/acme_ed25519", { exact: true }).first()).toBeVisible();
  await expect(row(app, "GitLab-Konto")).toContainText("gitlab.kunde.de");
  await expect(row(app, "~/dev/acme/")).toBeVisible();
});

test("copies the public key", async ({ app }) => {
  await app.goto("/#/profile/acme");
  await app.getByRole("button", { name: "Pubkey kopieren" }).first().click();
  await expect(app.getByRole("button", { name: "Kopiert" })).toBeVisible();
  expect(await mockState<string>(app, "clipboard")).toMatch(/^ssh-ed25519 \S+ tristan@zentrale-acme$/);
});

test("connection test shows success and failure with the steps", async ({ app }) => {
  await app.goto("/#/profile/privat");
  const github = row(app, "GitHub-Konto");
  await github.getByRole("button", { name: "Testen" }).click();
  await expect(github.getByRole("status", { name: "Teste Verbindung" })).toBeVisible();
  await expect(github.getByTestId("connection-status")).toHaveText("Verbunden als @tristan");

  await app.goto("/#/profile/kunde-b");
  const failing = row(app, "GitHub-Konto");
  await failing.getByRole("button", { name: "Testen" }).click();
  await expect(failing.getByTestId("connection-status")).toHaveText("GitHub kennt diesen Key nicht");
  await expect(failing.getByRole("list", { name: "Ablauf" })).toContainText("Anmelden als Benutzer „git“");
  await expect(failing.getByText(/Permission denied \(publickey\)/).first()).toBeVisible();
  // The sidebar reflects the failed test.
  await expect(app.getByRole("navigation", { name: "Keys" }).getByText("Test fehlgeschlagen")).toBeVisible();

  // The fix is one click away: copy the pubkey, paste it on GitHub, test again.
  await failing.getByRole("button", { name: "Pubkey kopieren & GitHub-Seite öffnen" }).click();
  await expect(failing.getByText("Pubkey ist kopiert.")).toBeVisible();
  expect(await mockState<string[]>(app, "openedUrls")).toEqual(["https://github.com/settings/ssh/new"]);
  await app.evaluate(() => {
    (window as unknown as { __keybundMock: { state: { accountKeys: string[] } } }).__keybundMock.state.accountKeys.push("github.com:~/.ssh/kunde-b_ed25519");
  });
  await failing.getByRole("button", { name: "Erledigt – jetzt testen" }).click();
  await expect(failing.getByTestId("connection-status")).toHaveText("Verbunden als @tristan");
  await expect(app.getByRole("navigation", { name: "Keys" }).getByText("Test fehlgeschlagen")).toHaveCount(0);
});

test("opens the provider page", async ({ app }) => {
  await app.goto("/#/profile/acme");
  const accounts = row(app, "GitLab-Konto");
  await accounts.getByRole("button", { name: "Key eintragen" }).click();
  await expect(accounts.getByText("Pubkey ist kopiert.")).toBeVisible();
  await expect.poll(() => mockState<string[]>(app, "openedUrls")).toEqual(["https://gitlab.kunde.de/-/user_settings/ssh_keys"]);
  expect(await mockState<string>(app, "clipboard")).toMatch(/tristan@zentrale-acme$/);
});

test("shows the generated config", async ({ app }) => {
  await app.goto("/#/profile/privat");
  await app.getByRole("tab", { name: "Generierte Config" }).click();
  await expect(app.getByText("~/.config/keybund/git/privat.gitconfig", { exact: true })).toBeVisible();
  await expect(app.getByText(/sshCommand = ssh -i ~\/.ssh\/privat_ed25519 -o IdentitiesOnly=yes -o AddKeysToAgent=yes/)).toBeVisible();
  await expect(app.getByText(/Host meinserver/)).toBeVisible();
});

test("a change is written right away", async ({ app }) => {
  await app.goto("/#/profile/acme");
  await expect(app.getByText("Configs aktuell")).toBeVisible();
  await app.getByRole("switch", { name: "Commits signieren" }).click();
  await expect(app.getByRole("switch", { name: "Commits signieren" })).toHaveAttribute("aria-checked", "true");
  await expect(app.getByText("Git und ssh benutzen es ab sofort.")).toBeVisible();
  await expect(app.getByText("Configs aktuell")).toBeVisible();
  await app.getByRole("tab", { name: "Generierte Config" }).click();
  await expect(app.getByText(/gpgsign = true/).first()).toBeVisible();
});

test("a file changed outside Keybund can be written again", async ({ app }) => {
  await app.evaluate(() => {
    const mock = (window as unknown as { __keybundMock: { state: { applied: Map<string, string> } } }).__keybundMock;
    mock.state.applied.set("~/.config/keybund/git/acme.gitconfig", "kaputt\n");
  });
  // The plan is not cached yet, so the page shows the difference right away.
  await app.goto("/#/apply");
  await expect(app.getByRole("heading", { name: "Configs neu schreiben" })).toBeVisible();
  await expect(app.getByText("-kaputt")).toBeVisible();
  await app.getByRole("button", { name: "Neu schreiben" }).click();
  await expect(app.getByText("Alle Configs passen zu deinen Keys.")).toBeVisible();
});

test("adds and removes a folder, rejects nested folders", async ({ app }) => {
  await app.goto("/#/profile/kunde-b");
  await usage(app).getByRole("button", { name: "Git-Ordner" }).click();
  await app.getByLabel("Ordner", { exact: true }).fill("~/dev/acme/sub");
  await app.getByRole("button", { name: "Ordner speichern" }).click();
  await expect(app.getByText(/Git-Ordner überschneiden sich/).first()).toBeVisible();

  await app.getByLabel("Ordner", { exact: true }).fill("~/work/kunde-b");
  await app.getByRole("button", { name: "Ordner speichern" }).click();
  await expect(app.getByText("~/work/kunde-b/", { exact: true })).toBeVisible();
  await app.getByRole("button", { name: "Ordner ~/work/kunde-b/ entfernen" }).click();
  await expect(app.getByText("~/work/kunde-b/", { exact: true })).toHaveCount(0);
});

test("adds a server and installs the key", async ({ app }) => {
  await app.goto("/#/profile/kunde-b");
  await usage(app).getByRole("button", { name: "Server" }).click();
  const form = app.getByRole("form", { name: "Server hinzufügen" });
  await form.getByLabel("Hostname oder IP").fill("10.0.0.7");
  await form.getByLabel(/Alias/).fill("kundenserver");
  await form.getByRole("button", { name: "Server speichern" }).click();
  const card = row(app, "Server kundenserver");
  await card.getByRole("button", { name: "Key eintragen" }).click();
  const dialog = app.getByRole("alertdialog");
  await expect(dialog).toContainText("Erste Verbindung zu 10.0.0.7");
  await dialog.getByRole("button", { name: "Fingerprint stimmt" }).click();
  await expect(card.getByText("Key auf kundenserver eingetragen")).toBeVisible();
  await expect(app.getByRole("navigation", { name: "Keys" }).getByRole("link", { name: /Kunde B.*kundenserver/ })).toBeVisible();
});

test("adds a git account", async ({ app }) => {
  await app.goto("/#/profile/kunde-b");
  await usage(app).getByRole("button", { name: "Git-Konto" }).click();
  await app.getByLabel("Anbieter").selectOption("gitlab");
  await app.getByRole("button", { name: "Konto speichern" }).click();
  await expect(row(app, "GitLab-Konto")).toContainText("gitlab.com");
  await expect(app.getByRole("navigation", { name: "Keys" }).getByRole("link", { name: /Kunde B.*GitHub · GitLab/ })).toBeVisible();
});

test("archives a profile and removes its configs", async ({ app }) => {
  await app.goto("/#/profile/kunde-b");
  await app.getByRole("button", { name: "Archivieren" }).click();
  await expect(app.getByRole("alertdialog")).toContainText("„Kunde B“ archivieren?");
  await app.getByRole("alertdialog").getByRole("button", { name: "Archivieren" }).click();

  await expect(app.getByRole("heading", { name: "Übersicht" })).toBeVisible();
  await expect(app.getByText("Configs aktuell")).toBeVisible();
  expect(await app.evaluate(() =>
    (window as unknown as { __keybundMock: { state: { applied: Map<string, string> } } }).__keybundMock.state.applied.has(
      "~/.config/keybund/git/kunde-b.gitconfig",
    ),
  )).toBe(false);

  await expect(app.getByRole("navigation", { name: "Keys" }).getByText("Kunde B")).toHaveCount(0);
  await expect(app.getByRole("navigation", { name: "Archiv" }).getByText("Kunde B")).toBeVisible();
});

test("sidebar search finds keys by name, folder and server", async ({ app }) => {
  await app.getByLabel("Keys durchsuchen").fill("acm");
  const nav = app.getByRole("navigation", { name: "Keys" });
  const keys = nav.getByRole("link").filter({ hasNotText: "Welcher Key" });
  await expect(keys).toHaveCount(1);
  await app.getByLabel("Keys durchsuchen").fill("1.2.3");
  await expect(keys).toHaveCount(1);
  await expect(nav.getByRole("link", { name: /Privat/ })).toBeVisible();
  await app.getByLabel("Keys durchsuchen").fill("xyz");
  await expect(nav.getByText("Kein Key passt")).toBeVisible();
  await expect(nav.getByRole("link", { name: "Welcher Key gilt für „xyz“?" })).toBeVisible();
});

test("unknown key shows a not-found page", async ({ app }) => {
  await app.goto("/#/profile/gibtsnicht");
  await expect(app.getByRole("heading", { name: "Key nicht gefunden" })).toBeVisible();
});

test("the e-mail warning does not move the signing switch", async ({ app }) => {
  await app.goto("/#/profile/acme");
  const sign = app.getByRole("switch", { name: "Commits signieren" });
  const before = await pagePosition(sign);
  await app.getByLabel("E-Mail").fill("kaputt");
  await expect(app.getByRole("alert")).toHaveText("Bitte eine gültige E-Mail-Adresse eingeben.");
  expect(await pagePosition(sign)).toEqual(before);
  await expect(app.getByRole("button", { name: "Identität speichern" })).toBeDisabled();
});

test("a failing browser start is explained, not swallowed", async ({ app }) => {
  await app.goto("/#/profile/acme");
  await app.evaluate(() => {
    (window as unknown as { __keybundMock: { state: { openFails: boolean } } }).__keybundMock.state.openFails = true;
  });
  const accounts = row(app, "GitLab-Konto");
  await accounts.getByRole("button", { name: "Key eintragen" }).click();
  await expect(app.getByText("Browser konnte nicht geöffnet werden")).toBeVisible();
  await expect(app.getByRole("button", { name: "Link kopieren" })).toBeVisible();
});
