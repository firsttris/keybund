/**
 * In-memory implementation of every Tauri command, used when the frontend
 * runs in a plain browser (`pnpm dev:mock`), in unit tests and in the
 * Playwright E2E tests. It mirrors the Rust behavior closely enough to
 * exercise all screens, but writes nothing to disk.
 */
import type {
  ApplyReport,
  ArchiveResult,
  Check,
  ClonePreview,
  CreatedProject,
  DeviceFlowStart,
  DeviceFlowStatus,
  FileChange,
  Fix,
  GeneratedFile,
  HostEntry,
  ImportCandidate,
  ImportProposal,
  InstallResult,
  KeyInfo,
  LastTest,
  NewProject,
  Paths,
  Plan,
  ProbeResult,
  Profile,
  ProfileDefaults,
  ProfileView,
  ProfilesView,
  Provider,
  Settings,
  UploadResult,
  UploadTarget,
  Report,
  UninstallOptions,
  UninstallPreview,
  WhichResult,
} from "../bindings";
import { slugify } from "../utils";

type Args = Record<string, unknown>;

class MockError {
  constructor(
    public kind: string,
    public message: string,
  ) {}
}

const ADD_KEY_URL: Record<Provider, ((host: string) => string) | null> = {
  github: (h) => `https://${h}/settings/ssh/new`,
  gitlab: (h) => `https://${h}/-/user_settings/ssh_keys`,
  bitbucket: () => "https://bitbucket.org/account/settings/ssh-keys/",
  generic: null,
};

const PROVIDER_NAME: Record<Provider, string> = {
  github: "GitHub",
  gitlab: "GitLab",
  bitbucket: "Bitbucket",
  generic: "Server",
};

function host(hostname: string, provider: Provider, extra: Partial<HostEntry> = {}): HostEntry {
  return { alias: null, hostname, user: null, port: null, provider, ...extra };
}

function profile(p: Partial<Profile> & Pick<Profile, "id" | "name">): Profile {
  return {
    color: null,
    key: { path: `~/.ssh/${p.id}_ed25519`, comment: null },
    git: null,
    folders: [],
    archived: false,
    hosts: [],
    ...p,
  };
}

/** Deterministic fake public key / fingerprint per key path. */
function fakeKey(path: string): { blob: string; fingerprint: string } {
  let h = 2166136261;
  for (const c of path) h = Math.imul(h ^ c.charCodeAt(0), 16777619) >>> 0;
  const chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let s = "";
  for (let i = 0; i < 43; i++) {
    h = Math.imul(h ^ (h >>> 13), 1274126177) >>> 0;
    s += chars[h % 64];
  }
  return { blob: `AAAAC3NzaC1lZDI1NTE5AAAAI${s.slice(0, 43)}`, fingerprint: `SHA256:${s}` };
}

export interface MockState {
  defaultProfile: string | null;
  profiles: Profile[];
  /** Key paths that exist, with their age in days. */
  keys: Map<string, { ageDays: number; comment: string }>;
  /** Rendered file contents as last applied. */
  applied: Map<string, string>;
  lastTests: Map<string, LastTest>;
  doctorIncludeOnTop: boolean;
  openedUrls: string[];
  /** Simulates a missing xdg-open. */
  openFails: boolean;
  /** Opens real browser tabs (browser preview); off in tests. */
  openTabs: boolean;
  clipboard: string | null;
  cloned: string[];
  /** Artificial latency for slow commands (ms). */
  latency: number;
  /** Host blocks of the user's own ~/.ssh/config. */
  sshBlocks: SshBlock[];
  /** Profile whose connection test logs in with another key from ~/.ssh/config. */
  otherKeyFor: string | null;
  /** Keybund's include lines are in the user's files. */
  installed: boolean;
  /** `hostname:keypath` pairs installed on servers via install_key. */
  installedKeys: string[];
  /** Hostnames that time out. */
  unreachable: string[];
  /** Keybund's OAuth apps are registered (login in the browser on github.com / gitlab.com). */
  deviceFlow: boolean;
  /** Polls the device flow answers "pending" before the outcome. */
  deviceFlowPolls: number;
  deviceFlowOutcome: "done" | "denied" | "expired";
  /** Seconds between polls the device flow asks for. */
  pollInterval: number;
  /** `secret-tool` is installed. */
  keyring: boolean;
  /** Tokens in the keyring by `profile@host`. */
  storedTokens: Map<string, string>;
  /** `hostname:keypath` pairs added to git accounts via the API. */
  uploadedKeys: string[];
  flows: Map<string, { id: string; host: string; polls: number }>;
  settings: Settings;
}

export interface SshBlock {
  pattern: string;
  hostname: string;
  identity: string;
  comment: string | null;
}

