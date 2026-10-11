/**
 * Light or dark. The setting (localStorage) wins, else the system's colour scheme, followed live.
 * public/theme.js applies the same rule before the first paint; this module keeps the class on
 * <html> up to date afterwards.
 */
export type Theme = "light" | "dark";

const STORAGE_KEY = "keybund_theme";
const DARK_QUERY = "(prefers-color-scheme: dark)";

const isTheme = (v: unknown): v is Theme => v === "light" || v === "dark";

/** The stored choice, `null` = follow the system. */
export function storedTheme(): Theme | null {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    return isTheme(v) ? v : null;
  } catch {
    return null;
  }
}

/** The system's colour scheme; light where the browser cannot tell. */
export function systemTheme(): Theme {
  return window.matchMedia?.(DARK_QUERY).matches ? "dark" : "light";
}

export function currentTheme(): Theme {
  return storedTheme() ?? systemTheme();
}

const listeners = new Set<() => void>();

function apply(theme: Theme) {
  document.documentElement.classList.toggle("dark", theme === "dark");
  for (const fn of listeners) fn();
}

/** The colour scheme on screen now. */
export function appliedTheme(): Theme {
  return document.documentElement.classList.contains("dark") ? "dark" : "light";
}

/** Called after the colour scheme changed – for `useSyncExternalStore`. */
export function onThemeChange(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** Switches the colour scheme; `null` follows the system again. */
export function setTheme(theme: Theme | null) {
  try {
    if (theme) localStorage.setItem(STORAGE_KEY, theme);
    else localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Without storage the choice lasts until the app closes.
    if (theme) {
      apply(theme);
      return;
    }
  }
  apply(currentTheme());
}

/** Applies the colour scheme and follows the system's while there is no stored choice. */
export function initTheme(): () => void {
  apply(currentTheme());
  const media = window.matchMedia?.(DARK_QUERY);
  if (!media) return () => {};
  const onChange = () => {
    if (!storedTheme()) apply(systemTheme());
  };
  media.addEventListener("change", onChange);
  return () => media.removeEventListener("change", onChange);
}
