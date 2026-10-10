import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import type { HostEntry, Profile, Provider } from "./bindings";

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

export const COLOR_NAMES: Record<string, string> = {
  emerald: "Grün",
  blue: "Blau",
  violet: "Violett",
  amber: "Gelb",
  rose: "Rosa",
  cyan: "Türkis",
  orange: "Orange",
  zinc: "Grau",
};

export function colorHex(color: string | null | undefined): string {
  return (color && PROFILE_COLORS[color]) || PROFILE_COLORS.zinc;
}

export const PROVIDERS: Provider[] = ["github", "gitlab", "bitbucket", "generic"];

export function providerLabel(provider: Provider): string {
  return { github: "GitHub", gitlab: "GitLab", bitbucket: "Bitbucket", generic: "Server" }[provider];
}

export function defaultHostname(provider: Provider): string {
  return { github: "github.com", gitlab: "gitlab.com", bitbucket: "bitbucket.org", generic: "" }[provider];
}

/** Short label for the sidebar: the provider, or the number of hosts. */
export function hostSummary(profile: Profile): string {
  if (profile.hosts.length === 0) return "";
  if (profile.hosts.length > 1) return `${profile.hosts.length} Hosts`;
  return providerLabel(profile.hosts[0].provider);
}

export function hostLabel(host: HostEntry): string {
  const parts = [providerLabel(host.provider)];
  if (host.alias) parts.push(host.hostname);
  if (host.user) parts.push(`User ${host.user}`);
  parts.push(`Port ${host.port ?? 22}`);
  return parts.join(" · ");
}

/** `2026-03-03` → `03.03.2026` */
export function formatDate(iso: string | null | undefined): string {
  if (!iso) return "–";
  const [y, m, d] = iso.split("-");
  return `${d}.${m}.${y}`;
}

/** Age in days → „vor 7 Monaten“. */
export function formatAge(days: number | null | undefined): string {
  if (days == null) return "";
  if (days < 1) return "heute";
  if (days < 2) return "gestern";
  if (days < 31) return `vor ${days} Tagen`;
  const months = Math.floor(days / 30.44);
  if (months < 12) return months === 1 ? "vor 1 Monat" : `vor ${months} Monaten`;
  const years = Math.floor(days / 365.25);
  return years === 1 ? "vor 1 Jahr" : `vor ${years} Jahren`;
}

/** Milliseconds since the epoch → „vor 2 Min.“ */
export function formatRelative(at: number | null | undefined, now: number = Date.now()): string {
  if (at == null) return "";
  const seconds = Math.max(0, Math.round((now - at) / 1000));
  if (seconds < 45) return "gerade eben";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `vor ${minutes} Min.`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `vor ${hours} Std.`;
  return `vor ${Math.round(hours / 24)} Tagen`;
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

export function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}