export function seedState(): MockState {
  const profiles = [
    profile({
      id: "privat",
      name: "Privat",
      color: "emerald",
      key: { path: "~/.ssh/privat_ed25519", comment: "tristan@zentrale" },
      git: { name: "Tristan", email: "tristan@privat.de", sign: true },
      folders: ["~/dev/privat/"],
      hosts: [host("github.com", "github"), host("1.2.3.4", "generic", { alias: "meinserver", user: "tristan", port: 22 })],
    }),
    profile({
      id: "sparkasse",
      name: "Sparkasse",
      color: "blue",
      key: { path: "~/.ssh/sparkasse_ed25519", comment: "tristan@zentrale-sparkasse" },
      git: { name: "Tristan", email: "tristan@kunde.de", sign: false },
      folders: ["~/dev/sparkasse/"],
      hosts: [host("gitlab.kunde.de", "gitlab")],
    }),
    profile({
      id: "kunde-b",
      name: "Kunde B",
      color: "violet",
      git: { name: "Tristan", email: "tristan@kunde-b.de", sign: false },
      folders: ["~/dev/kunde-b/"],
      hosts: [host("github.com", "github")],
    }),
    profile({
      id: "bundesagentur",
      name: "Bundesagentur",
      color: "amber",
      key: { path: "~/.ssh/archive/bundesagentur_ed25519", comment: null },
      git: { name: "Tristan", email: "tristan@ba.de", sign: false },
      folders: ["~/dev/bundesagentur/"],
      archived: true,
      hosts: [host("github.com", "github")],
    }),
  ];
  const state: MockState = {
    defaultProfile: "privat",
    profiles,
    keys: new Map([
      ["~/.ssh/privat_ed25519", { ageDays: 420, comment: "tristan@zentrale" }],
      ["~/.ssh/sparkasse_ed25519", { ageDays: 221, comment: "tristan@zentrale-sparkasse" }],
      ["~/.ssh/kunde-b_ed25519", { ageDays: 35, comment: "tristan@zentrale-kunde-b" }],
      ["~/.ssh/archive/bundesagentur_ed25519", { ageDays: 900, comment: "tristan@ba" }],
      ["~/.ssh/id_ed25519", { ageDays: 1300, comment: "tristan@laptop" }],
      ["~/.ssh/pflegeabc/id_ed25519", { ageDays: 500, comment: "tristan@pflegeabc" }],
      ["~/.ssh/s-communication_ed25519", { ageDays: 600, comment: "tristan@s-communication" }],
    ]),
    applied: new Map(),
    lastTests: new Map([
      ["sparkasse", { success: true, account: "@tristan", message: "Verbunden als @tristan", at: Date.now() - 120_000 }],
    ]),
    doctorIncludeOnTop: false,
    openedUrls: [],
    openFails: false,
    openTabs: false,
    clipboard: null,
    cloned: [],
    latency: 300,
    sshBlocks: [
      { pattern: "pflegeabc", hostname: "github.com", identity: "~/.ssh/pflegeabc/id_ed25519", comment: "Pflegeabc Account" },
      {
        pattern: "gitlab.sfp.berlin",
        hostname: "gitlab.sfp.berlin",
        identity: "~/.ssh/s-communication_ed25519",
        comment: "S-Communication",
      },
    ],
    otherKeyFor: null,
    installed: true,
    installedKeys: [],
    deviceFlow: true,
    deviceFlowPolls: 1,
    deviceFlowOutcome: "done",
    pollInterval: 2,
    keyring: true,
    storedTokens: new Map(),
    uploadedKeys: [],
    flows: new Map(),
    settings: { projects_dir: "~/dev", keys_dir: "~/.ssh", inline_ssh_config: false, known_hosts_per_profile: true },
    unreachable: [],
  };
  state.applied = renderAll(state);
  return state;
}

// --- rendering (mirrors crates/core/src/render.rs) -------------------------

const MANAGED = "# Managed by Keybund – manual changes will be overwritten.";

function sshCommand(path: string) {
  return `ssh -i ${path} -o IdentitiesOnly=yes -o AddKeysToAgent=yes`;
}

export function renderProfileGitconfig(p: Profile): string {
  let out = `${MANAGED}\n# Profil: ${p.name}\n`;
  if (p.git) {
    out += `[user]\n\tname = ${p.git.name}\n\temail = ${p.git.email}\n`;
    if (p.git.sign) out += `\tsigningkey = ${p.key.path}.pub\n`;
  }
  out += `[core]\n\tsshCommand = ${sshCommand(p.key.path)}\n`;
  if (p.git?.sign) out += "[gpg]\n\tformat = ssh\n[commit]\n\tgpgsign = true\n[tag]\n\tgpgsign = true\n";
  return out;
}

function includeIfBlocks(p: Profile): string {
  return p.folders
    .map((f) => `[includeIf "gitdir:${f}"]\n\tpath = ~/.config/keybund/git/${p.id}.gitconfig\n`)
    .join("\n");
}

/** Mirrors HostEntry::ssh_patterns: servers by alias and hostname, git hosts only by alias. */
export function sshPatterns(h: HostEntry): string[] {
  const patterns = h.alias ? [h.alias] : [];
  if (h.provider === "generic" && !patterns.includes(h.hostname)) patterns.push(h.hostname);
  return patterns;
}

function sshHostBlocks(p: Profile): string {
  return p.hosts
    .filter((h) => sshPatterns(h).length > 0)
    .map((h) => {
      let block = `# Profil: ${p.name}\nHost ${sshPatterns(h).join(" ")}\n    HostName ${h.hostname}\n`;
      if (h.user) block += `    User ${h.user}\n`;
      if (h.port != null) block += `    Port ${h.port}\n`;
      return `${block}    IdentityFile ${p.key.path}\n    IdentitiesOnly yes\n    AddKeysToAgent yes\n`;
    })
    .join("\n");
}

function renderAll(state: MockState, profiles = state.profiles, def = state.defaultProfile): Map<string, string> {
  const active = profiles.filter((p) => !p.archived);
  const files = new Map<string, string>();
  const ssh = active.map(sshHostBlocks).filter(Boolean);
  files.set("~/.ssh/config.d/keybund.conf", `${MANAGED}\n${ssh.map((b) => `\n${b}`).join("")}\nHost *\n`);
  let index = `${MANAGED}\n# Included from ~/.gitconfig.\n`;
  const defProfile = active.find((p) => p.id === def);
  if (defProfile) index += `\n# Default-Profil: ${defProfile.name}\n[core]\n\tsshCommand = ${sshCommand(defProfile.key.path)}\n`;
  for (const p of active) if (p.folders.length) index += `\n${includeIfBlocks(p)}`;
  files.set("~/.config/keybund/git/index.gitconfig", index);
  for (const p of active) files.set(`~/.config/keybund/git/${p.id}.gitconfig`, renderProfileGitconfig(p));
  return files;
}

