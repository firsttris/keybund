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
  FileChange,
  Fix,
  GeneratedFile,
  GlobalIdentity,
  HostEntry,
  HostKeyCheck,
  ImportCandidate,
  ImportDone,
  ImportProposal,
  InstallResult,
  KeyInfo,
  LastTest,
  Msg,
  NewProject,
  Paths,
  Plan,
  Platform,
  ProbeResult,
  Profile,
  ProfileDefaults,
  ProfilesView,
  ProfileView,
  Provider,
  Report,
  Settings,
  UninstallOptions,
  UninstallPreview,
  WhichResult,
} from "../bindings";
import { slugify } from "../utils";

type Args = Record<string, unknown>;

/** A message as the Rust side sends it: a catalog key and its arguments. */
export function msg(key: string, args: Record<string, string | number | Msg> = {}): Msg {
  return {
    key,
    args: Object.fromEntries(Object.entries(args).map(([k, v]) => [k, typeof v === "number" ? String(v) : v])),
  };
}

class MockError {
  constructor(
    public kind: string,
    public message: Msg,
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
  /** `hostname:keypath` pairs the user pasted into git accounts (by hand). */
  accountKeys: string[];
  settings: Settings;
  /** Key paths loaded in ssh-agent (`null`: no agent). */
  agentKeys: string[] | null;
  /** When the configs were last written (ms). */
  lastApplied: number | null;
  /** `user.name` / `user.email` from ~/.gitconfig. */
  globalIdentity: GlobalIdentity;
  /** libssh version of an installed Dolphin (`null`: no Dolphin). */
  dolphinLibssh: string | null;
  /** The system the backend runs on (`Env::platform`). */
  platform: Platform;
  /** known_hosts names (`host` or `[host]:port`) whose key is stored. */
  knownHosts: string[];
  /** profiles.toml cannot be parsed: the detail of the `config_parse` error every load fails with. */
  configError: string | null;
  /** `host_key_check` fails with this error kind (`tool_missing`, `io`, …). */
  hostKeyError: string | null;
  /** Writing the configs fails with this detail: profiles.toml is saved, the configs are not. */
  applyError: string | null;
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
      hosts: [
        host("github.com", "github"),
        host("1.2.3.4", "generic", { alias: "meinserver", user: "tristan", port: 22 }),
      ],
    }),
    profile({
      id: "acme",
      name: "Acme",
      color: "blue",
      key: { path: "~/.ssh/acme_ed25519", comment: "tristan@zentrale-acme" },
      git: { name: "Tristan", email: "tristan@kunde.de", sign: false },
      folders: ["~/dev/acme/"],
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
      id: "altprojekt",
      name: "Altprojekt",
      color: "amber",
      key: { path: "~/.ssh/archive/altprojekt_ed25519", comment: null },
      git: { name: "Tristan", email: "tristan@altprojekt.de", sign: false },
      folders: ["~/dev/altprojekt/"],
      archived: true,
      hosts: [host("github.com", "github")],
    }),
  ];
  const state: MockState = {
    defaultProfile: "privat",
    profiles,
    keys: new Map([
      ["~/.ssh/privat_ed25519", { ageDays: 420, comment: "tristan@zentrale" }],
      ["~/.ssh/acme_ed25519", { ageDays: 221, comment: "tristan@zentrale-acme" }],
      ["~/.ssh/kunde-b_ed25519", { ageDays: 35, comment: "tristan@zentrale-kunde-b" }],
      ["~/.ssh/archive/altprojekt_ed25519", { ageDays: 900, comment: "tristan@altprojekt" }],
      ["~/.ssh/id_ed25519", { ageDays: 1300, comment: "tristan@laptop" }],
      ["~/.ssh/nebenjob/id_ed25519", { ageDays: 500, comment: "tristan@nebenjob" }],
      ["~/.ssh/nordlicht-media_ed25519", { ageDays: 600, comment: "tristan@nordlicht-media" }],
    ]),
    applied: new Map(),
    lastTests: new Map([
      [
        "acme/gitlab.kunde.de",
        {
          host: "gitlab.kunde.de",
          success: true,
          account: "@tristan",
          message: msg("probe_connected", { account: "@tristan" }),
          at: Date.now() - 120_000,
        },
      ],
    ]),
    doctorIncludeOnTop: false,
    openedUrls: [],
    openFails: false,
    openTabs: false,
    clipboard: null,
    cloned: [],
    latency: 300,
    sshBlocks: [
      {
        pattern: "nebenjob",
        hostname: "github.com",
        identity: "~/.ssh/nebenjob/id_ed25519",
        comment: "Nebenjob Account",
      },
      {
        pattern: "gitlab.nordlicht.de",
        hostname: "gitlab.nordlicht.de",
        identity: "~/.ssh/nordlicht-media_ed25519",
        comment: "Nordlicht-Media",
      },
    ],
    otherKeyFor: null,
    installed: true,
    installedKeys: [],
    accountKeys: [],
    knownHosts: ["1.2.3.4", "gitlab.kunde.de"],
    dolphinLibssh: null,
    platform: "linux",
    agentKeys: ["~/.ssh/privat_ed25519", "~/.ssh/acme_ed25519"],
    lastApplied: Date.now() - 3 * 3_600_000,
    globalIdentity: { name: "Tristan Teufel", email: "tristanteufel@example.org" },
    settings: { projects_dir: "~/dev", keys_dir: "~/.ssh", absolute_ssh_include: false, known_hosts_per_profile: true },
    unreachable: [],
    configError: null,
    hostKeyError: null,
    applyError: null,
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
  let out = `${MANAGED}\n# Key: ${p.name}\n`;
  // Empty name or e-mail: git keeps the one from ~/.gitconfig.
  if (p.git && (p.git.name.trim() || p.git.email.trim() || p.git.sign)) {
    out += "[user]\n";
    if (p.git.name.trim()) out += `\tname = ${p.git.name}\n`;
    if (p.git.email.trim()) out += `\temail = ${p.git.email}\n`;
    if (p.git.sign) out += `\tsigningkey = ${p.key.path}.pub\n`;
  }
  out += `[core]\n\tsshCommand = ${sshCommand(p.key.path)}\n`;
  if (p.git?.sign)
    out +=
      '[gpg]\n\tformat = ssh\n[commit]\n\tgpgsign = true\n[gpg "ssh"]\n\tallowedSignersFile = ~/.config/keybund/allowed_signers\n';
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

function sshHostBlocks(p: Profile, settings?: Settings): string {
  return p.hosts
    .filter((h) => sshPatterns(h).length > 0)
    .map((h) => {
      let block = `# Key: ${p.name}\nHost ${sshPatterns(h).join(" ")}\n    HostName ${h.hostname}\n`;
      const user = h.user ?? (h.provider === "generic" ? null : "git");
      if (user) block += `    User ${user}\n`;
      if (h.port != null) block += `    Port ${h.port}\n`;
      block += `    IdentityFile ${p.key.path}\n    IdentitiesOnly yes\n    AddKeysToAgent yes\n`;
      if (settings?.known_hosts_per_profile && h.provider === "generic") {
        block += `    UserKnownHostsFile ~/.ssh/known_hosts.d/${p.id} ~/.ssh/known_hosts\n    UpdateHostKeys yes\n`;
      }
      return block;
    })
    .join("\n");
}

function markApplied(state: MockState) {
  state.applied = renderAll(state);
  state.lastApplied = Date.now();
}

/** Like `save_and_apply`: profiles.toml is already saved when writing the configs fails. */
function applyConfigs(state: MockState) {
  if (state.applyError !== null) {
    throw new MockError("io", msg("error_io", { path: "~/.ssh/config.d/keybund.conf", detail: state.applyError }));
  }
  markApplied(state);
}

function renderAll(state: MockState, profiles = state.profiles, def = state.defaultProfile): Map<string, string> {
  const active = profiles.filter((p) => !p.archived);
  const files = new Map<string, string>();
  const ssh = active.map((p) => sshHostBlocks(p, state.settings)).filter(Boolean);
  files.set("~/.ssh/config.d/keybund.conf", `${MANAGED}\n${ssh.map((b) => `\n${b}`).join("")}\nHost *\n`);
  let index = `${MANAGED}\n# Included from ~/.gitconfig.\n`;
  const defProfile = active.find((p) => p.id === def);
  if (defProfile)
    index += `\n# Default key: ${defProfile.name}\n[core]\n\tsshCommand = ${sshCommand(defProfile.key.path)}\n`;
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
  const body = [...a.map((l) => (b.includes(l) ? ` ${l}` : `-${l}`)), ...added.map((l) => `+${l}`)];
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
      const file = b.identity
        .split("/")
        .pop()!
        .replace(/_(ed25519|rsa|ecdsa)$/, "");
      let id = file === "id" || file.startsWith("id_") ? slugify(b.pattern) : slugify(file);
      for (let n = 2; taken.has(id); n++) id = `${id.replace(/-\d+$/, "")}-${n}`;
      taken.add(id);
      const provider = detectProvider(b.hostname);
      const plainGit = b.pattern === b.hostname && provider !== "generic";
      const asDefault = plainGit && !defaultGiven;
      if (asDefault) defaultGiven = true;
      // A shared git host (github.com …) never gets a Host block of its own (mirrors import.rs shared_host).
      const shared =
        ["github.com", "gitlab.com", "bitbucket.org"].includes(b.hostname) ||
        state.profiles.some((p) => p.hosts.some((h) => h.hostname === b.hostname));
      const noAlias = asDefault || (plainGit && shared) || (b.pattern === b.hostname && provider === "generic");
      return {
        profile: profile({
          id,
          name: b.comment ?? b.pattern,
          key: { path: b.identity, comment: null },
          hosts: [host(b.hostname, provider, { alias: noAlias ? null : b.pattern })],
        }),
        make_default: asDefault,
        blocks: [b.pattern],
        key_exists: state.keys.has(b.identity),
        notes: asDefault ? [msg("import_becomesDefault", { host: b.hostname })] : [],
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
  const shared = state.profiles
    .filter((p) => !p.archived)
    .flatMap((p) => p.hosts.filter((h) => !h.alias).map((h) => h.hostname));
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
            ? msg("doctor_sshConfig_old", { host: b.pattern })
            : msg("doctor_sshConfig_duplicate", { host: b.pattern })
          : msg("doctor_sshConfig_global", { host: b.pattern }),
        detail: isCovered
          ? mixes
            ? msg("doctor_sshConfig_old_detail", { key: b.identity })
            : msg("doctor_sshConfig_duplicate_detail")
          : msg("doctor_sshConfig_global_detail", { key: b.identity }),
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

const PINNED_HOSTS = ["github.com", "gitlab.com", "bitbucket.org"];

function knownName(h: HostEntry): string {
  return h.port && h.port !== 22 ? `[${h.hostname}]:${h.port}` : h.hostname;
}

/** A stable fake fingerprint per host. */
function hostFingerprint(h: HostEntry): string {
  return `SHA256:${fakeKey(`hostkey-${knownName(h)}`).fingerprint.slice(7)}`;
}

function hostOf(state: MockState, id: string, host: string | null | undefined): { p: Profile; h: HostEntry } {
  const p = find(state, id);
  const h = host ? p.hosts.find((x) => x.alias === host || x.hostname === host) : p.hosts[0];
  if (!h)
    throw new MockError(
      "no_host",
      host ? msg("error_noSuchHost", { id: p.id, host }) : msg("error_noHost", { id: p.id }),
    );
  return { p, h };
}

function hostKeyKnown(state: MockState, h: HostEntry): boolean {
  return state.knownHosts.includes(knownName(h));
}

/** The error `host_key_check` fails with when a test asks for one (`state.hostKeyError`). */
function hostKeyFailure(kind: string, h: HostEntry): MockError {
  switch (kind) {
    case "tool_missing":
      return new MockError(kind, msg("error_toolMissing", { tool: "ssh-keyscan" }));
    case "io":
      return new MockError(kind, msg("error_io", { path: "~/.ssh/known_hosts", detail: "Permission denied" }));
    case "network":
      return new MockError(kind, msg("error_network", { host: h.hostname, detail: "Connection timed out" }));
    default:
      return new MockError(kind, msg("error_other", { detail: kind }));
  }
}

/** `history::host_tests`: the last test of each current host, in host order. */
export function hostTests(state: MockState, p: Profile): LastTest[] {
  return p.hosts.flatMap((h) => {
    const t = state.lastTests.get(`${p.id}/${h.alias ?? h.hostname}`);
    return t ? [t] : [];
  });
}

/** `history::summary`: the latest failing host, else the latest test, else an old per-key entry. */
export function testSummary(state: MockState, p: Profile): LastTest | null {
  const tests = hostTests(state, p);
  const latest = (list: LastTest[]) =>
    list.reduce<LastTest | null>((a, b) => (!a || (b.at ?? 0) > (a.at ?? 0) ? b : a), null);
  return latest(tests.filter((t) => !t.success)) ?? latest(tests) ?? state.lastTests.get(p.id) ?? null;
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
    last_test: testSummary(state, p),
    host_tests: hostTests(state, p),
    in_agent: state.agentKeys ? state.agentKeys.includes(p.key.path) : null,
  };
}

function find(state: MockState, id: string): Profile {
  const p = state.profiles.find((x) => x.id === id);
  if (!p) throw new MockError("profile_not_found", msg("error_profileNotFound", { id }));
  return p;
}

// --- paths and validation (mirror crates/core/src/store.rs) -----------------

/** The home of the seeded user (key comments say tristan@zentrale). */
export const HOME = "/home/tristan";

/** `Env::expand`: `~` and `~/…` become absolute. */
function expand(path: string): string {
  if (path === "~") return HOME;
  return path.startsWith("~/") ? `${HOME}/${path.slice(2)}` : path;
}

/** `Env::contract`: the home prefix becomes `~` again. */
function contract(path: string): string {
  if (path === HOME) return "~";
  return path.startsWith(`${HOME}/`) ? `~/${path.slice(HOME.length + 1)}` : path;
}

/** `store::lexical`: `.` and `..` resolved without the file system, `//` collapsed. */
function lexical(path: string): string {
  const absolute = path.startsWith("/");
  const out: string[] = [];
  for (const part of path.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      if (out.length > 0 && out[out.length - 1] !== "..") out.pop();
      else if (!absolute) out.push("..");
      continue;
    }
    out.push(part);
  }
  return `${absolute ? "/" : ""}${out.join("/")}` || (absolute ? "/" : ".");
}

/**
 * `store::normalize_folder`: expanded, lexically cleaned, written with `~`
 * and a trailing `/` (without it gitdir: would not match subfolders). The
 * real one also resolves symlinks of the existing part.
 */
export function normalizeFolder(f: string): string {
  const t = f.trim();
  const s = contract(lexical(expand(t)));
  return s.endsWith("/") ? s : `${s}/`;
}

/** `store::normalize_dir`: a settings folder, absolute or below `~`, without a trailing `/`. */
function normalizeDir(input: string): string {
  const t = input.trim();
  if (!(t.startsWith("/") || t === "~" || t.startsWith("~/"))) {
    throw new MockError("invalid_path", msg("error_invalidPath", { path: t }));
  }
  const s = contract(expand(t));
  return s.length > 1 ? s.replace(/\/+$/, "") : s;
}

/** `store::is_valid_id`: a–z, 0–9, - and _, at most 64 characters. */
export function isValidId(id: string): boolean {
  return id.length > 0 && id.length <= 64 && /^[a-z0-9_-]+$/.test(id);
}

/** `char::is_control`: C0 and C1 control characters (and DEL). */
const hasControl = (v: string) =>
  [...v].some((c) => c.charCodeAt(0) < 0x20 || (c.charCodeAt(0) >= 0x7f && c.charCodeAt(0) < 0xa0));
const invalidValue = (field: string, value: string) =>
  new MockError("invalid_value", msg("error_invalidValue", { value, field: msg(`field_${field}`) }));

/** `store::check_values`: what ends up verbatim in ssh and git configs. */
function checkValues(p: Profile) {
  const printable = (field: string, value: string) => {
    if (hasControl(value)) throw invalidValue(field, value);
  };
  // A token for `Host` / `HostName` / `User`: no spaces, wildcards, quotes or leading '-'.
  const token = (field: string, value: string) => {
    if (value === "" || value.startsWith("-") || hasControl(value) || /[\s*?!,"'#%[\]]/.test(value)) {
      throw invalidValue(field, value);
    }
  };
  printable("name", p.name);
  printable("key", p.key.path);
  // ssh expands % and ${…} in IdentityFile, a quote would end the quoted path, a backslash is swallowed.
  if (/["%\\]/.test(p.key.path) || p.key.path.includes("${")) throw invalidValue("key", p.key.path);
  // ssh resolves a relative IdentityFile against the directory it runs in.
  if (p.key.path && !(p.key.path.startsWith("/") || p.key.path.startsWith("~/")))
    throw new MockError("invalid_path", msg("error_invalidPath", { path: p.key.path }));
  if (p.git) {
    printable("gitName", p.git.name);
    printable("gitEmail", p.git.email);
    // The e-mail is the principal in allowed_signers, a pattern line of its own.
    if (p.git.sign && p.git.email.trim() && /[\s,*?!]/.test(p.git.email)) throw invalidValue("gitEmail", p.git.email);
  }
  for (const f of p.folders) {
    printable("folder", f);
    // gitdir: patterns are globs; git compares the real path, so `..` would never match.
    const dotted = f.split("/").some((c) => c === "." || c === "..");
    if (/["*?[\\]/.test(f) || !(f.startsWith("/") || f.startsWith("~/")) || dotted) {
      throw new MockError("invalid_path", msg("error_invalidPath", { path: f }));
    }
  }
  const seen = new Set<string>();
  for (const h of p.hosts) {
    token("hostname", h.hostname);
    if (h.alias != null) token("alias", h.alias);
    if (h.user != null) token("user", h.user);
    // Port 0 is no port: ssh would silently use 22.
    if (h.port === 0) throw invalidValue("port", "0");
    // The same account or server twice in one key: two identical blocks, two identical rows.
    const same = `${h.hostname.toLowerCase()}\u0000${h.alias?.toLowerCase() ?? ""}`;
    if (seen.has(same)) throw new MockError("duplicate_host", msg("error_duplicateHost", { hostname: h.hostname }));
    seen.add(same);
  }
}

/** `store::validate` (+ `validate_in`), in the same order. */
function validate(profiles: Profile[], defaultProfile: string | null = null) {
  const ids = new Set<string>();
  const aliases = new Set<string>();
  for (const p of profiles) {
    if (!isValidId(p.id)) throw new MockError("invalid_id", msg("error_invalidId", { id: p.id }));
    if (ids.has(p.id)) throw new MockError("duplicate_id", msg("error_duplicateId", { id: p.id }));
    ids.add(p.id);
    if (!p.name.trim()) throw new MockError("missing_name", msg("error_missingName", { id: p.id }));
    checkValues(p);
    if (p.git?.sign && !p.git.email.trim())
      throw new MockError("sign_without_email", msg("error_signWithoutEmail", { id: p.id }));
    if (p.archived) continue;
    for (const h of p.hosts) {
      for (const pattern of sshPatterns(h)) {
        // ssh matches Host patterns case-sensitively, but two keys for `NAS` and `nas` would only confuse.
        if (aliases.has(pattern.toLowerCase()))
          throw new MockError("duplicate_alias", msg("error_duplicateAlias", { host: pattern }));
        aliases.add(pattern.toLowerCase());
      }
    }
  }
  if (defaultProfile !== null && !profiles.some((p) => p.id === defaultProfile && !p.archived))
    throw new MockError("default_profile_missing", msg("error_defaultProfileMissing", { id: defaultProfile }));
  // Folders must not be nested, compared as real paths: includeIf would apply both.
  const slashed = (f: string) => (f.endsWith("/") ? f : `${f}/`);
  const folders = profiles.filter((p) => !p.archived).flatMap((p) => p.folders);
  for (let i = 0; i < folders.length; i++)
    for (let j = i + 1; j < folders.length; j++) {
      const [a, b] = [slashed(expand(folders[i])), slashed(expand(folders[j]))];
      if (a.startsWith(b) || b.startsWith(a))
        throw new MockError("nested_folders", msg("error_nestedFolders", { a: folders[i], b: folders[j] }));
    }
}

/**
 * Like `api::normalized`: trimmed values, empty optionals dropped, folders
 * normalized. Host names are stored in lower case so `ssh nas` and
 * `ssh NAS.local` agree.
 */
function normalized(profile: Profile): Profile {
  const p = structuredClone(profile);
  p.id = p.id.trim();
  p.name = p.name.trim();
  p.folders = p.folders.filter((f) => f.trim()).map(normalizeFolder);
  for (const h of p.hosts) {
    h.hostname = h.hostname.trim().toLowerCase();
    h.alias = h.alias?.trim().toLowerCase() || null;
    h.user = h.user?.trim() || null;
  }
  return p;
}

/** `api::with_new_profile`: the profiles and the default key as the new project would leave them. */
function withNewProject(state: MockState, project: NewProject): { profiles: Profile[]; def: string | null } {
  if (state.profiles.some((p) => p.id === project.profile.id))
    throw new MockError("duplicate_id", msg("error_duplicateId", { id: project.profile.id }));
  const p = normalized(project.profile);
  if (project.key.type === "existing") p.key = { path: contract(expand(project.key.path)), comment: null };
  const profiles = [...state.profiles, p];
  // Only on request: a standard key replaces ssh's own key choice for every repo outside the git folders.
  const def = project.make_default ? p.id : state.defaultProfile;
  validate(profiles, def);
  return { profiles, def };
}

function doctorReport(state: MockState): Report {
  const plan = planFor(state);
  const checks: Check[] = [
    state.doctorIncludeOnTop
      ? { id: "ssh-include", status: "ok", title: msg("doctor_sshInclude_ok"), detail: null, fix: null }
      : {
          id: "ssh-include",
          status: "error",
          title: msg("doctor_sshInclude_notTop"),
          detail: msg("doctor_sshInclude_notTop_detail"),
          fix: { type: "move_ssh_include" },
        },
    { id: "ssh-dir-perms", status: "ok", title: msg("doctor_sshDir_ok", { mode: "700" }), detail: null, fix: null },
    { id: "key-perms", status: "ok", title: msg("doctor_keyPerms_ok"), detail: null, fix: null },
    { id: "git-env", status: "ok", title: msg("doctor_gitEnv_ok"), detail: null, fix: null },
    { id: "agent", status: "ok", title: msg("doctor_agent_ok"), detail: msg("doctor_agent_keys", { n: 3 }), fix: null },
    { id: "add-keys", status: "ok", title: msg("doctor_addKeys_ok"), detail: null, fix: null },
    plan.changes.length === 0
      ? { id: "configs", status: "ok", title: msg("doctor_configs_ok"), detail: null, fix: null }
      : {
          id: "configs",
          status: "warn",
          title: msg("doctor_configs_stale", { n: plan.changes.length }),
          detail: msg("common_text", { text: plan.changes.map((c) => c.path).join(", ") }),
          fix: { type: "apply" },
        },
    ...sshConfigChecks(state),
    ...dolphinChecks(state),
  ];
  return { checks };
}

function dolphinChecks(state: MockState): Check[] {
  const v = state.dolphinLibssh;
  if (!v) return [];
  const [, minor, patch] = v.split(".").map(Number);
  const ok = minor > 10 || (minor === 10 && patch >= 2) || state.settings.absolute_ssh_include;
  return [
    ok
      ? { id: "dolphin", status: "ok", title: msg("doctor_dolphin_ok", { version: v }), detail: null, fix: null }
      : {
          id: "dolphin",
          status: "warn",
          title: msg("doctor_dolphin_old", { version: v }),
          detail: msg("doctor_dolphin_old_detail"),
          fix: { type: "absolute_ssh_include" },
        },
  ];
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
    if (!p) return { ...base, notes: [msg("which_noKey")] };
    return {
      ...base,
      key_path: p.key.path,
      fingerprint: keyInfo(state, p.key.path).fingerprint,
      profile_id: p.id,
      source: match ? `~/.config/keybund/git/${p.id}.gitconfig` : "~/.config/keybund/git/index.gitconfig",
      via: match ? msg("common_text", { text: `includeIf "gitdir:${match.f}"` }) : msg("which_viaDefault"),
      identities_only: true,
      git_name: p.git?.name ?? null,
      git_email: p.git?.email ?? null,
      agent_loaded: true,
      notes: match ? [] : [msg("which_defaultApplies")],
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
      via: msg("common_text", { text: `Host ${q}` }),
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
      ? [msg("which_folderDecides", { host: q, names: users.map((p) => p.name).join(", ") })]
      : [msg("which_unknownHost")],
  };
}

// --- dispatcher ---------------------------------------------------------------

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export function createMockBackend(state: MockState = seedState()) {
  function openUrl(url: string) {
    if (!url.startsWith("https://")) throw new MockError("not_https", msg("error_notHttps", { url }));
    if (state.openFails) throw new MockError("tool_missing", msg("error_toolMissing", { tool: "xdg-open" }));
    state.openedUrls.push(url);
    if (state.openTabs) window.open(url, "_blank", "noopener");
  }

  async function handle(cmd: string, args: Args): Promise<unknown> {
    // Every command starts with `store::load`: a broken profiles.toml fails all of them.
    if (state.configError !== null && !cmd.startsWith("plugin:")) {
      throw new MockError("config_parse", msg("error_configParse", { detail: state.configError }));
    }
    switch (cmd) {
      case "load_profiles": {
        const result: ProfilesView = {
          default_profile: state.defaultProfile,
          profiles: state.profiles.map((p) => view(state, p)),
          pending_changes: planFor(state).changes.length,
          import_available: proposals(state).length,
          last_applied: state.lastApplied,
          global_identity: structuredClone(state.globalIdentity),
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
        next.projects_dir = normalizeDir(next.projects_dir);
        next.keys_dir = normalizeDir(next.keys_dir);
        state.settings = next;
        applyConfigs(state);
        return structuredClone(next);
      }
      case "normalize_folder":
        return normalizeFolder(String(args.folder));
      case "preview_new_project": {
        const { profiles, def } = withNewProject(state, args.project as NewProject);
        return planFor(state, profiles, def);
      }
      case "create_project": {
        await sleep(state.latency);
        const project = args.project as NewProject;
        const { profiles, def } = withNewProject(state, project);
        const p = profiles[profiles.length - 1];
        if (project.key.type === "generate") {
          if (state.keys.has(p.key.path))
            throw new MockError("key_exists", msg("error_keyExists", { path: p.key.path }));
          state.keys.set(p.key.path, { ageDays: 0, comment: project.key.comment });
          p.key.comment = project.key.comment;
        } else if (!state.keys.has(p.key.path)) {
          throw new MockError("key_missing", msg("error_keyMissing", { path: p.key.path }));
        }
        state.profiles = profiles;
        state.defaultProfile = def;
        const plan = planFor(state);
        applyConfigs(state);
        const v = view(state, p);
        state.clipboard = v.key.public_key ?? null;
        let openError: Msg | null = null;
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
        await sleep(state.latency);
        const next = normalized(args.profile as Profile);
        const profiles = state.profiles.map((p) => (p.id === next.id ? next : p));
        find(state, next.id);
        validate(profiles, state.defaultProfile);
        state.profiles = profiles;
        applyConfigs(state);
        return view(state, next);
      }
      case "set_default_profile": {
        const id = (args.id as string | null) ?? null;
        if (id && !state.profiles.some((p) => p.id === id && !p.archived))
          throw new MockError("default_profile_missing", msg("error_defaultProfileMissing", { id }));
        state.defaultProfile = id;
        applyConfigs(state);
        return null;
      }
      case "copy_public_key": {
        const key = keyInfo(state, find(state, String(args.id)).key.path).public_key;
        if (!key)
          throw new MockError("key_missing", msg("error_keyMissing", { path: find(state, String(args.id)).key.path }));
        state.clipboard = key;
        return key;
      }
      case "open_url":
        openUrl(String(args.url));
        return null;
      case "test_connection": {
        await sleep(state.latency);
        const p = find(state, String(args.id));
        // `history::record`: per host, every result; an old per-key entry gives way.
        const remember = (result: ProbeResult) => {
          state.lastTests.delete(p.id);
          state.lastTests.set(`${p.id}/${result.host}`, {
            host: result.host,
            success: result.success,
            account: result.account,
            message: result.message,
            at: Date.now(),
          });
          return result;
        };
        const wanted = args.host as string | null | undefined;
        const h = wanted ? p.hosts.find((x) => x.alias === wanted || x.hostname === wanted) : p.hosts[0];
        if (!h)
          throw new MockError(
            "no_host",
            wanted ? msg("error_noSuchHost", { id: p.id, host: wanted }) : msg("error_noHost", { id: p.id }),
          );
        // Like StrictHostKeyChecking=yes: no connection to an unconfirmed host.
        const pinned = PINNED_HOSTS.includes(h.hostname) && (!h.port || h.port === 22);
        const unconfirmed = !hostKeyKnown(state, h) && !pinned && !state.unreachable.includes(h.hostname);
        if (unconfirmed) {
          return remember({
            success: false,
            host: h.alias ?? h.hostname,
            account: null,
            message: msg("probe_hostKeyUnconfirmed", { host: h.hostname }),
            output: "Host key verification failed.",
            command: `ssh -T -o StrictHostKeyChecking=yes ${h.alias ?? h.hostname}`,
            used_key: null,
            other_key: false,
            key_rejected: false,
            steps: [
              { ok: null, text: msg("step_connecting", { host: h.hostname, port: h.port ?? 22 }) },
              { ok: false, text: msg("step_hostKeyFailed") },
            ],
            log: "Host key verification failed.",
          });
        }
        // Kunde B's key is not in its GitHub account until it is uploaded.
        const where = `${h.hostname}:${p.key.path}`;
        const registered =
          p.id !== "kunde-b" || state.accountKeys.includes(where) || state.installedKeys.includes(where);
        const ok = registered && !state.unreachable.includes(h.hostname);
        const user = h.user ?? (h.provider === "generic" ? "tristan" : "git");
        const steps: ProbeResult["steps"] = state.unreachable.includes(h.hostname)
          ? [
              { ok: null, text: msg("step_connecting", { host: h.hostname, port: h.port ?? 22 }) },
              { ok: false, text: msg("step_noConnection", { reason: "Connection timed out" }) },
            ]
          : [
              ...(h.alias
                ? [{ ok: null, text: msg("step_config", { file: "~/.ssh/config.d/keybund.conf", host: h.alias }) }]
                : []),
              { ok: null, text: msg("step_connecting", { host: h.hostname, port: h.port ?? 22 }) },
              { ok: true, text: msg("step_connected") },
              { ok: null, text: msg("step_user", { user }) },
              { ok: true, text: msg("step_hostKeyOk") },
              { ok: null, text: msg("step_offer", { key: p.key.path }) },
              ok
                ? { ok: true, text: msg("step_accepted", { key: p.key.path }) }
                : { ok: false, text: msg("step_rejected") },
              ok
                ? { ok: true, text: msg("step_loggedIn") }
                : {
                    ok: false,
                    text: msg("step_server", { line: `${user}@${h.hostname}: Permission denied (publickey).` }),
                  },
            ];
        const result: ProbeResult = {
          success: ok,
          host: h.alias ?? h.hostname,
          account: ok && h.provider !== "generic" ? "@tristan" : null,
          message: ok
            ? h.provider === "generic"
              ? msg("probe_loggedIn")
              : msg("probe_connected", { account: "@tristan" })
            : h.provider === "generic"
              ? msg("probe_keyUnknown_server")
              : msg("probe_keyUnknown", { provider: PROVIDER_NAME[h.provider] }),
          output: ok ? "Hi tristan! You've successfully authenticated." : "Permission denied (publickey).",
          command: `ssh -T -o PasswordAuthentication=no -o IdentitiesOnly=yes -o AddKeysToAgent=yes -i ${p.key.path} ${h.alias ?? h.hostname}`,
          used_key: ok ? p.key.path : null,
          other_key: false,
          key_rejected: !registered && !state.unreachable.includes(h.hostname),
          steps,
          log: [
            `OpenSSH_9.6p1`,
            `debug1: Connecting to ${h.hostname} port ${h.port ?? 22}.`,
            ...steps.map((s) => `debug1: ${s.text.key}`),
          ].join("\n"),
        };
        if (state.otherKeyFor === p.id) {
          Object.assign(result, {
            success: false,
            other_key: true,
            key_rejected: true,
            used_key: "~/.ssh/privat_ed25519",
            account: "@firsttris",
            message: msg("probe_otherKey_as", { account: "@firsttris", key: "~/.ssh/privat_ed25519" }),
          });
        }
        return remember(result);
      }
      case "install_key": {
        await sleep(state.latency);
        const p = find(state, String(args.id));
        const name = String(args.host);
        const h = p.hosts.find((x) => x.alias === name || x.hostname === name);
        if (!h) throw new MockError("no_host", msg("error_noSuchHost", { id: p.id, host: name }));
        const key = `${h.hostname}:${p.key.path}`;
        if (!hostKeyKnown(state, h) && !state.unreachable.includes(h.hostname)) {
          return {
            success: false,
            already_present: false,
            message: msg("probe_hostKeyUnconfirmed", { host: h.hostname }),
            output: "Host key verification failed.",
          } satisfies InstallResult;
        }
        if (state.unreachable.includes(h.hostname)) {
          return {
            success: false,
            already_present: false,
            message: msg("probe_unreachable", { host: name }),
            output: "ssh: connect to host … Connection timed out",
          } satisfies InstallResult;
        }
        const already = state.installedKeys.includes(key);
        if (!already) state.installedKeys.push(key);
        return {
          success: true,
          already_present: already,
          message: already ? msg("install_present", { host: name }) : msg("install_added", { host: name }),
          output: "",
        } satisfies InstallResult;
      }
      case "host_key_check": {
        // ssh-keyscan takes a moment; meanwhile a second test may start.
        await sleep(state.latency);
        const { p, h } = hostOf(state, String(args.id), args.host as string | null);
        if (state.hostKeyError !== null) throw hostKeyFailure(state.hostKeyError, h);
        if (hostKeyKnown(state, h)) return { state: "known" } satisfies HostKeyCheck;
        if (PINNED_HOSTS.includes(h.hostname) && (!h.port || h.port === 22)) {
          state.knownHosts.push(knownName(h));
          return { state: "pinned" } satisfies HostKeyCheck;
        }
        if (state.unreachable.includes(h.hostname))
          return { state: "unreachable", host: h.hostname } satisfies HostKeyCheck;
        return {
          state: "unknown",
          host: knownName(h),
          file:
            h.provider === "generic" && state.settings.known_hosts_per_profile
              ? `~/.ssh/known_hosts.d/${p.id}`
              : "~/.ssh/known_hosts",
          keys: [{ key_type: "ssh-ed25519", fingerprint: hostFingerprint(h) }],
        } satisfies HostKeyCheck;
      }
      case "trust_host_key": {
        const { h } = hostOf(state, String(args.id), args.host as string | null);
        if (!(args.fingerprints as string[]).includes(hostFingerprint(h))) {
          throw new MockError("host_key_changed", msg("error_hostKeyChanged", { host: h.hostname }));
        }
        state.knownHosts.push(knownName(h));
        return null;
      }
      case "preview_add_host":
      case "add_host": {
        if (cmd === "add_host") await sleep(state.latency);
        const p = find(state, String(args.id));
        const host = normalized({ ...profile({ id: p.id, name: p.name }), hosts: [args.host as HostEntry] }).hosts[0];
        const next = { ...structuredClone(p), hosts: [...p.hosts, host] };
        const profiles = state.profiles.map((x) => (x.id === p.id ? next : x));
        validate(profiles, state.defaultProfile);
        if (cmd === "preview_add_host") return planFor(state, profiles);
        state.profiles = profiles;
        applyConfigs(state);
        return view(state, next);
      }
      case "last_test":
        return testSummary(state, find(state, String(args.id)));
      case "plan_changes":
        return planFor(state);
      case "apply_changes": {
        const plan = planFor(state);
        const expected = args.expectedDiffs as string[];
        if (JSON.stringify(plan.changes.map((c) => c.diff)) !== JSON.stringify(expected))
          throw new MockError("stale_plan", msg("error_stalePlan", { path: plan.changes[0]?.path ?? "" }));
        applyConfigs(state);
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
        if (fix.type === "apply") markApplied(state);
        if (fix.type === "absolute_ssh_include") state.settings = { ...state.settings, absolute_ssh_include: true };
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
        if (!m) throw new MockError("invalid_url", msg("error_invalidUrl", { url }));
        const [, hostname, path] = m;
        const name = path
          .split("/")
          .pop()!
          .replace(/\.git$/, "");
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
        const name = String(args.url)
          .split(/[/:]/)
          .pop()!
          .replace(/\.git$/, "");
        const target = (args.target as string | null) || (p.folders[0] ? `${p.folders[0]}${name}` : null);
        if (!target) throw new MockError("no_folder", msg("error_noFolder", { id: p.id }));
        if (state.cloned.includes(target))
          throw new MockError("target_exists", msg("error_targetExists", { path: target }));
        state.cloned.push(target);
        return target;
      }
      case "archive_profile": {
        const p = find(state, String(args.id));
        if (p.archived) throw new MockError("archived", msg("error_archived", { id: p.id }));
        const file = p.key.path.split("/").pop()!;
        const newPath = `~/.ssh/archive/${file}`;
        // Another active key entry uses the same file: it stays where it is.
        const shared = state.profiles.some((x) => x.id !== p.id && !x.archived && x.key.path === p.key.path);
        if (!shared && state.keys.has(newPath))
          throw new MockError("key_exists", msg("error_keyExists", { path: newPath }));
        if (!shared) {
          const key = state.keys.get(p.key.path);
          state.keys.delete(p.key.path);
          if (key) state.keys.set(newPath, key);
          p.key = { ...p.key, path: newPath };
        }
        p.archived = true;
        if (state.defaultProfile === p.id) state.defaultProfile = null;
        // `history::forget`: an archived key has no test to show.
        for (const k of [...state.lastTests.keys()]) {
          if (k === p.id || k.startsWith(`${p.id}/`)) state.lastTests.delete(k);
        }
        applyConfigs(state);
        const result: ArchiveResult = {
          key_path: p.key.path,
          remove_key_urls: p.hosts.flatMap((h) =>
            h.provider === "github"
              ? [`https://${h.hostname}/settings/keys`]
              : ADD_KEY_URL[h.provider]
                ? [ADD_KEY_URL[h.provider]!(h.hostname)]
                : [],
          ),
        };
        return result;
      }
      case "generated_config": {
        const p = find(state, String(args.id));
        const files: GeneratedFile[] = [
          { path: `~/.config/keybund/git/${p.id}.gitconfig`, content: renderProfileGitconfig(p), excerpt: false },
        ];
        if (p.folders.length)
          files.push({ path: "~/.config/keybund/git/index.gitconfig", content: includeIfBlocks(p), excerpt: true });
        const ssh = sshHostBlocks(p, state.settings);
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
          platform: state.platform,
        };
        return result;
      }
      case "import_proposals":
        return proposals(state);
      case "import_profiles": {
        const chosen = args.chosen as ImportProposal[];
        const added = chosen.map((c) => ({
          ...structuredClone(c.profile),
          folders: c.profile.folders.map(normalizeFolder),
        }));
        const profiles = [...state.profiles, ...added];
        const def = chosen.find((c) => c.make_default);
        validate(profiles, def ? def.profile.id : state.defaultProfile);
        state.profiles = profiles;
        if (def) state.defaultProfile = def.profile.id;
        applyConfigs(state);
        const done: ImportDone = {
          ids: added.map((p) => p.id),
          report: { written: [...state.applied.keys()], deleted: [], backups: [] },
        };
        if (args.removeOld && cleanupPlan(state).changes.length) {
          state.sshBlocks = state.sshBlocks.filter((b) => !covered(state, b));
          done.report.written.push("~/.ssh/config");
          done.report.backups.push("~/.ssh/config.keybund-backup");
        }
        return done;
      }
      case "cleanup_preview":
        return cleanupPlan(state);
      case "cleanup_apply": {
        const plan = cleanupPlan(state);
        if (JSON.stringify(plan.changes.map((c) => c.diff)) !== JSON.stringify(args.expectedDiffs))
          throw new MockError("stale_plan", msg("error_stalePlan", { path: plan.changes[0]?.path ?? "" }));
        state.sshBlocks = state.sshBlocks.filter((b) => !covered(state, b));
        const report: ApplyReport = {
          written: ["~/.ssh/config"],
          deleted: [],
          backups: ["~/.ssh/config.keybund-backup"],
        };
        return report;
      }
      case "uninstall_preview":
      case "uninstall_apply": {
        const opts = args.opts as UninstallOptions;
        const files: FileChange[] = [];
        if (state.installed) {
          files.push({
            ...lineDiff(
              "~/.ssh/config",
              renderSshConfig(state.sshBlocks),
              renderSshConfig(state.sshBlocks).replace("Include ~/.ssh/config.d/keybund.conf\n\n", ""),
            ),
            user_file: true,
          });
          for (const [path, content] of state.applied) files.push(lineDiff(path, content, undefined));
        }
        if (opts.purge) files.push(lineDiff("~/.config/keybund/profiles.toml", "default_profile = …\n", undefined));
        const plan: Plan = { changes: files };
        if (cmd === "uninstall_preview") return { plan, backups_available: true } satisfies UninstallPreview;
        if (JSON.stringify(plan.changes.map((c) => c.diff)) !== JSON.stringify(args.expectedDiffs))
          throw new MockError("stale_plan", msg("error_stalePlan", { path: plan.changes[0]?.path ?? "" }));
        state.installed = false;
        state.applied = new Map();
        if (opts.purge) {
          state.profiles = [];
          state.defaultProfile = null;
        }
        const report: ApplyReport = {
          written: ["~/.ssh/config"],
          deleted: files.slice(1).map((f) => f.path),
          backups: [],
        };
        return report;
      }
      case "plugin:app|version":
        return "0.1.0";
      case "plugin:dialog|open":
        return "~/dev/neues-projekt";
      default:
        throw new MockError("other", msg("error_other", { detail: `unknown command: ${cmd}` }));
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
