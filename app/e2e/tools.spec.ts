import { expect, test } from "./fixtures";

test("resolves a folder and a host", async ({ app }) => {
  // Straight from the search: Enter asks which key applies.
  await app.getByLabel("Keys durchsuchen").fill("~/dev/acme/portal-frontend");
  await app.getByLabel("Keys durchsuchen").press("Enter");
  await expect(app.getByRole("heading", { level: 1, name: "Welcher Key?" })).toBeVisible();
  const result = app.getByTestId("which-result");
  await expect(result).toContainText("acme_ed25519");
  await expect(result).toContainText('includeIf "gitdir:~/dev/acme/"');
  await expect(result).toContainText("Tristan <tristan@kunde.de>");
  await expect(result).toContainText("ja, nur dieser Key wird angeboten");
  await expect(result.getByRole("link", { name: "Key Acme" })).toBeVisible();

  await app.getByRole("button", { name: "meinserver" }).click();
  await expect(result).toContainText("privat_ed25519");
  await expect(result).toContainText("tristan@1.2.3.4:22");

  await app.getByLabel("Ordner oder Host").fill("github.com");
  await app.getByRole("button", { name: "Auflösen" }).click();
  await expect(result).toContainText("Für Git-Repos entscheidet der Ordner");
});

test("doctor fixes the include position", async ({ app }) => {
  const todo = app.getByRole("list", { name: "Zu tun" });
  await expect(todo).toContainText("Include steht nicht ganz oben");
  await expect(app.getByLabel("1 Hinweise")).toBeVisible();
  await todo.getByRole("link", { name: "Zum Doctor" }).click();
  await app.getByRole("button", { name: "Include steht nicht ganz oben beheben" }).click();
  await expect(app.getByText("Include steht ganz oben in ~/.ssh/config")).toBeVisible();
  await expect(app.getByText("Alles in Ordnung")).toBeVisible();
  await expect(app.getByLabel("1 Hinweise")).toHaveCount(0);
});

test("clones into the matching profile", async ({ app }) => {
  await app.getByRole("link", { name: "Repo klonen" }).click();
  await app.getByLabel("Repo-URL").fill("git@gitlab.kunde.de:team/portal-frontend.git");
  await app.getByRole("button", { name: "Prüfen" }).click();
  await expect(app.getByRole("radio", { name: /Acme/ })).toBeChecked();
  await expect(app.getByLabel("Zielordner")).toHaveValue("~/dev/acme/portal-frontend");
  await app.getByRole("button", { name: "Klonen" }).click();
  await expect(app.getByRole("status")).toContainText("Geklont nach ~/dev/acme/portal-frontend");

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
  await expect(app.getByLabel("Key für Repos außerhalb deiner Git-Ordner")).toHaveValue("privat");
  await app.getByLabel("Key für Repos außerhalb deiner Git-Ordner").selectOption("acme");
  await expect(app.getByText("Git und ssh benutzen es ab sofort.")).toBeVisible();
  await app.goto("/#/");
  await expect(app.getByText("Configs aktuell")).toBeVisible();
  await expect(app.getByRole("main").getByRole("list", { name: "Acme wird benutzt für" })).toContainText("Standard für Git");
});

test("lists existing keys with their usage", async ({ app }) => {
  await app.goto("/#/settings");
  await expect(app.getByText("~/.ssh/id_ed25519", { exact: true })).toBeVisible();
  await expect(app.getByRole("link", { name: "genutzt von Acme" })).toBeVisible();
  await app.getByText("Erweitert").click();
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

test("sidebar stays in place when switching pages", async ({ app }, testInfo) => {
  test.skip(testInfo.project.name === "narrow", "the sidebar sits above the content on narrow screens");
  const doctor = app.getByRole("link", { name: /^Doctor/ });
  await app.goto("/#/profile/acme");
  await expect(app.getByRole("heading", { level: 1, name: "Acme" })).toBeVisible();
  const start = await doctor.boundingBox();
  for (const [link, heading] of [
    [/^Doctor/, "Doctor"],
    [/Repo klonen/, "Repo klonen"],
    [/Einstellungen/, "Einstellungen"],
  ] as const) {
    await app.getByRole("link", { name: link }).click();
    await expect(app.getByRole("heading", { level: 1, name: heading })).toBeVisible();
    expect(await doctor.boundingBox(), String(link)).toEqual(start);
  }
});

test("takes over the existing ~/.ssh/config", async ({ app }) => {
  await app.getByRole("link", { name: "Übernehmen" }).click();
  await expect(app.getByRole("heading", { name: "Bestehende SSH-Config übernehmen" })).toBeVisible();
  await expect(app.getByRole("region", { name: "Nebenjob Account" })).toContainText("~/.ssh/nebenjob/id_ed25519");
  await app.getByRole("button", { name: "2 Keys übernehmen" }).click();
  await expect(app.getByRole("status")).toContainText("Alles übernommen: 2 Keys, alte Einträge entfernt");
  await app.getByText("Geänderte Dateien").click();
  await expect(app.getByText("Backup: ~/.ssh/config.keybund-backup")).toBeVisible();
  await expect(app.getByRole("navigation", { name: "Keys" }).getByRole("link", { name: /Nebenjob Account/ })).toBeVisible();
  // The banner is gone.
  await app.goto("/#/");
  await expect(app.getByText(/mit eigenem Key, die Keybund/)).toHaveCount(0);
});

test("doctor removes an old entry after the import", async ({ app }) => {
  await app.goto("/#/import");
  await app.getByRole("checkbox", { name: /Alte Einträge danach/ }).uncheck();
  await app.getByRole("button", { name: "2 Keys übernehmen" }).click();
  await expect(app.getByRole("status")).toContainText("Alles übernommen: 2 Keys");
  await app.goto("/#/doctor");
  await expect(app.getByText("Doppelter Eintrag für nebenjob in ~/.ssh/config")).toBeVisible();
  await app.getByRole("button", { name: "Doppelter Eintrag für nebenjob in ~/.ssh/config beheben" }).click();
  await expect(app.getByText(/Doppelter Eintrag/)).toHaveCount(0);
});

test("removes Keybund after a preview", async ({ app }) => {
  await app.goto("/#/settings");
  await app.getByText("Erweitert").click();
  await app.getByRole("link", { name: "Entfernen …" }).click();
  await expect(app.getByText("-Include ~/.ssh/config.d/keybund.conf")).toBeVisible();
  await app.getByRole("button", { name: "Keybund entfernen" }).click();
  await app.getByRole("alertdialog").getByRole("button", { name: "Entfernen" }).click();
  await expect(app.getByRole("status")).toContainText("Keybund ist entfernt");
  // Configs are now missing, the profiles are still there to set it up again.
  await app.goto("/#/");
  await expect(app.getByText(/Config-Dateien passen nicht zu deinen Keys/)).toBeVisible();
});
