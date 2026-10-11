// Sets the colour scheme before the first paint, so the app does not flash white in dark mode.
// A classic script from public/ because the CSP allows no inline scripts. The same rule as
// src/lib/theme.ts: the stored choice, else the system's.
(() => {
  let theme = null;
  try {
    theme = localStorage.getItem("keybund_theme");
  } catch {}
  if (theme !== "light" && theme !== "dark") {
    theme = window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  document.documentElement.classList.toggle("dark", theme === "dark");
})();
