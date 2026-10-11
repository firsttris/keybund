import { screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { renderApp } from "@/test/render";
import { appliedTheme, initTheme, onThemeChange, setTheme, storedTheme, systemTheme } from "./theme";

/** A system colour scheme the test can switch, like the desktop's dark mode. */
function fakeSystem(dark: boolean) {
  const listeners = new Set<() => void>();
  const media = {
    get matches() {
      return dark;
    },
    addEventListener: (_: string, fn: () => void) => listeners.add(fn),
    removeEventListener: (_: string, fn: () => void) => listeners.delete(fn),
  };
  const original = window.matchMedia;
  window.matchMedia = (() => media) as unknown as typeof window.matchMedia;
  return {
    set(next: boolean) {
      dark = next;
      for (const fn of listeners) fn();
    },
    listeners,
    restore: () => {
      window.matchMedia = original;
    },
  };
}

let system: ReturnType<typeof fakeSystem>;
let stop: () => void;

beforeEach(() => {
  system = fakeSystem(true);
  stop = initTheme();
});

afterEach(() => {
  setTheme(null);
  stop();
  system.restore();
});

describe("theme", () => {
  it("follows the system's colour scheme, also when it changes", () => {
    expect(storedTheme()).toBeNull();
    expect(systemTheme()).toBe("dark");
    expect(document.documentElement).toHaveClass("dark");
    system.set(false);
    expect(document.documentElement).not.toHaveClass("dark");
    expect(appliedTheme()).toBe("light");
  });

  it("a chosen scheme wins over the system and is remembered; null follows the system again", () => {
    const seen: string[] = [];
    const off = onThemeChange(() => seen.push(appliedTheme()));
    setTheme("light");
    expect(storedTheme()).toBe("light");
    expect(localStorage.getItem("keybund_theme")).toBe("light");
    expect(appliedTheme()).toBe("light");
    system.set(true);
    expect(appliedTheme()).toBe("light");
    setTheme(null);
    expect(localStorage.getItem("keybund_theme")).toBeNull();
    expect(appliedTheme()).toBe("dark");
    off();
    expect(seen).toEqual(["light", "dark"]);
  });

  it("stops following the system when cleaned up", () => {
    stop();
    expect(system.listeners.size).toBe(0);
  });

  it("is switched in the settings", async () => {
    const { user } = await renderApp("/settings");
    const select = await screen.findByLabelText("Farbschema");
    expect(screen.getByRole("option", { name: "Wie das System (Dunkel)" })).toBeInTheDocument();
    await user.selectOptions(select, "light");
    expect(appliedTheme()).toBe("light");
    expect(storedTheme()).toBe("light");
    await user.selectOptions(select, "");
    expect(storedTheme()).toBeNull();
    expect(appliedTheme()).toBe("dark");
  });
});
