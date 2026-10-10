import type { HostEntry, Profile, ProfileView } from "./bindings";

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

/** A profile that only exists for servers (no folders, no git accounts). */
export function isServerOnly(profile: Profile): boolean {
  return profile.folders.length === 0 && gitAccounts(profile).length === 0 && servers(profile).length > 0;
}

export interface ServerEntry {
  host: HostEntry;
  profile: Profile;
}

/**
 * The three ways a key is chosen: the default (fallback), projects and
 * accounts (by folder), servers (by host).
 */
export function groupProfiles(views: ProfileView[]) {
  const active = views.filter((v) => !v.profile.archived);
  return {
    standard: active.find((v) => v.is_default) ?? null,
    projects: active.filter((v) => !v.is_default && !isServerOnly(v.profile)),
    servers: active.flatMap((v) => servers(v.profile).map((host) => ({ host, profile: v.profile }))) as ServerEntry[],
    archived: views.filter((v) => v.profile.archived),
  };
}
