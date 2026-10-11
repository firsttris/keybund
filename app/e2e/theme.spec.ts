import { expect, test } from "./fixtures";

const background = (page: import("@playwright/test").Page) =>
  page.evaluate(() => getComputedStyle(document.body).backgroundColor);

test("follows the system's colour scheme and can be chosen in the settings", async ({ app }) => {
  const html = app.locator("html");
  await app.emulateMedia({ colorScheme: "light" });
  await expect(html).not.toHaveClass(/dark/);
  expect(await background(app)).toBe("rgb(255, 255, 255)");

  // The desktop switches to dark mode: the app follows without a reload.
  await app.emulateMedia({ colorScheme: "dark" });
  await expect(html).toHaveClass(/dark/);
  expect(await background(app)).toBe("rgb(9, 9, 11)");

  await app.getByRole("link", { name: /Einstellungen/ }).click();
  await app.getByLabel("Farbschema").selectOption("light");
  await expect(html).not.toHaveClass(/dark/);

  // Remembered, and already set before the app starts (public/theme.js): no dark flash.
  await app.reload();
  await expect(html).not.toHaveClass(/dark/);
  await app.getByLabel("Farbschema").selectOption("");
  await expect(html).toHaveClass(/dark/);
});

test("the colour scheme is set before the app renders", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  // Before any module script runs: the class comes from the classic script in <head>.
  await page.route("**/src/main.tsx", (route) => route.abort());
  await page.route("**/assets/*.js", (route) => route.abort());
  await page.goto("/#/");
  await expect(page.locator("html")).toHaveClass(/dark/);
});
