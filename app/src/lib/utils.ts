import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import { m } from "@/paraglide/messages";
import type { HostEntry, Provider } from "./bindings";
import { localeOf, pick } from "./i18n";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** Profile colors offered in the UI; the name is stored in profiles.toml. */
export const PROFILE_COLORS: Record<string, string> = {
  emerald: "#10b981",
  blue: "#3b82f6",
  violet: "#a78bfa",
  amber: "#f59e0b",
  rose: "#fb7185",
  cyan: "#22d3ee",
  orange: "#fb923c",
  zinc: "#a1a1aa",
};

const COLOR_NAMES: Record<string, () => string> = {
  emerald: m.color_emerald,
  blue: m.color_blue,
  violet: m.color_violet,
  amber: m.color_amber,
  rose: m.color_rose,
  cyan: m.color_cyan,
  orange: m.color_orange,
  zinc: m.color_zinc,
};

export function colorName(color: string): string {
  return pick(COLOR_NAMES, color);
}

export function colorHex(color: string | null | undefined): string {
  return (color && PROFILE_COLORS[color]) || PROFILE_COLORS.zinc;
}

export const PROVIDERS: Provider[] = ["github", "gitlab", "bitbucket", "generic"];
export const GIT_PROVIDERS: Provider[] = ["github", "gitlab", "bitbucket"];

export function providerLabel(provider: Provider): string {
  return provider === "generic" ? m.provider_generic() : { github: "GitHub", gitlab: "GitLab", bitbucket: "Bitbucket" }[provider];
}

export function defaultHostname(provider: Provider): string {
  return { github: "github.com", gitlab: "gitlab.com", bitbucket: "bitbucket.org", generic: "" }[provider];
}

/** Short label for the sidebar: the git provider(s), servers are listed separately. */
export function hostLabel(host: HostEntry): string {
  const parts = [providerLabel(host.provider)];
  if (host.alias) parts.push(host.hostname);
  if (host.user) parts.push(m.host_user({ user: host.user }));
  parts.push(m.host_port({ port: String(host.port ?? 22) }));
  return parts.join(" · ");
}

/** `2026-03-03` → `03.03.2026` (`03/03/2026` in English). */
export function formatDate(iso: string | null | undefined): string {
  if (!iso) return "–";
  const [y, mo, d] = iso.split("-").map(Number);
  return new Date(Date.UTC(y, mo - 1, d)).toLocaleDateString(localeOf(), {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    timeZone: "UTC",
  });
}

const relative = (numeric: "auto" | "always", style: "long" | "short" = "long") =>
  new Intl.RelativeTimeFormat(localeOf(), { numeric, style });

/** Age in days → „vor 7 Monaten“ / “7 months ago”. */
export function formatAge(days: number | null | undefined): string {
  if (days == null) return "";
  if (days < 2) return relative("auto").format(-Math.floor(days), "day");
  if (days < 31) return relative("always").format(-days, "day");
  const months = Math.floor(days / 30.44);
  if (months < 12) return relative("always").format(-months, "month");
  return relative("always").format(-Math.floor(days / 365.25), "year");
}

/** Milliseconds since the epoch → „vor 2 Min.“ / “2 min ago”. */
export function formatRelative(at: number | null | undefined, now: number = Date.now()): string {
  if (at == null) return "";
  const seconds = Math.max(0, Math.round((now - at) / 1000));
  if (seconds < 45) return m.time_justNow();
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return relative("always", "short").format(-minutes, "minute");
  const hours = Math.round(minutes / 60);
  if (hours < 24) return relative("always", "short").format(-hours, "hour");
  return relative("always").format(-Math.round(hours / 24), "day");
}

/** `heute, 09:41`, `gestern, 18:02` or `03.03.2026`. */
export function formatDay(at: number, now: Date = new Date()): string {
  const d = new Date(at);
  const time = d.toLocaleTimeString(localeOf(), { hour: "2-digit", minute: "2-digit" });
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((day(now) - day(d)) / 86_400_000);
  if (diff === 0) return m.time_todayAt({ time });
  if (diff === 1) return m.time_yesterdayAt({ time });
  return d.toLocaleDateString(localeOf(), { day: "2-digit", month: "2-digit", year: "numeric" });
}

/** `Kunde C` → `kunde-c`, same rules as the Rust side. */
export function slugify(name: string): string {
  let out = "";
  for (const ch of name.toLowerCase()) {
    const mapped = ({ ä: "ae", ö: "oe", ü: "ue", ß: "ss" } as Record<string, string>)[ch] ?? (/[a-z0-9]/.test(ch) ? ch : "-");
    if (mapped === "-" && (out === "" || out.endsWith("-"))) continue;
    out += mapped;
  }
  return out.replace(/-+$/, "");
}

export function isValidEmail(email: string): boolean {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim());
}