function lineDiff(path: string, oldText: string | undefined, newText: string | undefined): FileChange {
  const a = oldText ? oldText.split("\n").filter((l, i, arr) => i < arr.length - 1 || l) : [];
  const b = newText ? newText.split("\n").filter((l, i, arr) => i < arr.length - 1 || l) : [];
  const removed = a.filter((l) => !b.includes(l));
  const added = b.filter((l) => !a.includes(l));
  const body = [
    ...a.map((l) => (b.includes(l) ? ` ${l}` : `-${l}`)),
    ...added.map((l) => `+${l}`),
  ];
  return {
    path,
    kind: oldText === undefined ? "create" : newText === undefined ? "delete" : "modify",
    diff: `--- ${path}\n+++ ${path}\n@@ -1,${a.length} +1,${b.length} @@\n${body.join("\n")}\n`,
    added: added.length,
    removed: removed.length,
    user_file: false,
  };
}

function planFor(state: MockState, profiles = state.profiles, def = state.defaultProfile): Plan {
  const next = renderAll(state, profiles, def);
  const paths = new Set([...state.applied.keys(), ...next.keys()]);
  const changes: FileChange[] = [];
  for (const path of paths) {
    const before = state.applied.get(path);
    const after = next.get(path);
    if (before !== after) changes.push(lineDiff(path, before, after));
  }
  return { changes };
}

// --- ~/.ssh/config import (mirrors crates/core/src/import.rs) ---------------

function renderSshConfig(blocks: SshBlock[]): string {
  const body = blocks
    .map(
      (b) =>
        `${b.comment ? `# ${b.comment}\n` : ""}Host ${b.pattern}\n    HostName ${b.hostname}\n    User git\n    IdentityFile ${b.identity}\n    IdentitiesOnly yes\n`,
    )
    .join("\n");
  return `Include ~/.ssh/config.d/keybund.conf\n${body ? `\n${body}` : ""}`;
}

function covered(state: MockState, b: SshBlock): boolean {
  return state.profiles.some(
    (p) =>
      !p.archived &&
      p.key.path === b.identity &&
      p.hosts.some((h) => h.alias === b.pattern || (!h.alias && h.hostname === b.pattern)),
  );
}

function detectProvider(hostname: string): Provider {
  if (hostname.includes("github")) return "github";
  if (hostname.includes("bitbucket")) return "bitbucket";
  if (hostname.includes("gitlab")) return "gitlab";
  return "generic";
}

function proposals(state: MockState): ImportProposal[] {
  let defaultGiven = state.defaultProfile !== null;
  const taken = new Set(state.profiles.map((p) => p.id));
  return state.sshBlocks
    .filter((b) => !covered(state, b))
    .map((b) => {
      const file = b.identity.split("/").pop()!.replace(/_(ed25519|rsa|ecdsa)$/, "");
      let id = file === "id" || file.startsWith("id_") ? slugify(b.pattern) : slugify(file);
      for (let n = 2; taken.has(id); n++) id = `${id.replace(/-\d+$/, "")}-${n}`;
      taken.add(id);
      const provider = detectProvider(b.hostname);
      const asDefault = b.pattern === b.hostname && provider !== "generic" && !defaultGiven;
      if (asDefault) defaultGiven = true;
      return {
        profile: profile({
          id,
          name: b.comment ?? b.pattern,
          key: { path: b.identity, comment: null },
          hosts: [host(b.hostname, provider, { alias: asDefault ? null : b.pattern })],
        }),
        make_default: asDefault,
        blocks: [b.pattern],
        key_exists: state.keys.has(b.identity),
        notes: asDefault ? [`Wird Default-Profil: gilt für ${b.hostname} in allen Ordnern ohne eigenes Profil`] : [],
      };
    });
}

function cleanupPlan(state: MockState): Plan {
  const remaining = state.sshBlocks.filter((b) => !covered(state, b));
  if (remaining.length === state.sshBlocks.length) return { changes: [] };
  const change = lineDiff("~/.ssh/config", renderSshConfig(state.sshBlocks), renderSshConfig(remaining));
  return { changes: [{ ...change, user_file: true }] };
}

function sshConfigChecks(state: MockState): Check[] {
  const shared = state.profiles.filter((p) => !p.archived).flatMap((p) => p.hosts.filter((h) => !h.alias).map((h) => h.hostname));
  return state.sshBlocks.flatMap((b): Check[] => {
    const mixes = shared.includes(b.pattern);
    const isCovered = covered(state, b);
    if (!mixes && !isCovered) return [];
    return [
      {
        id: `ssh-config-${b.pattern}`,
        status: "warn",
        title: isCovered
          ? mixes
            ? `Alter Eintrag für ${b.pattern} in ~/.ssh/config`
            : `Doppelter Eintrag für ${b.pattern} in ~/.ssh/config`
          : `Globaler Eintrag für ${b.pattern} in ~/.ssh/config`,
        detail: isCovered ? "Keybund schreibt diesen Host-Block selbst, der alte kann weg." : `${b.identity} wird zusätzlich angeboten.`,
        fix: isCovered ? { type: "cleanup_ssh_config" } : null,
      },
    ];
  });
}

// --- views -----------------------------------------------------------------

