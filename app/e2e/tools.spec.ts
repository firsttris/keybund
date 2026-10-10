import { expect, test } from "./fixtures";

test("resolves a folder and a host", async ({ app }) => {
  await app.getByRole("link", { name: "Welcher Key?" }).click();
  await app.getByLabel("Ordner oder Host").fill("~/dev/sparkasse/portal-frontend");
  await app.getByRole("button", { name: "Auflösen" }).click();
  const result = app.getByTestId("which-result");
  await expect(result).toContainText("sparkasse_ed25519");
  await expect(result).toContainText('includeIf "gitdir:~/dev/sparkasse/"');
  await expect(result).toContainText("Tristan <tristan@kunde.de>");
  await expect(result).toContainText("ja, nur dieser Key wird angeboten");
  await expect(result.getByRole("link", { name: "Profil Sparkasse" })).toBeVisible();

  await app.getByRole("button", { name: "meinserver" }).click();
  await expect(result).toContainText("privat_ed25519");
  await expect(result).toContainText("tristan@1.2.3.4:22");

  await app.getByLabel("Ordner oder Host").fill("github.com");
  await app.getByRole("button", { name: "Auflösen" }).click();
  await expect(result).toContainText("Für Git-Repos entscheidet der Ordner");
});

test("doctor fixes the include position", async ({ app }) => {
  await expect(app.getByRole("alert")).toContainText("Include steht nicht ganz oben");
  await expect(app.getByLabel("1 Hinweise")).toBeVisible();
  await app.getByRole("link", { name: "Zum Doctor" }).click();
  await app.getByRole("button", { name: "Include steht nicht ganz oben beheben" }).click();
  await expect(app.getByText("Include steht ganz oben in ~/.ssh/config")).toBeVisible();
  await expect(app.getByText("Alles in Ordnung")).toBeVisible();
  await expect(app.getByLabel("1 Hinweise")).toHaveCount(0);
});

test("clones into the matching profile", async ({ app }) => {
  await app.getByRole("link", { name: "Repo klonen" }).click();
  await app.getByLabel("Repo-URL").fill("git@gitlab.kunde.de:team/portal-frontend.git");
  await app.getByRole("button", { name: "Prüfen" }).click();
  await expect(app.getByRole("radio", { name: /Sparkasse/ })).toBeChecked();
  await expect(app.getByLabel("Zielordner")).toHaveValue("~/dev/sparkasse/portal-frontend");
  await app.getByRole("button", { name: "Klonen" }).click();
  await expect(app.getByRole("status")).toContainText("Geklont nach ~/dev/sparkasse/portal-frontend");

  // Same target again fails.
  await app.getByRole("button", { name: "Klonen" }).click();
  await expect(app.getByText(/Zielordner existiert bereits/)).toBeVisible();
});

test("clone asks when several profiles share a host", async ({ app }) => {
  await app.goto("/#/clone");
  await app.getByLabel("Repo-URL").fill("git@github.com:firsttris/keybund.git");
  await app.getByRole("button", { name: "Prüfen" }).click();
  await expect(app.getByRole("radio", { name: /Privat/ })).toBeChecked();
  await app.getByRole("radio", { name: /Kunde B/ }).check();
  await expect(app.getByLabel("Zielordner")).toHaveValue("~/dev/kunde-b/keybund");

  await app.getByLabel("Repo-URL").fill("kaputt");
  await app.getByRole("button", { name: "Prüfen" }).click();
  await expect(app.getByText("Ungültige Repo-URL: kaputt")).toBeVisible();
});

test("changes the default profile", async ({ app }) => {
  await app.goto("/#/settings");
  await expect(app.getByLabel("Key außerhalb aller Projektordner")).toHaveValue("privat");
  await app.getByLabel("Key außerhalb aller Projektordner").selectOption("sparkasse");
  await app.goto("/#/");
  await expect(app.getByText("1 Datei ist nicht aktuell")).toBeVisible();
  await expect(app.getByRole("link", { name: /Sparkasse.*Default/ })).toBeVisible();
});

test("lists existing keys with their usage", async ({ app }) => {
  await app.goto("/#/settings");
  await expect(app.getByText("~/.ssh/id_ed25519", { exact: true })).toBeVisible();
  await expect(app.getByRole("link", { name: "genutzt von Sparkasse" })).toBeVisible();
  await expect(app.getByText("~/.config/keybund/profiles.toml")).toBeVisible();
});

test("layout has no horizontal scrolling", async ({ app }) => {
  for (const route of ["/", "/profile/privat", "/new", "/which", "/clone", "/apply", "/settings", "/doctor"]) {
    await app.goto(`/#${route}`);
    await app.waitForLoadState("networkidle");
    const overflow = await app.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    expect(overflow, route).toBeLessThanOrEqual(0);
  }
});
