import { describe, expect, it } from "vitest";
import { colorHex, formatAge, formatDate, formatDay, formatRelative, hostLabel, isValidEmail, plural, slugify } from "./utils";

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

  it("formats days", () => {
    const now = new Date(2026, 9, 10, 15, 0);
    expect(formatDay(new Date(2026, 9, 10, 9, 41).getTime(), now)).toBe("heute, 09:41");
    expect(formatDay(new Date(2026, 9, 9, 18, 2).getTime(), now)).toBe("gestern, 18:02");
    expect(formatDay(new Date(2026, 2, 3, 8, 0).getTime(), now)).toBe("03.03.2026");
  });
});
