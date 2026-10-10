import { test as base, expect, type Locator, type Page } from "@playwright/test";

/** Reads the in-memory backend's state from the page. */
export async function mockState<T>(page: Page, pick: string): Promise<T> {
  return page.evaluate((p) => {
    const state = (window as unknown as { __keybundMock: { state: Record<string, unknown> } }).__keybundMock.state;
    return state[p] as unknown;
  }, pick) as Promise<T>;
}

/** Position relative to the document, independent of scrolling. */
export async function pagePosition(locator: Locator) {
  return locator.evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { x: r.left + window.scrollX, y: r.top + window.scrollY, width: r.width, height: r.height };
  });
}

export const test = base.extend<{ app: Page }>({
  app: async ({ page }, use) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.goto("/#/");
    await expect(page.getByRole("navigation", { name: "Profile" })).toBeVisible();
    await use(page);
    expect(errors, "uncaught errors in the page").toEqual([]);
  },
});

export { expect };
