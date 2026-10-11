import { describe, expect, it, vi } from "vitest";
import type { ImportDone, ImportProposal, NewProject, Plan, Profile, ProfilesView } from "../bindings";
import { createMockBackend, normalizeFolder, seedState } from "./backend";

function backend() {
  const state = seedState();
  state.latency = 0;
  return createMockBackend(state);
}

const project = (id: string, folder = `~/dev/${id}`): NewProject => ({
  profile: {
    id,
    name: id,
    color: null,
    key: { path: `~/.ssh/${id}_ed25519`, comment: null },
    git: { name: "T", email: `t@${id}.de`, sign: false },
    folders: [folder],
    archived: false,
    hosts: [{ alias: null, hostname: "github.com", user: null, port: null, provider: "github" }],
  },
  key: { type: "generate", comment: id, passphrase: null },
  make_default: false,
});

// The mock must honor the same contract as the Rust commands, otherwise
// the E2E tests would test a fantasy backend.
describe("mock backend contract", () => {
  it("starts in sync", async () => {
    const b = backend();
    expect(((await b.invoke("plan_changes")) as Plan).changes).toEqual([]);
    const view = (await b.invoke("load_profiles")) as ProfilesView;
    expect(view.pending_changes).toBe(0);
    expect(view.profiles.map((p) => p.profile.id)).toEqual(["privat", "acme", "kunde-b", "altprojekt"]);
  });

  it("previews without writing, creates with writing", async () => {
    const b = backend();
    const plan = (await b.invoke("preview_new_project", { project: project("c") })) as Plan;
    expect(plan.changes.map((c) => c.path)).toContain("~/.config/keybund/git/c.gitconfig");
    expect(b.state.profiles).toHaveLength(4);
    await b.invoke("create_project", { project: project("c"), openProviderPage: true });
    expect(b.state.profiles).toHaveLength(5);
    expect(b.state.openedUrls).toEqual(["https://github.com/settings/ssh/new"]);
    expect(((await b.invoke("plan_changes")) as Plan).changes).toEqual([]);
    await expect(b.invoke("create_project", { project: project("c") })).rejects.toMatchObject({ kind: "duplicate_id" });
  });

  it("rejects nested folders", async () => {
    const b = backend();
    await expect(b.invoke("preview_new_project", { project: project("x", "~/dev/privat/x") })).rejects.toMatchObject({
      kind: "nested_folders",
    });
  });

  it("writes changes right away", async () => {
    const b = backend();
    const p = b.state.profiles[1];
    await b.invoke("update_profile", { profile: { ...p, git: { ...p.git!, sign: true } } as Profile });
    await b.invoke("set_default_profile", { id: null });
    expect(((await b.invoke("plan_changes")) as Plan).changes).toEqual([]);
    expect(b.state.applied.get(`~/.config/keybund/git/${p.id}.gitconfig`)).toContain("gpgsign = true");
  });

  it("validates the default key and trims values like the Rust side", async () => {
    const b = backend();
    await expect(b.invoke("set_default_profile", { id: "gibtsnicht" })).rejects.toMatchObject({
      kind: "default_profile_missing",
    });
    const p = b.state.profiles[0];
    const host = { alias: "  ", hostname: " example.org ", user: " git ", port: null, provider: "generic" as const };
    await b.invoke("update_profile", { profile: { ...p, name: " Privat ", hosts: [host] } as Profile });
    expect(b.state.profiles[0].name).toBe("Privat");
    expect(b.state.profiles[0].hosts[0]).toMatchObject({ alias: null, hostname: "example.org", user: "git" });
  });

  it("remembers the last test per host, the key's summary prefers a failure", async () => {
    const b = backend();
    b.state.unreachable.push("1.2.3.4");
    await b.invoke("test_connection", { id: "privat", host: "meinserver" });
    await b.invoke("test_connection", { id: "privat", host: "github.com" });
    const view = ((await b.invoke("load_profiles")) as ProfilesView).profiles[0];
    expect(view.host_tests.map((t) => [t.host, t.success])).toEqual([
      ["github.com", true],
      ["meinserver", false],
    ]);
    expect(view.last_test).toMatchObject({ host: "meinserver", success: false });
  });

  it("pins provider host keys only on port 22", async () => {
    const b = backend();
    const p = b.state.profiles[0];
    const host = { alias: null, hostname: "github.com", user: null, port: 443, provider: "github" as const };
    await b.invoke("update_profile", { profile: { ...p, hosts: [host] } as Profile });
    const result = (await b.invoke("test_connection", { id: p.id, host: null })) as {
      success: boolean;
      message: unknown;
    };
    expect(result.success).toBe(false);
    expect(result.message).toMatchObject({ key: "probe_hostKeyUnconfirmed" });
  });

  it("refuses to restore a file that changed since the preview", async () => {
    const b = backend();
    b.state.applied.set("~/.config/keybund/git/acme.gitconfig", "kaputt\n");
    const plan = (await b.invoke("plan_changes")) as Plan;
    expect(plan.changes).toHaveLength(1);
    await expect(b.invoke("apply_changes", { expectedDiffs: [] })).rejects.toMatchObject({ kind: "stale_plan" });
    await b.invoke("apply_changes", { expectedDiffs: plan.changes.map((c) => c.diff) });
    expect(((await b.invoke("plan_changes")) as Plan).changes).toEqual([]);
  });

  it("archives and removes the configs", async () => {
    const b = backend();
    const res = await b.invoke("archive_profile", { id: "kunde-b" });
    expect(res).toMatchObject({
      key_path: "~/.ssh/archive/kunde-b_ed25519",
      remove_key_urls: ["https://github.com/settings/keys"],
    });
    expect(b.state.applied.has("~/.config/keybund/git/kunde-b.gitconfig")).toBe(false);
    expect(((await b.invoke("plan_changes")) as Plan).changes).toEqual([]);
    await expect(b.invoke("archive_profile", { id: "kunde-b" })).rejects.toMatchObject({ kind: "archived" });
  });

  it("resolves folders and hosts", async () => {
    const b = backend();
    expect(await b.invoke("which", { query: "~/dev/acme/repo" })).toMatchObject({ profile_id: "acme", mode: "folder" });
    expect(await b.invoke("which", { query: "~/elsewhere" })).toMatchObject({
      profile_id: "privat",
      via: { key: "which_viaDefault" },
    });
    expect(await b.invoke("which", { query: "meinserver" })).toMatchObject({
      profile_id: "privat",
      hostname: "1.2.3.4",
    });
  });

  it("previews clones", async () => {
    const b = backend();
    expect(await b.invoke("clone_preview", { url: "git@gitlab.kunde.de:team/app.git" })).toMatchObject({
      matched: true,
      candidates: [{ profile_id: "acme", target: "~/dev/acme/app" }],
    });
    expect(await b.invoke("clone_preview", { url: "https://github.com/a/b" })).toMatchObject({
      matched: true,
      repo: { name: "b" },
    });
    await expect(b.invoke("clone_preview", { url: "kaputt" })).rejects.toMatchObject({ kind: "invalid_url" });
  });

  it("proposes, imports and cleans up ~/.ssh/config like the core", async () => {
    const b = backend();
    const proposals = (await b.invoke("import_proposals")) as ImportProposal[];
    expect(proposals.map((p) => p.profile.id)).toEqual(["nebenjob", "nordlicht-media"]);
    expect(proposals.every((p) => !p.make_default)).toBe(true);
    expect(((await b.invoke("load_profiles")) as ProfilesView).import_available).toBe(2);
    expect(((await b.invoke("cleanup_preview")) as Plan).changes).toEqual([]);

    await b.invoke("import_profiles", { chosen: proposals, removeOld: false });
    const after = (await b.invoke("load_profiles")) as ProfilesView;
    expect([after.import_available, after.pending_changes]).toEqual([0, 0]);
    const cleanup = (await b.invoke("cleanup_preview")) as Plan;
    expect(cleanup.changes[0]).toMatchObject({ path: "~/.ssh/config", user_file: true });
    await expect(b.invoke("cleanup_apply", { expectedDiffs: [] })).rejects.toMatchObject({ kind: "stale_plan" });
    await b.invoke("cleanup_apply", { expectedDiffs: cleanup.changes.map((c) => c.diff) });
    expect(b.state.sshBlocks).toEqual([]);
  });

  it("imports and removes the old entries in one step", async () => {
    const b = backend();
    const proposals = (await b.invoke("import_proposals")) as ImportProposal[];
    const done = (await b.invoke("import_profiles", { chosen: proposals.slice(0, 1), removeOld: true })) as ImportDone;
    expect(done.ids).toEqual(["nebenjob"]);
    expect(done.report.backups).toEqual(["~/.ssh/config.keybund-backup"]);
    expect(b.state.sshBlocks.map((x) => x.pattern)).toEqual(["gitlab.nordlicht.de"]);
  });

  it("makes a plain git host the default when there is none", async () => {
    const b = backend();
    b.state.defaultProfile = null;
    b.state.sshBlocks = [
      { pattern: "github.com", hostname: "github.com", identity: "~/.ssh/id_ed25519", comment: "Privat" },
    ];
    const [p] = (await b.invoke("import_proposals")) as ImportProposal[];
    expect(p.make_default).toBe(true);
    expect(p.profile.hosts[0].alias).toBeNull();
  });

  it("checks ids, names and values like store::validate", async () => {
    const b = backend();
    const p = b.state.profiles[0];
    const update = (patch: Partial<Profile>) => b.invoke("update_profile", { profile: { ...p, ...patch } as Profile });
    const withHost = (host: Partial<Profile["hosts"][number]>) =>
      update({
        hosts: [{ alias: null, hostname: "example.org", user: null, port: null, provider: "generic", ...host }],
      });
    await expect(b.invoke("create_project", { project: project("Kunde C") })).rejects.toMatchObject({
      kind: "invalid_id",
      message: { key: "error_invalidId", args: { id: "Kunde C" } },
    });
    await expect(update({ name: "  " })).rejects.toMatchObject({ kind: "missing_name" });
    await expect(update({ name: "Pri\nvat" })).rejects.toMatchObject({
      kind: "invalid_value",
      message: { key: "error_invalidValue", args: { value: "Pri\nvat", field: { key: "field_name", args: {} } } },
    });
    for (const path of ['~/.ssh/a"b', "~/.ssh/%d/key", `~/.ssh/\${HOME}/key`, "~/.ssh/a\\b"]) {
      await expect(update({ key: { path, comment: null } })).rejects.toMatchObject({
        kind: "invalid_value",
        message: { args: { field: { key: "field_key" } } },
      });
    }
    await expect(update({ git: { name: "T", email: "a b@x.de", sign: true } })).rejects.toMatchObject({
      kind: "invalid_value",
      message: { args: { field: { key: "field_gitEmail" } } },
    });
    for (const hostname of ["my server", "-x", "a*b", "a?b", "a,b", "a#b", "a'b", "a[b]"]) {
      await expect(withHost({ hostname })).rejects.toMatchObject({
        kind: "invalid_value",
        message: { args: { value: hostname, field: { key: "field_hostname" } } },
      });
    }
    await expect(withHost({ alias: "-nas" })).rejects.toMatchObject({
      message: { args: { field: { key: "field_alias" } } },
    });
    await expect(withHost({ user: "root admin" })).rejects.toMatchObject({
      message: { args: { field: { key: "field_user" } } },
    });
    for (const folder of ["dev/x", "~/dev/*", "~/dev/a?b", "~/dev/[a]", "~/dev\\x"]) {
      await expect(update({ folders: [folder] })).rejects.toMatchObject({
        kind: "invalid_path",
        message: { key: "error_invalidPath" },
      });
    }
    // Nothing of that was saved.
    expect(b.state.profiles[0]).toEqual(p);
    // A saved folder with `.` or `..` (git compares real paths) fails every later check.
    b.state.profiles[2].folders = ["~/dev/../x/"];
    await expect(b.invoke("preview_add_host", { id: "kunde-b", host: p.hosts[0] })).rejects.toMatchObject({
      kind: "invalid_path",
      message: { args: { path: "~/dev/../x/" } },
    });
  });

  it("rejects the same host twice in one key and aliases that differ only in case", async () => {
    const b = backend();
    const github = { alias: null, hostname: "github.com", user: null, port: null, provider: "github" as const };
    await expect(
      b.invoke("add_host", { id: "privat", host: { ...github, hostname: "GitHub.com" } }),
    ).rejects.toMatchObject({
      kind: "duplicate_host",
      message: { key: "error_duplicateHost", args: { hostname: "github.com" } },
    });
    // Another alias on the same host is a different entry.
    await b.invoke("add_host", { id: "privat", host: { ...github, alias: "gh-work" } });
    await expect(
      b.invoke("preview_add_host", { id: "privat", host: { ...github, alias: "GH-Work" } }),
    ).rejects.toMatchObject({ kind: "duplicate_host" });
    await expect(
      b.invoke("add_host", { id: "kunde-b", host: { ...github, provider: "generic", hostname: "MeinServer" } }),
    ).rejects.toMatchObject({ kind: "duplicate_alias", message: { args: { host: "meinserver" } } });
  });

  it("stores host names in lower case and trims them", async () => {
    const b = backend();
    const host = { alias: " NAS ", hostname: "Nas.Local", user: " Root ", port: null, provider: "generic" as const };
    await b.invoke("add_host", { id: "kunde-b", host });
    expect(b.state.profiles[2].hosts[1]).toMatchObject({ alias: "nas", hostname: "nas.local", user: "Root" });
    const p = b.state.profiles[0];
    await b.invoke("update_profile", {
      profile: { ...p, hosts: [{ ...host, alias: "Srv2", hostname: "Nas2.Local" }] } as Profile,
    });
    expect(b.state.profiles[0].hosts[0]).toMatchObject({ alias: "srv2", hostname: "nas2.local" });
    const c = project("c");
    c.profile.hosts = [{ ...host, alias: "C-Host", hostname: "Nas3.Local" }];
    await b.invoke("create_project", { project: c });
    expect(b.state.profiles[4].hosts[0]).toMatchObject({ alias: "c-host", hostname: "nas3.local" });
  });

  it("normalizes folders like store::normalize_folder", async () => {
    const b = backend();
    expect(await b.invoke("normalize_folder", { folder: " ~/dev//x/./y/../z " })).toBe("~/dev/x/z/");
    expect(await b.invoke("normalize_folder", { folder: "/home/tristan/dev/x" })).toBe("~/dev/x/");
    expect(await b.invoke("normalize_folder", { folder: "/srv/x/" })).toBe("/srv/x/");
    expect(await b.invoke("normalize_folder", { folder: "~" })).toBe("~/");
    expect(normalizeFolder("/home/tristan")).toBe("~/");
    const p = b.state.profiles[2];
    await b.invoke("update_profile", { profile: { ...p, folders: ["/home/tristan/dev/kunde-b//"] } as Profile });
    expect(b.state.profiles[2].folders).toEqual(["~/dev/kunde-b/"]);
    // Nested is decided on the real path.
    await expect(
      b.invoke("update_profile", { profile: { ...p, folders: ["/home/tristan/dev/privat/sub"] } as Profile }),
    ).rejects.toMatchObject({ kind: "nested_folders" });
  });

  it("writes the settings folders relative to ~ without a trailing slash", async () => {
    const b = backend();
    const settings = { ...b.state.settings, projects_dir: "/home/tristan/work/", keys_dir: " ~/.ssh/keys// " };
    expect(await b.invoke("update_settings", { settings })).toMatchObject({
      projects_dir: "~/work",
      keys_dir: "~/.ssh/keys",
    });
    expect(await b.invoke("update_settings", { settings: { ...settings, projects_dir: "/" } })).toMatchObject({
      projects_dir: "/",
    });
    await expect(b.invoke("update_settings", { settings: { ...settings, keys_dir: "keys" } })).rejects.toMatchObject({
      kind: "invalid_path",
      message: { args: { path: "keys" } },
    });
  });

  it("takes its time for writes when asked", async () => {
    const b = backend();
    b.state.latency = 100;
    vi.useFakeTimers();
    try {
      const p = b.state.profiles[0];
      let done = 0;
      void b.invoke("update_profile", { profile: { ...p, name: "Neu" } as Profile }).then(() => done++);
      void b
        .invoke("add_host", {
          id: "kunde-b",
          host: { alias: null, hostname: "10.0.0.1", user: null, port: null, provider: "generic" },
        })
        .then(() => done++);
      void b.invoke("host_key_check", { id: "privat", host: null }).then(() => done++);
      await vi.advanceTimersByTimeAsync(50);
      expect(done).toBe(0);
      await vi.advanceTimersByTimeAsync(60);
      expect(done).toBe(3);
    } finally {
      vi.useRealTimers();
    }
  });

  it("previews a new key as the default only on request", async () => {
    const b = backend();
    b.state.defaultProfile = null;
    const index = (plan: Plan) => plan.changes.find((c) => c.path.endsWith("index.gitconfig"))?.diff ?? "";
    const plain = (await b.invoke("preview_new_project", { project: project("c") })) as Plan;
    expect(index(plain)).not.toContain("+# Default key: c");
    const asDefault = (await b.invoke("preview_new_project", {
      project: { ...project("c"), make_default: true },
    })) as Plan;
    expect(index(asDefault)).toContain("+# Default key: c");
    // The existing key's path is written relative to ~.
    const existing: NewProject = { ...project("d"), key: { type: "existing", path: "/home/tristan/.ssh/id_ed25519" } };
    await b.invoke("create_project", { project: existing });
    expect(b.state.profiles[4].key.path).toBe("~/.ssh/id_ed25519");
    expect(b.state.defaultProfile).toBeNull();
  });

  it("refuses to archive over an archived key and forgets the last test", async () => {
    const b = backend();
    b.state.keys.set("~/.ssh/archive/acme_ed25519", { ageDays: 1, comment: "alt" });
    await expect(b.invoke("archive_profile", { id: "acme" })).rejects.toMatchObject({
      kind: "key_exists",
      message: { args: { path: "~/.ssh/archive/acme_ed25519" } },
    });
    expect(b.state.profiles[1].archived).toBe(false);
    b.state.keys.delete("~/.ssh/archive/acme_ed25519");
    expect(b.state.lastTests.has("acme/gitlab.kunde.de")).toBe(true);
    await b.invoke("archive_profile", { id: "acme" });
    expect(b.state.lastTests.has("acme/gitlab.kunde.de")).toBe(false);
    expect(((await b.invoke("load_profiles")) as ProfilesView).profiles[1].last_test).toBeNull();
  });

  it("leaves a key file in place that another key entry still uses", async () => {
    const b = backend();
    b.state.profiles[2].key.path = "~/.ssh/privat_ed25519";
    expect(await b.invoke("archive_profile", { id: "kunde-b" })).toMatchObject({ key_path: "~/.ssh/privat_ed25519" });
    expect(b.state.keys.has("~/.ssh/privat_ed25519")).toBe(true);
    expect(b.state.keys.has("~/.ssh/archive/privat_ed25519")).toBe(false);
    expect(b.state.profiles[2]).toMatchObject({ archived: true, key: { path: "~/.ssh/privat_ed25519" } });
  });

  it("keeps profiles.toml when writing the configs fails", async () => {
    const b = backend();
    b.state.applyError = "Read-only file system";
    const p = b.state.profiles[1];
    await expect(
      b.invoke("update_profile", { profile: { ...p, git: { ...p.git!, sign: true } } as Profile }),
    ).rejects.toMatchObject({ kind: "io", message: { key: "error_io", args: { detail: "Read-only file system" } } });
    expect(b.state.profiles[1].git!.sign).toBe(true);
    expect(((await b.invoke("load_profiles")) as ProfilesView).pending_changes).toBe(1);
    await expect(b.invoke("set_default_profile", { id: null })).rejects.toMatchObject({ kind: "io" });
    b.state.applyError = null;
    await b.invoke("set_default_profile", { id: null });
    expect(((await b.invoke("load_profiles")) as ProfilesView).pending_changes).toBe(0);
  });

  it("fails every command while profiles.toml cannot be parsed", async () => {
    const b = backend();
    b.state.configError = "Zeile 3";
    await expect(b.invoke("load_profiles")).rejects.toMatchObject({
      kind: "config_parse",
      message: { key: "error_configParse", args: { detail: "Zeile 3" } },
    });
    await expect(b.invoke("settings")).rejects.toMatchObject({ kind: "config_parse" });
    expect(await b.invoke("plugin:app|version")).toBe("0.1.0");
  });

  it("lets the host key check fail on request", async () => {
    const b = backend();
    for (const [kind, key] of [
      ["tool_missing", "error_toolMissing"],
      ["io", "error_io"],
      ["network", "error_network"],
      ["no_host", "error_other"],
    ]) {
      b.state.hostKeyError = kind;
      await expect(b.invoke("host_key_check", { id: "privat", host: "meinserver" })).rejects.toMatchObject({
        kind,
        message: { key },
      });
    }
    b.state.hostKeyError = null;
    expect(await b.invoke("host_key_check", { id: "privat", host: "meinserver" })).toEqual({ state: "known" });
  });

  it("rejects unknown commands", async () => {
    await expect(backend().invoke("nope")).rejects.toMatchObject({ kind: "other" });
  });
});
