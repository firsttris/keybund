import { defineConfig, devices } from "@playwright/test";

// E2E tests run the real frontend in Chromium against the in-memory mock
// backend (`--mode mock`), which implements every Tauri command.
const executablePath = process.env.PW_CHROMIUM_PATH || undefined;

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: "http://localhost:1420",
    trace: "retain-on-failure",
    launchOptions: { executablePath },
  },
  projects: [
    { name: "desktop", use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 820 } } },
    { name: "narrow", use: { ...devices["Desktop Chrome"], viewport: { width: 760, height: 700 } } },
  ],
  webServer: {
    command: "pnpm dev:mock",
    url: "http://localhost:1420",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