function keyInfo(state: MockState, path: string): KeyInfo {
  const key = state.keys.get(path);
  const fake = fakeKey(path);
  const created = key ? new Date(Date.now() - key.ageDays * 86_400_000).toISOString().slice(0, 10) : null;
  return {
    path,
    pub_path: `${path}.pub`,
    private_exists: !!key,
    public_exists: !!key,
    key_type: key ? "ssh-ed25519" : null,
    fingerprint: key ? fake.fingerprint : null,
    comment: key?.comment ?? null,
    public_key: key ? `ssh-ed25519 ${fake.blob} ${key.comment}` : null,
    created,
    age_days: key?.ageDays ?? null,
  };
}

const API_PROVIDERS: Provider[] = ["github", "gitlab"];

function gitHost(state: MockState, id: string, host: string): { p: Profile; h: HostEntry } {
  const p = find(state, id);
  const h = p.hosts.find((x) => x.provider !== "generic" && (x.alias === host || x.hostname === host));
  if (!h) throw new MockError("other", `Profil „${p.id}“ hat kein Git-Konto ${host}`);
  return { p, h };
}

function hasDeviceFlow(state: MockState, h: HostEntry): boolean {
  return state.deviceFlow && ((h.provider === "github" && h.hostname === "github.com") || (h.provider === "gitlab" && h.hostname === "gitlab.com"));
}

function uploadTo(state: MockState, p: Profile, h: HostEntry, token: string): UploadResult {
  const name = PROVIDER_NAME[h.provider];
  if (token === "falsch") {
    return { success: false, already_present: false, account: null, message: `${name} hat das Token abgelehnt (abgelaufen, widerrufen oder ohne Berechtigung)`, auth_failed: true };
  }
  const key = `${h.hostname}:${p.key.path}`;
  const already = state.uploadedKeys.includes(key);
  if (!already) state.uploadedKeys.push(key);
  return {
    success: true,
    already_present: already,
    account: "tristan",
    message: already ? `Der Key war bei ${name} schon eingetragen (@tristan)` : `Key bei ${name} eingetragen (@tristan)`,
    auth_failed: false,
  };
}

function view(state: MockState, p: Profile): ProfileView {
  return {
    profile: structuredClone(p),
    key: keyInfo(state, p.key.path),
    is_default: state.defaultProfile === p.id,
    add_key_urls: p.hosts.flatMap((h) => {
      const url = ADD_KEY_URL[h.provider]?.(h.hostname);
      return url ? [{ host: h.hostname, provider: h.provider, url }] : [];
    }),
    last_test: state.lastTests.get(p.id) ?? null,
  };
}

function find(state: MockState, id: string): Profile {
  const p = state.profiles.find((x) => x.id === id);
  if (!p) throw new MockError("profile_not_found", `Profil „${id}“ existiert nicht`);
  return p;
}

function normalizeFolder(f: string): string {
  const t = f.trim();
  return t.endsWith("/") ? t : `${t}/`;
}

function validate(profiles: Profile[]) {
  const folders = profiles.filter((p) => !p.archived).flatMap((p) => p.folders);
  for (let i = 0; i < folders.length; i++)
    for (let j = i + 1; j < folders.length; j++)
      if (folders[i].startsWith(folders[j]) || folders[j].startsWith(folders[i]))
        throw new MockError("nested_folders", `Projektordner überschneiden sich: ${folders[i]} und ${folders[j]}`);
  const patterns = new Set<string>();
  for (const h of profiles.filter((p) => !p.archived).flatMap((p) => p.hosts)) {
    for (const pattern of sshPatterns(h)) {
      if (patterns.has(pattern)) throw new MockError("duplicate_alias", `Host „${pattern}“ ist mehreren Profilen zugeordnet`);
      patterns.add(pattern);
    }
  }
  const ids = new Set<string>();
  for (const p of profiles) {
    if (ids.has(p.id)) throw new MockError("duplicate_id", `Profil-ID „${p.id}“ ist doppelt vergeben`);
    ids.add(p.id);
  }
}

function withNewProject(state: MockState, project: NewProject): Profile[] {
  const p = structuredClone(project.profile);
  p.folders = p.folders.filter((f) => f.trim()).map(normalizeFolder);
  if (project.key.type === "existing") p.key = { path: project.key.path, comment: null };
  const profiles = [...state.profiles, p];
  validate(profiles);
  return profiles;
}

function doctorReport(state: MockState): Report {
  const plan = planFor(state);
  const checks: Check[] = [
    state.doctorIncludeOnTop
      ? { id: "ssh-include", status: "ok", title: "Include steht ganz oben in ~/.ssh/config", detail: null, fix: null }
      : {
          id: "ssh-include",
          status: "error",
          title: "Include steht nicht ganz oben",
          detail: "In ~/.ssh/config steht es unter einem Host-Block und gilt nur für diesen.",
          fix: { type: "move_ssh_include" },
        },
    { id: "ssh-dir-perms", status: "ok", title: "~/.ssh hat Rechte 700", detail: null, fix: null },
    { id: "key-perms", status: "ok", title: "Private Keys haben Rechte 600", detail: null, fix: null },
    { id: "git-env", status: "ok", title: "Keine Umgebungsvariable übersteuert Git", detail: null, fix: null },
    { id: "agent", status: "ok", title: "ssh-agent läuft", detail: "3 Keys geladen", fix: null },
    { id: "add-keys", status: "ok", title: "AddKeysToAgent aktiv", detail: null, fix: null },
    plan.changes.length === 0
      ? { id: "configs", status: "ok", title: "Configs sind aktuell", detail: null, fix: null }
      : {
          id: "configs",
          status: "warn",
          title: `${plan.changes.length} Datei(en) nicht aktuell`,
          detail: plan.changes.map((c) => c.path).join(", "),
          fix: { type: "apply" },
        },
    ...sshConfigChecks(state),
  ];
  return { checks };
}

