import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { m } from "@/paraglide/messages";
import { renderApp } from "@/test/render";
import { currentLang, setLang, storedLang, systemLang, t } from "./i18n";
import { msg } from "./mock/backend";
import { formatAge, formatDate, formatDay, formatRelative } from "./utils";

afterEach(() => setLang("de"));

describe("language", () => {
  it("follows the system: German for de-*, English for everything else", () => {
    expect(systemLang(["de-DE", "en-US"])).toBe("de");
    expect(systemLang(["de-AT"])).toBe("de");
    expect(systemLang(["en-GB", "de-DE"])).toBe("en");
    expect(systemLang(["fr-FR"])).toBe("en");
    expect(systemLang([])).toBe("en");
  });

  it("switches and remembers the choice, or follows the system again", () => {
    setLang("en");
    expect(currentLang()).toBe("en");
    expect(storedLang()).toBe("en");
    expect(document.documentElement.lang).toBe("en");
    expect(m.common_save()).toBe("Save");
    setLang(null);
    expect(storedLang()).toBeNull();
    expect(currentLang()).toBe(systemLang());
    setLang("de");
    expect(m.common_save()).toBe("Speichern");
  });

  it("renders backend messages with nested messages and plain arguments", () => {
    const invalid = msg("error_invalidValue", { value: "a b", field: msg("field_alias") });
    expect(t(invalid)).toBe("Ungültiger Wert für Alias: „a b“");
    setLang("en");
    expect(t(invalid)).toBe("Invalid value for alias: “a b”");
    expect(t(msg("probe_connected", { account: "@octocat" }))).toBe("Connected as @octocat");
    expect(t(msg("no_such_key"))).toBe("no_such_key");
  });

  it("formats dates, ages and plurals per language", () => {
    const now = new Date(2026, 9, 10, 12, 0);
    expect(formatDate("2026-03-03")).toBe("03.03.2026");
    expect(formatAge(400)).toBe("vor 1 Jahr");
    expect(formatRelative(now.getTime() - 120_000, now.getTime())).toBe("vor 2 Min.");
    expect(formatDay(new Date(2026, 9, 10, 9, 41).getTime(), now)).toBe("heute, 09:41");
    expect(m.sidebar_hints({ n: 1 })).toBe("1 Hinweis");
    expect(m.sidebar_hints({ n: 3 })).toBe("3 Hinweise");
    setLang("en");
    expect(formatDate("2026-03-03")).toBe("03/03/2026");
    expect(formatAge(400)).toBe("1 year ago");
    expect(formatAge(0)).toBe("today");
    expect(formatRelative(now.getTime() - 120_000, now.getTime())).toBe("2 min ago");
    expect(formatDay(new Date(2026, 9, 9, 18, 2).getTime(), now)).toBe("yesterday, 18:02");
    expect(m.sidebar_hints({ n: 1 })).toBe("1 hint");
    expect(m.sidebar_hints({ n: 3 })).toBe("3 hints");
  });

  it("switches the whole app from the settings, without a reload", async () => {
    const { user } = await renderApp("/settings");
    await user.selectOptions(await screen.findByLabelText("Sprache der App"), "en");
    expect(await screen.findByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /New key/ })).toBeInTheDocument();
    // Messages from the backend (doctor checks) follow too.
    await user.click(screen.getByRole("link", { name: /Doctor/ }));
    expect(await screen.findByText("Configs are up to date")).toBeInTheDocument();
    await user.click(screen.getByRole("link", { name: "Settings" }));
    await user.selectOptions(await screen.findByLabelText("Language of the app"), "de");
    expect(await screen.findByRole("heading", { level: 1, name: "Einstellungen" })).toBeInTheDocument();
  });
});

const de = JSON.parse(readFileSync("messages/de.json", "utf8")) as Record<string, unknown>;
const en = JSON.parse(readFileSync("messages/en.json", "utf8")) as Record<string, unknown>;
const keys = Object.keys(de).filter((k) => k !== "$schema");

/** Placeholders a message uses, over all variants. */
const placeholders = (v: unknown) => new Set([...JSON.stringify(v).matchAll(/\{(\w+)\}/g)].map((x) => x[1]));

describe("messages/*.json", () => {
  it("has every message in both languages, none empty, with the same placeholders", () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(de).sort());
    const empty = keys.filter((k) => [de[k], en[k]].some((v) => typeof v === "string" && !v.trim()));
    expect(empty).toEqual([]);
    const differ = keys.filter(
      (k) => JSON.stringify([...placeholders(de[k])].sort()) !== JSON.stringify([...placeholders(en[k])].sort()),
    );
    expect(differ).toEqual([]);
  });

  it("every message is used (frontend or Rust), every used key exists", () => {
    const files = (dir: string, ext: RegExp): string[] =>
      readdirSync(dir, { withFileTypes: true }).flatMap((e): string[] =>
        e.isDirectory()
          ? e.name === "paraglide" || e.name === "target"
            ? []
            : files(join(dir, e.name), ext)
          : ext.test(e.name)
            ? [join(dir, e.name)]
            : [],
      );
    const ts = files("src", /\.tsx?$/).filter((f) => !f.endsWith(".test.ts") && !f.endsWith(".test.tsx"));
    const rust = [...files("../crates", /\.rs$/), ...files("src-tauri/src", /\.rs$/)];
    const tsCode = ts.map((f) => readFileSync(f, "utf8")).join("\n");
    const rustCode = rust.map((f) => readFileSync(f, "utf8")).join("\n");
    const used = new Set([
      ...[...tsCode.matchAll(/\bm\.(\w+)\b/g)].map((x) => x[1]),
      // Keys named in data: msg("key", …) in the mock, `field_${…}` in Rust.
      ...[...tsCode.matchAll(/\bmsg\(\s*"(\w+)"/g)].map((x) => x[1]),
      ...[...rustCode.matchAll(/msg!\(\s*"(\w+)"/g)].map((x) => x[1]),
    ]);
    const dynamic = (k: string) => k.startsWith("field_");
    expect(keys.filter((k) => !used.has(k) && !dynamic(k))).toEqual([]);
    const tsKeys = [...tsCode.matchAll(/\bm\.([a-z][A-Za-z0-9]*_\w+)/g)].map((x) => x[1]);
    expect([...new Set(tsKeys)].filter((k) => !(k in de))).toEqual([]);
  });
});
