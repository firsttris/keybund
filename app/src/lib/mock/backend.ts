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
  HostEntry,
  ImportCandidate,
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
  Report,
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

function sshHostBlocks(p: Profile): string {
  return p.hosts
    .filter((h) => h.alias)
    .map((h) => {
      let block = `# Profil: ${p.name}\nHost ${h.alias}\n    HostName ${h.hostname}\n`;
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
        };
        return result;
      }
      case "profile_defaults": {
        const id = slugify(String(args.name ?? ""));
        const result: ProfileDefaults = {
          id,
          key_path: `~/.ssh/${id}_ed25519`,
          folder: `~/dev/${id}/`,
          key_comment: `tristan@zentrale-${id}`,
          key_exists: state.keys.has(`~/.ssh/${id}_ed25519`),
          id_taken: state.profiles.some((p) => p.id === id),
        };
        return result;
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
        const h = p.hosts[0];
        if (!h) throw new MockError("no_host", `Profil „${p.id}“ hat keinen Host`);
        const ok = p.id !== "kunde-b";
        const result: ProbeResult = {
          success: ok,
          host: h.alias ?? h.hostname,
          account: ok && h.provider !== "generic" ? "@tristan" : null,
          message: ok
            ? h.provider === "generic"
              ? "Anmeldung erfolgreich"
              : "Verbunden als @tristan"
            : `${PROVIDER_NAME[h.provider]} kennt diesen Key nicht`,
          output: ok ? "Hi tristan! You've successfully authenticated." : "git@github.com: Permission denied (publickey).",
          command: `ssh -T -o PasswordAuthentication=no -o IdentitiesOnly=yes -o AddKeysToAgent=yes -i ${p.key.path} git@${h.hostname}`,
        };
        state.lastTests.set(p.id, { success: ok, account: result.account, message: result.message, at: Date.now() });
        return result;
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