function which(state: MockState, query: string): WhichResult {
  const q = query.trim();
  const base: WhichResult = {
    query: q,
    mode: "folder",
    key_path: null,
    fingerprint: null,
    profile_id: null,
    source: null,
    via: null,
    identities_only: null,
    git_name: null,
    git_email: null,
    hostname: null,
    user: null,
    port: null,
    agent_loaded: null,
    notes: [],
  };
  const active = state.profiles.filter((p) => !p.archived);
  const isPath = /^[~/.]/.test(q) || q.includes("/");
  if (isPath) {
    const dir = q.endsWith("/") ? q : `${q}/`;
    const match = active
      .flatMap((p) => p.folders.map((f) => ({ p, f })))
      .filter(({ f }) => dir.startsWith(f))
      .sort((a, b) => b.f.length - a.f.length)[0];
    const p = match?.p ?? active.find((x) => x.id === state.defaultProfile);
    if (!p) return { ...base, notes: ["Ordner gehört zu keinem Profil und es gibt kein Default-Profil."] };
    return {
      ...base,
      key_path: p.key.path,
      fingerprint: keyInfo(state, p.key.path).fingerprint,
      profile_id: p.id,
      source: match ? `~/.config/keybund/git/${p.id}.gitconfig` : "~/.config/keybund/git/index.gitconfig",
      via: match ? `includeIf "gitdir:${match.f}"` : "Default-Profil",
      identities_only: true,
      git_name: p.git?.name ?? null,
      git_email: p.git?.email ?? null,
      agent_loaded: true,
      notes: match ? [] : ["Ordner gehört zu keinem Profil, es gilt das Default-Profil."],
    };
  }
  const aliased = active.find((p) => p.hosts.some((h) => h.alias === q));
  if (aliased) {
    const h = aliased.hosts.find((x) => x.alias === q)!;
    return {
      ...base,
      mode: "host",
      key_path: aliased.key.path,
      fingerprint: keyInfo(state, aliased.key.path).fingerprint,
      profile_id: aliased.id,
      source: "~/.ssh/config.d/keybund.conf",
      via: `Host ${q}`,
      identities_only: true,
      hostname: h.hostname,
      user: h.user,
      port: h.port ?? 22,
      agent_loaded: true,
    };
  }
  const users = active.filter((p) => p.hosts.some((h) => !h.alias && h.hostname === q));
  return {
    ...base,
    mode: "host",
    source: "ssh -G",
    hostname: q,
    port: 22,
    identities_only: false,
    notes: users.length
      ? [`Für Git-Repos entscheidet der Ordner, nicht der Host (${q} wird von ${users.map((p) => p.name).join(", ")} genutzt).`]
      : ["Kein Profil kennt diesen Host – ssh probiert die Standard-Keys."],
  };
}

