import { describe, expect, it } from "vitest";
import type { Profile } from "./bindings";
import { colorHex, formatAge, formatDate, formatRelative, hostLabel, hostSummary, isValidEmail, plural, slugify } from "./utils";

describe("slugify", () => {
  // Same cases as store::tests::slugs in the Rust core.
  it.each([
    ["Kunde C", "kunde-c"],
    ["  Bundesagentur für Arbeit ", "bundesagentur-fuer-arbeit"],
    ["S-Communication", "s-communication"],
    ["Straße 42!", "strasse-42"],
    ["---", ""],
  ])("%s → %s", (input, expected) => expect(slugify(input)).toBe(expected));
});

describe("formatting", () => {
  it("formats dates", () => {
    expect(formatDate("2026-03-03")).toBe("03.03.2026");
    expect(formatDate(null)).toBe("–");
  });

  it.each([
    [0, "heute"],
    [1, "gestern"],
    [10, "vor 10 Tagen"],
    [31, "vor 1 Monat"],
    [221, "vor 7 Monaten"],
    [400, "vor 1 Jahr"],
    [1300, "vor 3 Jahren"],
  ])("age %i days → %s", (days, expected) => expect(formatAge(days)).toBe(expected));

  it("formats relative times", () => {
    const now = 1_000_000_000;
    expect(formatRelative(now - 10_000, now)).toBe("gerade eben");
    expect(formatRelative(now - 120_000, now)).toBe("vor 2 Min.");
    expect(formatRelative(now - 3 * 3_600_000, now)).toBe("vor 3 Std.");
    expect(formatRelative(now - 3 * 86_400_000, now)).toBe("vor 3 Tagen");
    expect(formatRelative(null)).toBe("");
  });

  it("pluralizes", () => {
    expect(plural(1, "Datei", "Dateien")).toBe("1 Datei");
    expect(plural(3, "Datei", "Dateien")).toBe("3 Dateien");
  });
});

describe("hosts and colors", () => {
  const base: Profile = {
    id: "p",
    name: "P",
    color: "blue",
    key: { path: "~/.ssh/p", comment: null },
    git: null,
    folders: [],
    archived: false,
    hosts: [],
  };

  it("summarizes hosts", () => {
    expect(hostSummary(base)).toBe("");
    const gh = { alias: null, hostname: "github.com", user: null, port: null, provider: "github" as const };
    expect(hostSummary({ ...base, hosts: [gh] })).toBe("GitHub");
    expect(hostSummary({ ...base, hosts: [gh, gh] })).toBe("2 Hosts");
  });

  it("labels hosts", () => {
    expect(hostLabel({ alias: "srv", hostname: "1.2.3.4", user: "t", port: 2222, provider: "generic" })).toBe(
      "Server · 1.2.3.4 · User t · Port 2222",
    );
    expect(hostLabel({ alias: null, hostname: "gitlab.kunde.de", user: null, port: null, provider: "gitlab" })).toBe("GitLab · Port 22");
  });

  it("maps colors with a fallback", () => {
    expect(colorHex("blue")).toBe("#3b82f6");
    expect(colorHex("unknown")).toBe(colorHex("zinc"));
    expect(colorHex(null)).toBe(colorHex("zinc"));
  });

  it("validates emails", () => {
    expect(isValidEmail("t@kunde.de")).toBe(true);
    expect(isValidEmail(" t@kunde.de ")).toBe(true);
    expect(isValidEmail("kein-mail")).toBe(false);
    expect(isValidEmail("a@b")).toBe(false);
  });
});
