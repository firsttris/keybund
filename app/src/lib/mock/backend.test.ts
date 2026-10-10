import { describe, expect, it } from "vitest";
import type { NewProject, Plan, Profile, ProfilesView } from "../bindings";
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

  it("refuses to apply a plan that changed since the preview", async () => {
    const b = backend();
    const p = b.state.profiles[1];
    await b.invoke("update_profile", { profile: { ...p, git: { ...p.git!, sign: true } } as Profile });
    const plan = (await b.invoke("plan_changes")) as Plan;
    expect(plan.changes).toHaveLength(1);
    await expect(b.invoke("apply_changes", { expectedDiffs: [] })).rejects.toMatchObject({ kind: "stale_plan" });
    await b.invoke("apply_changes", { expectedDiffs: plan.changes.map((c) => c.diff) });
    expect(((await b.invoke("plan_changes")) as Plan).changes).toEqual([]);
  });

  it("archives and plans the deletion", async () => {
    const b = backend();
    const res = await b.invoke("archive_profile", { id: "kunde-b" });
    expect(res).toMatchObject({ key_path: "~/.ssh/archive/kunde-b_ed25519", remove_key_urls: ["https://github.com/settings/keys"] });
    const plan = (await b.invoke("plan_changes")) as Plan;
    expect(plan.changes.find((c) => c.path.endsWith("kunde-b.gitconfig"))?.kind).toBe("delete");
    await expect(b.invoke("archive_profile", { id: "kunde-b" })).rejects.toMatchObject({ kind: "archived" });
  });

  it("resolves folders and hosts", async () => {
    const b = backend();
    expect(await b.invoke("which", { query: "~/dev/sparkasse/repo" })).toMatchObject({ profile_id: "sparkasse", mode: "folder" });
    expect(await b.invoke("which", { query: "~/elsewhere" })).toMatchObject({ profile_id: "privat", via: "Default-Profil" });
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

  it("rejects unknown commands", async () => {
    await expect(backend().invoke("nope")).rejects.toMatchObject({ kind: "other" });
  });
});
