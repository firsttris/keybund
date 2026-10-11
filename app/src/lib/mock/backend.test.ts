import { describe, expect, it } from "vitest";
import type { ImportDone, ImportProposal, NewProject, Plan, Profile, ProfilesView } from "../bindings";
import { createMockBackend, seedState } from "./backend";

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
    expect(view.profiles.map((p) => p.profile.id)).toEqual(["privat", "sparkasse", "kunde-b", "bundesagentur"]);
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

  it("pins provider host keys only on port 22", async () => {
    const b = backend();
    const p = b.state.profiles[0];
    const host = { alias: null, hostname: "github.com", user: null, port: 443, provider: "github" as const };
    await b.invoke("update_profile", { profile: { ...p, hosts: [host] } as Profile });
    const result = (await b.invoke("test_connection", { id: p.id, host: null })) as { success: boolean; message: string };
    expect(result.success).toBe(false);
    expect(result.message).toContain("nicht bestätigt");
  });

  it("refuses to restore a file that changed since the preview", async () => {
    const b = backend();
    b.state.applied.set("~/.config/keybund/git/sparkasse.gitconfig", "kaputt\n");
    const plan = (await b.invoke("plan_changes")) as Plan;
    expect(plan.changes).toHaveLength(1);
    await expect(b.invoke("apply_changes", { expectedDiffs: [] })).rejects.toMatchObject({ kind: "stale_plan" });
    await b.invoke("apply_changes", { expectedDiffs: plan.changes.map((c) => c.diff) });
    expect(((await b.invoke("plan_changes")) as Plan).changes).toEqual([]);
  });

  it("archives and removes the configs", async () => {
    const b = backend();
    const res = await b.invoke("archive_profile", { id: "kunde-b" });
    expect(res).toMatchObject({ key_path: "~/.ssh/archive/kunde-b_ed25519", remove_key_urls: ["https://github.com/settings/keys"] });
    expect(b.state.applied.has("~/.config/keybund/git/kunde-b.gitconfig")).toBe(false);
    expect(((await b.invoke("plan_changes")) as Plan).changes).toEqual([]);
    await expect(b.invoke("archive_profile", { id: "kunde-b" })).rejects.toMatchObject({ kind: "archived" });
  });

  it("resolves folders and hosts", async () => {
    const b = backend();
    expect(await b.invoke("which", { query: "~/dev/sparkasse/repo" })).toMatchObject({ profile_id: "sparkasse", mode: "folder" });
    expect(await b.invoke("which", { query: "~/elsewhere" })).toMatchObject({ profile_id: "privat", via: "Standard-Key" });
    expect(await b.invoke("which", { query: "meinserver" })).toMatchObject({ profile_id: "privat", hostname: "1.2.3.4" });
  });

  it("previews clones", async () => {
    const b = backend();
    expect(await b.invoke("clone_preview", { url: "git@gitlab.kunde.de:team/app.git" })).toMatchObject({
      matched: true,
      candidates: [{ profile_id: "sparkasse", target: "~/dev/sparkasse/app" }],
    });
    expect(await b.invoke("clone_preview", { url: "https://github.com/a/b" })).toMatchObject({ matched: true, repo: { name: "b" } });
    await expect(b.invoke("clone_preview", { url: "kaputt" })).rejects.toMatchObject({ kind: "invalid_url" });
  });

  it("proposes, imports and cleans up ~/.ssh/config like the core", async () => {
    const b = backend();
    const proposals = (await b.invoke("import_proposals")) as ImportProposal[];
    expect(proposals.map((p) => p.profile.id)).toEqual(["pflegeabc", "s-communication"]);
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
    expect(done.ids).toEqual(["pflegeabc"]);
    expect(done.report.backups).toEqual(["~/.ssh/config.keybund-backup"]);
    expect(b.state.sshBlocks.map((x) => x.pattern)).toEqual(["gitlab.sfp.berlin"]);
  });

  it("makes a plain git host the default when there is none", async () => {
    const b = backend();
    b.state.defaultProfile = null;
    b.state.sshBlocks = [{ pattern: "github.com", hostname: "github.com", identity: "~/.ssh/id_ed25519", comment: "Privat" }];
    const [p] = (await b.invoke("import_proposals")) as ImportProposal[];
    expect(p.make_default).toBe(true);
    expect(p.profile.hosts[0].alias).toBeNull();
  });

  it("rejects unknown commands", async () => {
    await expect(backend().invoke("nope")).rejects.toMatchObject({ kind: "other" });
  });
});
