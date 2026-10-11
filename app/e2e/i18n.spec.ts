import { readFileSync } from "node:fs";
import type { Page } from "@playwright/test";
import { expect, test } from "./fixtures";

// A browser in English gets the app in English (the system language decides).
// Every page, tab, dialog and connection test is opened and searched for
// German that was left behind – UI texts as well as messages from the backend.
test.use({ locale: "en-US" });

/** Words that only occur in the German texts, never in the English ones. */
function germanOnlyWords(): Set<string> {
  const words = (file: string) =>
    (
      JSON.stringify(Object.values(JSON.parse(readFileSync(file, "utf8"))))
        .replace(/\{\w+\}/g, " ")
        .match(/[A-Za-zÄÖÜäöüß]{4,}/g) ?? []
    ).map((w) => w.toLowerCase());
  const en = new Set(words("messages/en.json"));
  return new Set(words("messages/de.json").filter((w) => !en.has(w)));
}
const GERMAN_ONLY = germanOnlyWords();

/** The demo data is German on purpose (names and comments of the "user"); paths and code are not text. */
const DATA =
  /\b(privat|acme|kunde|altprojekt|nebenjob|nordlicht|tristan|zentrale|meinserver)\b|\S*[~/=@]\S*|\S*\w[._]\w\S*/gi;
/** The language picker names each language in itself. */
const ALLOWED = new Set(["Deutsch"]);

async function germanLeftovers(page: Page): Promise<string[]> {
  const text = await page.locator("body").innerText();
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => !ALLOWED.has(line))
    .filter((line) => {
      const clean = line.replace(DATA, " ");
      return (
        /[äöüÄÖÜß]/.test(clean) ||
        (clean.match(/[A-Za-zÄÖÜäöüß]{4,}/g) ?? []).some((w) => GERMAN_ONLY.has(w.toLowerCase()))
      );
    });
}

test("the app follows an English system and leaves no German behind", async ({ app }) => {
  const leftovers: string[] = [];
  const check = async (where: string) => {
    for (const line of await germanLeftovers(app)) leftovers.push(`${where}: ${line}`);
  };

  await expect(app.getByRole("heading", { level: 1, name: "Overview" })).toBeVisible();
  await expect(app.locator("html")).toHaveAttribute("lang", "en");
  await check("overview");

  // A key page: test a git account and a server (backend messages and steps).
  await app
    .getByRole("navigation", { name: "Keys" })
    .getByRole("link", { name: /Privat/ })
    .click();
  await expect(app.getByRole("heading", { name: "Where is this key used?" })).toBeVisible();
  const uses = app.getByRole("list", { name: "Uses" });
  await uses.getByRole("listitem").filter({ hasText: "GitHub account" }).getByRole("button", { name: "Test" }).click();
  await expect(app.getByTestId("connection-status").first()).toHaveText("Connected as @tristan");
  await app.getByText("Details: what happened during the test").first().click();
  await app.getByText("Show public key, fingerprint and file").click();
  await check("key page");
  await app.getByRole("tab", { name: "Generated config" }).click();
  await check("generated config");

  // A failing test with its fix, and the host key dialog of a new server.
  await app
    .getByRole("navigation", { name: "Keys" })
    .getByRole("link", { name: /Kunde B/ })
    .click();
  await app.getByRole("list", { name: "Uses" }).getByRole("button", { name: "Test" }).first().click();
  await expect(app.getByTestId("connection-status")).toHaveText("GitHub does not know this key");
  await check("failed test");
  await app.getByRole("button", { name: "Server" }).click();
  await app.getByLabel("Hostname or IP").fill("10.0.0.7");
  await app.getByRole("button", { name: "Save server" }).click();
  await app.getByRole("listitem").filter({ hasText: "Server 10.0.0.7" }).getByRole("button", { name: "Test" }).click();
  await expect(app.getByRole("alertdialog")).toBeVisible();
  await check("host key dialog");
  await app.getByRole("button", { name: "Cancel" }).click();

  for (const [name, heading] of [
    ["New key", "New SSH key"],
    ["Clone repository", "Clone repository"],
    ["Doctor", "Doctor"],
    ["Settings", "Settings"],
  ] as const) {
    await app.getByRole("link", { name, exact: false }).first().click();
    await expect(app.getByRole("heading", { level: 1, name: heading })).toBeVisible();
    await check(name);
  }
  await app.getByText("Advanced").click();
  await check("settings, advanced");

  await app.goto("/#/import");
  await expect(app.getByRole("heading", { level: 1, name: "Take over existing SSH config" })).toBeVisible();
  await check("import");
  await app.goto("/#/which?q=~/dev/acme/app");
  await expect(app.getByTestId("which-result")).toBeVisible();
  await check("which");
  await app.goto("/#/uninstall");
  await expect(app.getByRole("heading", { level: 1, name: "Remove Keybund" })).toBeVisible();
  await check("uninstall");

  expect(leftovers).toEqual([]);
});

test("the language can be switched in the settings", async ({ app }) => {
  await app.getByRole("link", { name: "Settings" }).click();
  await app.getByLabel("Language of the app").selectOption("de");
  await expect(app.getByRole("heading", { level: 1, name: "Einstellungen" })).toBeVisible();
  // Remembered for the next start.
  await app.reload();
  await expect(app.getByRole("heading", { level: 1, name: "Einstellungen" })).toBeVisible();
  await app.getByLabel("Sprache der App").selectOption("");
  await expect(app.getByRole("heading", { level: 1, name: "Settings" })).toBeVisible();
});
