// Takes the pictures in docs/ (README, documentation, banner, social preview)
// from the demo data of the mock backend. scripts/screenshots.sh serves the
// build; by hand: node scripts/screenshots.mjs <docs dir> [base url]
// The pictures show the English UI (the browser's language decides). The clock
// is fixed, so an unchanged page gives an unchanged picture.
import { readFileSync } from "node:fs";
import { chromium } from "@playwright/test";

const OUT = process.argv[2] ?? "../docs";
const BASE = process.argv[3] ?? "http://127.0.0.1:4317";
const NOW = new Date("2026-06-15T09:42:00+02:00");

const browser = await chromium.launch({ executablePath: process.env.PW_CHROMIUM_PATH || undefined });

async function newPage(options = {}) {
  const ctx = await browser.newContext({
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 1.5,
    locale: "en-US",
    timezoneId: "Europe/Berlin",
    colorScheme: "dark",
    ...options,
  });
  const page = await ctx.newPage();
  await page.clock.setFixedTime(NOW);
  return page;
}

const shot = async (page, name, opts = {}) => {
  // Let fonts, transitions and the mock's latency settle.
  await page.waitForTimeout(opts.wait ?? 800);
  await page.screenshot({ path: `${OUT}/screenshot-${name}.png`, fullPage: !!opts.full });
  console.log("shot", name);
};

const page = await newPage();
let loads = 0;
const open = async (path) => {
  // A fresh load (new query) also gives a fresh mock backend with the demo data.
  await page.goto(`${BASE}/?load=${++loads}#${path}`);
  await page.getByRole("navigation", { name: "Keys" }).waitFor();
};

// A key and everything it is used for: the heart of the app.
await open("/profile/privat");
await page.getByRole("heading", { name: "Where is this key used?" }).waitFor();
await shot(page, "key");

// The connection test explains step by step what ssh did.

await page.getByRole("button", { name: "Test", exact: true }).first().click();
await page.getByText("Details: what happened during the test").first().waitFor();
await page.getByText("Details: what happened during the test").first().click();
await page
  .getByText("Try it yourself")
  .first()
  .evaluate((el) => el.scrollIntoView({ block: "center" }));
await shot(page, "test", { wait: 600 });

// The config Keybund writes for this key.
await open("/profile/privat");
await page.getByRole("tab", { name: "Generated config" }).click();
await page.evaluate(() => document.activeElement?.blur());
await shot(page, "config");

await open("/");
await page.getByRole("heading", { name: "Your SSH keys" }).waitFor();
await shot(page, "home");

await open("/new");
await page.getByLabel("Name").first().fill("Client C");
await page.evaluate(() => document.activeElement?.blur());
await shot(page, "new-key");

await open("/which?q=~/dev/acme/app");
await page.getByText("acme_ed25519").first().waitFor();
await shot(page, "which");

await open("/import");
await page.getByRole("button", { name: /Take over \d+ keys?/ }).waitFor();
await shot(page, "import");

await open("/doctor");
await page.getByRole("button", { name: "Check again" }).waitFor();
await shot(page, "doctor");

await open("/clone");
await page.getByLabel("Repository URL").fill("git@github.com:kunde-b/app.git");
await page.getByRole("button", { name: "Check", exact: true }).click();
await page.getByLabel("Target folder").waitFor();
await page.evaluate(() => document.activeElement?.blur());
await shot(page, "clone");

await open("/settings");
await shot(page, "settings");

// Banner and social preview: the key page under the name.
const art = await newPage({ viewport: { width: 1280, height: 640 }, deviceScaleFactor: 1 });
const key = readFileSync(`${OUT}/screenshot-key.png`).toString("base64");
const icon = readFileSync(new URL("../src-tauri/icons/128x128@2x.png", import.meta.url)).toString("base64");
const card = (width, height, social) => `<!doctype html><html><body style="margin:0;width:${width}px;height:${height}px;
  background:radial-gradient(circle at 20% 10%,#27272a,#09090b 60%);font-family:Geist,Inter,system-ui,sans-serif;color:#fafafa;
  display:flex;align-items:center;gap:56px;padding:0 64px;box-sizing:border-box;overflow:hidden">
  <div style="flex:0 0 auto;max-width:${social ? 500 : 640}px">
    <div style="display:flex;align-items:center;gap:18px">
      <img src="data:image/png;base64,${icon}" width="64" height="64" style="border-radius:14px">
      <span style="font-size:56px;font-weight:700;letter-spacing:-1px">Keybund</span>
    </div>
    <p style="font-size:28px;line-height:1.35;color:#d4d4d8;margin:26px 0 0">The right SSH key, automatically.</p>
    <p style="font-size:18px;line-height:1.55;color:#a1a1aa;margin:16px 0 0">SSH key manager for Linux · one key per GitHub/GitLab account,
      client and server · Git identity per folder · plain OpenSSH and Git config, no agent</p>
  </div>
  <img src="data:image/png;base64,${key}" style="height:${Math.round(height * 0.8)}px;border-radius:14px;border:1px solid #3f3f46;box-shadow:0 20px 60px rgba(0,0,0,.6)">
</body></html>`;
await art.setContent(card(1280, 640, true));
await art.waitForTimeout(300);
await art.screenshot({ path: `${OUT}/social-preview.png` });
console.log("shot social-preview");
await art.setViewportSize({ width: 1800, height: 560 });
await art.setContent(card(1800, 560, false));
await art.waitForTimeout(300);
await art.screenshot({ path: `${OUT}/banner.png` });
console.log("shot banner");

await browser.close();
