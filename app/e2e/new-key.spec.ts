import type { Page } from "@playwright/test";
import { expect, mockState, pagePosition, test } from "./fixtures";

const usage = (app: Page) => app.getByRole("region", { name: "Wo wird dieser Key benutzt?" });
const row = (app: Page, text: string | RegExp) =>
  usage(app).getByRole("list", { name: "Verwendungen" }).getByRole("listitem").filter({ hasText: text }).first();

test("creates a key and sets it up for a GitHub folder", async ({ app }) => {
  await app.getByRole("link", { name: "Neuer Key" }).click();
  await expect(app.getByRole("heading", { level: 1, name: "Neuer SSH-Key" })).toBeVisible();
  await app.getByRole("button", { name: /Key anlegen/ }).click();
  await expect(app.getByRole("alert")).toHaveText("Bitte einen Namen eingeben.");

  await app.getByLabel("Name").fill("Kunde C");
  await expect(app.getByText("ed25519 · ~/.ssh/kunde-c_ed25519")).toBeVisible();
  await app.getByLabel(/Passphrase/).fill("geheimnis");
  await app.getByRole("button", { name: /Key anlegen/ }).click();

  // The key exists; now say what it is for.
  await expect(app.getByRole("heading", { level: 1, name: "Kunde C" })).toBeVisible();
  await expect(app.getByRole("navigation", { name: "Keys" }).getByRole("link", { name: /Kunde C.*noch nicht benutzt/ })).toBeVisible();
  await usage(app).getByRole("button", { name: /^Git-Ordner/ }).click();
  const form = app.getByRole("form", { name: "Git-Ordner hinzufügen" });
  await expect(form.getByLabel("Ordner", { exact: true })).toHaveAttribute("placeholder", "~/dev/kunde-c/");
  await expect(form.getByLabel("Name in Commits")).toHaveValue("Tristan Teufel");
  await form.getByLabel("E-Mail in Commits").fill("tristan@kunde-c.de");
  await form.getByRole("button", { name: "Ordner speichern" }).click();

  await expect(row(app, "~/dev/kunde-c/")).toBeVisible();
  const github = row(app, "GitHub-Konto");
  await github.getByRole("button", { name: "Key eintragen" }).click();
  await expect(github.getByText("Pubkey ist kopiert.")).toBeVisible();
  expect(await mockState<string[]>(app, "openedUrls")).toEqual(["https://github.com/settings/ssh/new"]);
  expect(await mockState<string>(app, "clipboard")).toMatch(/^ssh-ed25519 /);
  await github.getByRole("button", { name: "Erledigt – jetzt testen" }).click();
  await expect(github.getByTestId("connection-status")).toHaveText("Verbunden als @tristan");
  await expect(app.getByRole("navigation", { name: "Keys" }).getByRole("link", { name: /Kunde C.*~\/dev\/kunde-c\/ · GitHub/ })).toBeVisible();
});

test("takes an existing key for a server", async ({ app }) => {
  await app.getByRole("link", { name: "Einstellungen" }).click();
  await app.getByRole("listitem").filter({ hasText: /^~\/\.ssh\/id_ed25519/ }).getByRole("link", { name: "Als Key übernehmen" }).click();
  await expect(app.getByRole("radio", { name: /Vorhandenen Key nehmen/ })).toHaveAttribute("aria-checked", "true");
  await app.getByLabel("Name").fill("NAS");
  await app.getByRole("button", { name: /Key anlegen/ }).click();

  await expect(app.getByRole("heading", { level: 1, name: "NAS" })).toBeVisible();
  await usage(app).getByRole("button", { name: /^Server/ }).click();
  const form = app.getByRole("form", { name: "Server hinzufügen" });
  await form.getByLabel("Hostname oder IP").fill("192.168.1.20");
  await form.getByLabel(/Alias/).fill("backup");
  await form.getByLabel("Benutzer").fill("root");
  await form.getByRole("button", { name: "Server speichern" }).click();

  const server = row(app, "Server backup");
  await expect(server).toContainText("ssh backup verbindet zu root@192.168.1.20");
  await server.getByRole("button", { name: "Key eintragen" }).click();
  await app.getByRole("alertdialog").getByRole("button", { name: "Fingerprint stimmt" }).click();
  await expect(server.getByText("Key auf backup eingetragen")).toBeVisible();
  await server.getByRole("button", { name: "Testen" }).click();
  await expect(server.getByTestId("connection-status")).toHaveText("Anmeldung erfolgreich");
  expect(await mockState<string[]>(app, "openedUrls")).toEqual([]);
});

test("rejects a name that already exists", async ({ app }) => {
  await app.goto("/#/new");
  await app.getByLabel("Name").fill("Sparkasse");
  await expect(app.getByText("ed25519 · ~/.ssh/sparkasse_ed25519")).toBeVisible();
  await app.getByRole("button", { name: /Key anlegen/ }).click();
  await expect(app.getByRole("alert")).toHaveText("Einen Key „Sparkasse“ gibt es schon.");
});

test("the validation message does not move the button", async ({ app }) => {
  await app.goto("/#/new");
  const button = app.getByRole("button", { name: /Key anlegen/ });
  const before = await pagePosition(button);
  await button.click();
  await expect(app.getByRole("alert")).toBeVisible();
  expect(await pagePosition(button)).toEqual(before);
});