// --- dispatcher ---------------------------------------------------------------

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export function createMockBackend(state: MockState = seedState()) {
  function openUrl(url: string) {
    if (!url.startsWith("https://")) throw new MockError("tool_failed", `Nur https-Links werden geöffnet: ${url}`);
    if (state.openFails) throw new MockError("tool_missing", "Programm „xdg-open“ nicht gefunden – ist es installiert?");
    state.openedUrls.push(url);
    if (state.openTabs) window.open(url, "_blank", "noopener");
  }

  async function handle(cmd: string, args: Args): Promise<unknown> {
    switch (cmd) {
      case "load_profiles": {
        const result: ProfilesView = {
          default_profile: state.defaultProfile,
          profiles: state.profiles.map((p) => view(state, p)),
          pending_changes: planFor(state).changes.length,
          import_available: proposals(state).length,
        };
        return result;
      }
      case "profile_defaults": {
        const id = slugify(String(args.name ?? ""));
        const keyPath = `${state.settings.keys_dir}/${id}_ed25519`;
        const result: ProfileDefaults = {
          id,
          key_path: keyPath,
          folder: `${state.settings.projects_dir}/${id}/`,
          key_comment: `tristan@zentrale-${id}`,
          key_exists: state.keys.has(keyPath),
          id_taken: state.profiles.some((p) => p.id === id),
        };
        return result;
      }
      case "settings":
        return structuredClone(state.settings);
      case "update_settings": {
        const next = structuredClone(args.settings as Settings);
        for (const key of ["projects_dir", "keys_dir"] as const) {
          const v = next[key].trim();
          if (!(v.startsWith("/") || v === "~" || v.startsWith("~/"))) {
            throw new MockError("invalid_path", `Ungültiger Ordner „${v}“ – bitte mit / oder ~/ angeben`);
          }
          next[key] = v.length > 1 ? v.replace(/\/+$/, "") : v;
        }
        state.settings = next;
        return structuredClone(next);
      }
      case "normalize_folder":
        return normalizeFolder(String(args.folder));
      case "preview_new_project": {
        const project = args.project as NewProject;
        if (state.profiles.some((p) => p.id === project.profile.id))
          throw new MockError("duplicate_id", `Profil-ID „${project.profile.id}“ ist doppelt vergeben`);
        return planFor(state, withNewProject(state, project), state.defaultProfile ?? project.profile.id);
      }
      case "create_project": {
        await sleep(state.latency);
        const project = args.project as NewProject;
        if (state.profiles.some((p) => p.id === project.profile.id))
          throw new MockError("duplicate_id", `Profil-ID „${project.profile.id}“ ist doppelt vergeben`);
        const profiles = withNewProject(state, project);
        const p = profiles[profiles.length - 1];
        if (project.key.type === "generate") {
          if (state.keys.has(p.key.path)) throw new MockError("key_exists", `Key existiert bereits: ${p.key.path}`);
          state.keys.set(p.key.path, { ageDays: 0, comment: project.key.comment });
          p.key.comment = project.key.comment;
        } else if (!state.keys.has(p.key.path)) {
          throw new MockError("key_missing", `Key fehlt: ${p.key.path}`);
        }
        state.profiles = profiles;
        if (project.make_default || !state.defaultProfile) state.defaultProfile = p.id;
        const plan = planFor(state);
        state.applied = renderAll(state);
        const v = view(state, p);
        state.clipboard = v.key.public_key;
        let openError: string | null = null;
        if (args.openProviderPage && v.add_key_urls[0]) {
          try {
            openUrl(v.add_key_urls[0].url);
          } catch (e) {
            openError = (e as MockError).message;
          }
        }
        const created: CreatedProject = {
          profile: v,
          report: { written: plan.changes.map((c) => c.path), deleted: [], backups: [] },
          public_key: v.key.public_key ?? "",
          open_error: openError,
        };
        return created;
      }
      case "update_profile": {
        const next = structuredClone(args.profile as Profile);
        next.folders = next.folders.filter((f) => f.trim()).map(normalizeFolder);
        const profiles = state.profiles.map((p) => (p.id === next.id ? next : p));
        find(state, next.id);
        validate(profiles);
        state.profiles = profiles;
        return view(state, next);
      }
      case "set_default_profile":
        state.defaultProfile = (args.id as string | null) ?? null;
        return null;
      case "copy_public_key": {
        const key = keyInfo(state, find(state, String(args.id)).key.path).public_key;
        if (!key) throw new MockError("key_missing", "Key fehlt");
        state.clipboard = key;
        return key;
      }
      case "open_url":
        openUrl(String(args.url));
        return null;
      case "test_connection": {
        await sleep(state.latency);
        const p = find(state, String(args.id));
        const wanted = args.host as string | null | undefined;
        const h = wanted ? p.hosts.find((x) => x.alias === wanted || x.hostname === wanted) : p.hosts[0];
        if (!h) throw new MockError("no_host", wanted ? `Profil „${p.id}“ hat keinen Host ${wanted}` : `Profil „${p.id}“ hat keinen Host`);
        const ok = p.id !== "kunde-b" && !state.unreachable.includes(h.hostname);
        const result: ProbeResult = {
          success: ok,
          host: h.alias ?? h.hostname,
          account: ok && h.provider !== "generic" ? "@tristan" : null,
          message: ok
            ? h.provider === "generic"
              ? "Anmeldung erfolgreich"
              : "Verbunden als @tristan"
            : h.provider === "generic"
              ? `${h.alias ?? h.hostname} kennt diesen Key nicht`
              : `${PROVIDER_NAME[h.provider]} kennt diesen Key nicht`,
          output: ok ? "Hi tristan! You've successfully authenticated." : "Permission denied (publickey).",
          command: `ssh -T -o PasswordAuthentication=no -o IdentitiesOnly=yes -o AddKeysToAgent=yes -i ${p.key.path} ${h.alias ?? h.hostname}`,
          used_key: ok ? p.key.path : null,
          other_key: false,
        };
        if (state.otherKeyFor === p.id) {
          Object.assign(result, {
            success: false,
            other_key: true,
            used_key: "~/.ssh/privat_ed25519",
            account: "@firsttris",
            message: "Angemeldet als @firsttris, aber mit ~/.ssh/privat_ed25519 statt mit dem Key des Profils",
          });
        }
        state.lastTests.set(p.id, { success: result.success, account: result.account, message: result.message, at: Date.now() });
        return result;
      }
      case "install_key": {
        await sleep(state.latency);
        const p = find(state, String(args.id));
        const name = String(args.host);
        const h = p.hosts.find((x) => x.alias === name || x.hostname === name);
        if (!h) throw new MockError("other", `Profil „${p.id}“ hat keinen Server ${name}`);
        const key = `${h.hostname}:${p.key.path}`;
        if (state.unreachable.includes(h.hostname)) {
          return { success: false, already_present: false, message: `${name} ist nicht erreichbar`, output: "ssh: connect to host … Connection timed out" } satisfies InstallResult;
        }
        const already = state.installedKeys.includes(key);
        if (!already) state.installedKeys.push(key);
        return {
          success: true,
          already_present: already,
          message: already ? `Der Key war auf ${name} schon eingetragen` : `Key auf ${name} eingetragen`,
          output: "",
        } satisfies InstallResult;
      }
      case "upload_target": {
        const { p, h } = gitHost(state, String(args.id), String(args.host));
        const sign = !!p.git?.sign;
        const supported = API_PROVIDERS.includes(h.provider);
        const scopes = !supported ? [] : h.provider === "github" ? ["write:public_key", ...(sign ? ["admin:ssh_signing_key"] : [])] : ["api"];
        return {
          provider: h.provider,
          hostname: h.hostname,
          supported,
          device_flow: supported && hasDeviceFlow(state, h),
          token_url: !supported
            ? null
            : h.provider === "github"
              ? `https://${h.hostname}/settings/tokens/new?scopes=${scopes.join(",")}&description=Keybund`
              : `https://${h.hostname}/-/user_settings/personal_access_tokens?name=Keybund&scopes=api`,
          scopes,
          manual_url: ADD_KEY_URL[h.provider]?.(h.hostname) ?? null,
          remembered: state.keyring && state.storedTokens.has(`${p.id}@${h.hostname}`),
          keyring: state.keyring,
        } satisfies UploadTarget;
      }
      case "start_device_flow": {
        await sleep(state.latency);
        const { p, h } = gitHost(state, String(args.id), String(args.host));
        if (!hasDeviceFlow(state, h)) throw new MockError("api", `Für ${h.hostname} ist keine Anmeldung per Browser eingerichtet – bitte ein Token verwenden`);
        const flowId = `flow-${state.flows.size + 1}`;
        state.flows.set(flowId, { id: p.id, host: String(args.host), polls: 0 });
        return {
          flow_id: flowId,
          user_code: "WDJB-MJHT",
          verification_uri: h.provider === "github" ? "https://github.com/login/device" : "https://gitlab.com/oauth/device?user_code=WDJB-MJHT",
          expires_in: 900,
          interval: state.pollInterval,
        } satisfies DeviceFlowStart;
      }
      case "poll_device_flow": {
        const flowId = String(args.flowId);
        const flow = state.flows.get(flowId);
        if (!flow) throw new MockError("other", "Diese Anmeldung ist nicht mehr aktiv – bitte neu starten");
        if (flow.polls < state.deviceFlowPolls) {
          flow.polls++;
          return { status: "pending", interval: state.pollInterval } satisfies DeviceFlowStatus;
        }
        state.flows.delete(flowId);
        if (state.deviceFlowOutcome !== "done") return { status: state.deviceFlowOutcome } satisfies DeviceFlowStatus;
        const { p, h } = gitHost(state, flow.id, flow.host);
        const result = uploadTo(state, p, h, "gho_mock");
        if (args.remember && state.keyring) state.storedTokens.set(`${p.id}@${h.hostname}`, "gho_mock");
        return { status: "done", result } satisfies DeviceFlowStatus;
      }
      case "cancel_device_flow":
        state.flows.delete(String(args.flowId));
        return null;
      case "upload_key": {
        await sleep(state.latency);
        const { p, h } = gitHost(state, String(args.id), String(args.host));
        if (!API_PROVIDERS.includes(h.provider)) throw new MockError("other", `${PROVIDER_NAME[h.provider]} kann Keybund nicht automatisch eintragen`);
        const slot = `${p.id}@${h.hostname}`;
        const given = (args.token as string | null)?.trim() || null;
        const token = given ?? state.storedTokens.get(slot);
        if (!token) throw new MockError("other", "Kein gespeichertes Token – bitte anmelden oder ein Token eingeben");
        const result = uploadTo(state, p, h, token);
        if (result.auth_failed && !given) state.storedTokens.delete(slot);
        if (result.success && given && args.remember && state.keyring) state.storedTokens.set(slot, given);
        return result;
      }
      case "forget_token": {
        const { p, h } = gitHost(state, String(args.id), String(args.host));
        state.storedTokens.delete(`${p.id}@${h.hostname}`);
        return null;
      }
      case "preview_add_host":
      case "add_host": {
        const p = find(state, String(args.id));
        const host = structuredClone(args.host as HostEntry);
        const next = { ...structuredClone(p), hosts: [...p.hosts, host] };
        const profiles = state.profiles.map((x) => (x.id === p.id ? next : x));
        validate(profiles);
        if (cmd === "preview_add_host") return planFor(state, profiles);
        state.profiles = profiles;
        state.applied = renderAll(state);
        return view(state, next);
      }
      case "last_test":
        return state.lastTests.get(String(args.id)) ?? null;
      case "plan_changes":
        return planFor(state);
      case "apply_changes": {
        const plan = planFor(state);
        const expected = args.expectedDiffs as string[];
        if (JSON.stringify(plan.changes.map((c) => c.diff)) !== JSON.stringify(expected))
          throw new MockError("stale_plan", "Die Konfiguration hat sich seit der Vorschau geändert: Bitte Vorschau neu laden");
        state.applied = renderAll(state);
        const report: ApplyReport = {
          written: plan.changes.filter((c) => c.kind !== "delete").map((c) => c.path),
          deleted: plan.changes.filter((c) => c.kind === "delete").map((c) => c.path),
          backups: [],
        };
        return report;
      }
      case "which":
        await sleep(state.latency / 3);
        return which(state, String(args.query));
      case "run_doctor":
        return doctorReport(state);
      case "fix_doctor": {
        const fix = args.fix as Fix;
        if (fix.type === "move_ssh_include") state.doctorIncludeOnTop = true;
        if (fix.type === "apply") state.applied = renderAll(state);
        if (fix.type === "cleanup_ssh_config") state.sshBlocks = state.sshBlocks.filter((b) => !covered(state, b));
        return doctorReport(state);
      }
      case "scan_keys": {
        const result: ImportCandidate[] = [...state.keys.keys()]
          .filter((k) => !k.includes("/archive/"))
          .sort()
          .map((k) => ({ key: keyInfo(state, k), used_by: state.profiles.find((p) => p.key.path === k)?.id ?? null }));
        return result;
      }
      case "clone_preview": {
        const url = String(args.url).trim();
        const m = url.match(/^(?:[a-z+]+:\/\/)?(?:[^@/]+@)?([^:/]+)(?::\d+)?[:/](.+?)\/?$/);
        if (!m) throw new MockError("invalid_url", `Ungültige Repo-URL: ${url}`);
        const [, hostname, path] = m;
        const name = path.split("/").pop()!.replace(/\.git$/, "");
        const active = state.profiles.filter((p) => !p.archived);
        const matching = active.filter((p) => p.hosts.some((h) => h.hostname === hostname || h.alias === hostname));
        const list = matching.length ? matching : active;
        const result: ClonePreview = {
          repo: { host: hostname, path, name },
          matched: matching.length > 0,
          candidates: list.map((p) => ({
            profile_id: p.id,
            profile_name: p.name,
            target: p.folders[0] ? `${p.folders[0]}${name}` : null,
          })),
        };
        return result;
      }
      case "clone_repo": {
        await sleep(state.latency);
        const p = find(state, String(args.profileId));
        const name = String(args.url).split(/[/:]/).pop()!.replace(/\.git$/, "");
        const target = (args.target as string | null) || (p.folders[0] ? `${p.folders[0]}${name}` : null);
        if (!target) throw new MockError("no_folder", `Profil „${p.id}“ hat keinen Projektordner`);
        if (state.cloned.includes(target)) throw new MockError("target_exists", `Zielordner existiert bereits: ${target}`);
        state.cloned.push(target);
        return target;
      }
      case "archive_profile": {
        const p = find(state, String(args.id));
        if (p.archived) throw new MockError("archived", `Profil „${p.id}“ ist archiviert`);
        const file = p.key.path.split("/").pop()!;
        const newPath = `~/.ssh/archive/${file}`;
        const key = state.keys.get(p.key.path);
        state.keys.delete(p.key.path);
        if (key) state.keys.set(newPath, key);
        p.key = { ...p.key, path: newPath };
        p.archived = true;
        if (state.defaultProfile === p.id) state.defaultProfile = null;
        const result: ArchiveResult = {
          key_path: newPath,
          remove_key_urls: p.hosts.flatMap((h) =>
            h.provider === "github" ? [`https://${h.hostname}/settings/keys`] : ADD_KEY_URL[h.provider] ? [ADD_KEY_URL[h.provider]!(h.hostname)] : [],
          ),
        };
        return result;
      }
      case "generated_config": {
        const p = find(state, String(args.id));
        const files: GeneratedFile[] = [
          { path: `~/.config/keybund/git/${p.id}.gitconfig`, content: renderProfileGitconfig(p), excerpt: false },
        ];
        if (p.folders.length) files.push({ path: "~/.config/keybund/git/index.gitconfig", content: includeIfBlocks(p), excerpt: true });
        const ssh = sshHostBlocks(p);
        if (ssh) files.push({ path: "~/.ssh/config.d/keybund.conf", content: ssh, excerpt: true });
        return files;
      }
      case "paths": {
        const result: Paths = {
          profiles_file: "~/.config/keybund/profiles.toml",
          ssh_config: "~/.ssh/config",
          ssh_keybund_conf: "~/.ssh/config.d/keybund.conf",
          gitconfig: "~/.gitconfig",
          index_gitconfig: "~/.config/keybund/git/index.gitconfig",
        };
        return result;
      }
      case "import_proposals":
        return proposals(state);
      case "import_profiles": {
        const chosen = args.chosen as ImportProposal[];
        const added = chosen.map((c) => ({ ...structuredClone(c.profile), folders: c.profile.folders.map(normalizeFolder) }));
        const profiles = [...state.profiles, ...added];
        validate(profiles);
        state.profiles = profiles;
        const def = chosen.find((c) => c.make_default);
        if (def) state.defaultProfile = def.profile.id;
        return added.map((p) => p.id);
      }
      case "cleanup_preview":
        return cleanupPlan(state);
      case "cleanup_apply": {
        const plan = cleanupPlan(state);
        if (JSON.stringify(plan.changes.map((c) => c.diff)) !== JSON.stringify(args.expectedDiffs))
          throw new MockError("stale_plan", "Die Konfiguration hat sich seit der Vorschau geändert: Bitte Vorschau neu laden");
        state.sshBlocks = state.sshBlocks.filter((b) => !covered(state, b));
        const report: ApplyReport = { written: ["~/.ssh/config"], deleted: [], backups: ["~/.ssh/config.keybund-backup"] };
        return report;
      }
      case "uninstall_preview":
      case "uninstall_apply": {
        const opts = args.opts as UninstallOptions;
        const files: FileChange[] = [];
        if (state.installed) {
          files.push({ ...lineDiff("~/.ssh/config", renderSshConfig(state.sshBlocks), renderSshConfig(state.sshBlocks).replace("Include ~/.ssh/config.d/keybund.conf\n\n", "")), user_file: true });
          for (const [path, content] of state.applied) files.push(lineDiff(path, content, undefined));
        }
        if (opts.purge) files.push(lineDiff("~/.config/keybund/profiles.toml", "default_profile = …\n", undefined));
        const plan: Plan = { changes: files };
        if (cmd === "uninstall_preview") return { plan, backups_available: true } satisfies UninstallPreview;
        if (JSON.stringify(plan.changes.map((c) => c.diff)) !== JSON.stringify(args.expectedDiffs))
          throw new MockError("stale_plan", "Die Konfiguration hat sich seit der Vorschau geändert: Bitte Vorschau neu laden");
        state.installed = false;
        state.applied = new Map();
        if (opts.purge) {
          state.profiles = [];
          state.defaultProfile = null;
        }
        const report: ApplyReport = { written: ["~/.ssh/config"], deleted: files.slice(1).map((f) => f.path), backups: [] };
        return report;
      }
      case "plugin:app|version":
        return "0.1.0";
      case "plugin:dialog|open":
        return "~/dev/neues-projekt";
      default:
        throw new MockError("other", `Unbekannter Befehl: ${cmd}`);
    }
  }

  /** Same contract as Tauri: rejects with the serialized error. */
  async function invoke(cmd: string, args: Args = {}): Promise<unknown> {
    try {
      return await handle(cmd, args);
    } catch (e) {
      if (e instanceof MockError) throw { kind: e.kind, message: e.message };
      throw e;
    }
  }

  return { state, invoke };
}

export type MockBackend = ReturnType<typeof createMockBackend>;
