import { m } from "@/paraglide/messages";
import type { HostEntry, Profile, ProfileView } from "./bindings";
import { providerLabel } from "./utils";

/** A server (ssh, scp, sftp) as opposed to a git hosting account. */
export function isServer(host: HostEntry): boolean {
  return host.provider === "generic";
}

export function servers(profile: Profile): HostEntry[] {
  return profile.hosts.filter(isServer);
}

export function gitAccounts(profile: Profile): HostEntry[] {
  return profile.hosts.filter((h) => !isServer(h));
}

/** What a key is used for, in short words: "Standard", "2 Ordner", "GitHub", "nas". */
export function usageParts(view: ProfileView): string[] {
  const { profile } = view;
  const parts: string[] = [];
  if (view.is_default) parts.push(m.usage_default());
  if (profile.folders.length === 1) parts.push(profile.folders[0]);
  else if (profile.folders.length > 1) parts.push(m.usage_folders({ n: profile.folders.length }));
  const providers = [...new Set(gitAccounts(profile).map((h) => providerLabel(h.provider)))];
  parts.push(...providers);
  const srv = servers(profile);
  if (srv.length === 1) parts.push(srv[0].alias ?? srv[0].hostname);
  else if (srv.length > 1) parts.push(m.usage_servers({ n: srv.length }));
  return parts;
}

/** Active keys, the standard key first. */
export function activeKeys(views: ProfileView[]): ProfileView[] {
  const active = views.filter((v) => !v.profile.archived);
  return [...active.filter((v) => v.is_default), ...active.filter((v) => !v.is_default)];
}
