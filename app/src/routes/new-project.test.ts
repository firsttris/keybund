import { describe, expect, it } from "vitest";
import { emptyDraft, profileName, stepError, stepsFor, toHost, toProject, withDefaults, type Draft } from "./new-project";

const github: Draft = {
  ...emptyDraft(),
  name: "Kunde C",
  folder: "~/dev/kunde-c/",
  gitName: "Tristan",
  gitEmail: "t@kunde-c.de",
  comment: "t@zentrale-kunde-c",
  kind: "git",
};

const server: Draft = {
  ...emptyDraft(),
  kind: "server",
  hostname: "1.2.3.4",
  alias: "srv",
  user: "root",
  port: "2222",
  comment: "t@srv",
};

describe("stepsFor", () => {
  it("has a short flow for servers", () => {
    expect(stepsFor("server")).toEqual(["kind", "server", "key", "test"]);
    expect(stepsFor("git")).toEqual(["kind", "project", "account", "folder", "git", "key", "test"]);
    expect(stepsFor(null)).toEqual(stepsFor("git"));
  });
});

describe("toProject", () => {
  it("builds a GitHub project with a new key", () => {
    const p = toProject({ ...github, passphrase: "geheim" });
    expect(p.profile.id).toBe("kunde-c");
    expect(p.profile.key.path).toBe("~/.ssh/kunde-c_ed25519");
    expect(p.profile.hosts).toEqual([{ provider: "github", hostname: "github.com", alias: null, user: null, port: null }]);
    expect(p.profile.git).toEqual({ name: "Tristan", email: "t@kunde-c.de", sign: false });
    expect(p.key).toEqual({ type: "generate", comment: "t@zentrale-kunde-c", passphrase: "geheim" });
  });

  it("builds a server with an existing key and no git identity", () => {
    const p = toProject({ ...server, keyMode: "existing", existingPath: "~/.ssh/id_ed25519", gitEmail: "x@y.de" });
    expect(p.profile.key.path).toBe("~/.ssh/id_ed25519");
    expect(p.key).toEqual({ type: "existing", path: "~/.ssh/id_ed25519" });
    expect(p.profile.git).toBeNull();
    expect(p.profile.folders).toEqual([]);
    expect(p.profile.hosts[0]).toEqual({ provider: "generic", hostname: "1.2.3.4", alias: "srv", user: "root", port: 2222 });
    expect(p.make_default).toBe(false);
  });

  it("names a server after its alias or hostname", () => {
    expect(profileName(server)).toBe("srv");
    expect(profileName({ ...server, alias: "" })).toBe("1.2.3.4");
    expect(profileName({ ...server, name: "Prod" })).toBe("Prod");
    expect(toProject({ ...server, name: "Prod" }).profile.id).toBe("prod");
  });

  it("drops the user for git providers but keeps an alias", () => {
    const h = toHost({ ...github, alias: "x", user: "y" });
    expect(h.alias).toBe("x");
    expect(h.user).toBeNull();
  });

  it("passes an empty passphrase as null", () => {
    expect(toProject(github).key).toMatchObject({ passphrase: null });
  });
});

describe("stepError", () => {
  it("requires a kind", () => {
    expect(stepError("kind", { ...github, kind: null })).toMatch(/wofür/);
    expect(stepError("kind", github)).toBeNull();
  });

  it("validates the project step", () => {
    expect(stepError("project", { ...github, name: "" })).toMatch(/Projektnamen/);
    expect(stepError("project", { ...github, name: "!!!" })).toMatch(/mindestens/);
    expect(stepError("project", github, true)).toBe("Ein Profil „kunde-c“ gibt es schon.");
    expect(stepError("project", github)).toBeNull();
  });

  it("validates the git account", () => {
    expect(stepError("account", { ...github, hostname: " " })).toMatch(/Hostnamen/);
    expect(stepError("account", { ...github, port: "abc" })).toMatch(/Zahl/);
    expect(stepError("account", github)).toBeNull();
  });

  it("validates the server", () => {
    expect(stepError("server", { ...server, hostname: "" })).toMatch(/Hostname oder IP/);
    expect(stepError("server", { ...server, port: "22a" })).toMatch(/Zahl/);
    expect(stepError("server", { ...server, alias: "", hostname: "..." })).toMatch(/mindestens/);
    expect(stepError("server", server)).toBeNull();
  });

  it("requires a folder", () => {
    expect(stepError("folder", { ...github, folder: "" })).toMatch(/Projektordner/);
    expect(stepError("folder", github)).toBeNull();
  });

  it("validates the git identity", () => {
    expect(stepError("git", { ...github, gitName: "" })).toMatch(/Git-Namen/);
    expect(stepError("git", { ...github, gitEmail: "nope" })).toMatch(/E-Mail/);
    expect(stepError("git", github)).toBeNull();
  });

  it("validates the key step", () => {
    expect(stepError("key", { ...github, comment: "" })).toMatch(/Kommentar/);
    expect(stepError("key", { ...github, keyMode: "existing", existingPath: "" })).toMatch(/vorhandenen Key/);
    expect(stepError("key", github)).toBeNull();
    expect(stepError("test", github)).toBeNull();
  });

  it("lets a server reuse the key of a profile", () => {
    expect(stepError("key", { ...server, keyMode: "profile" })).toMatch(/Profil wählen/);
    expect(stepError("key", { ...server, keyMode: "profile", profileId: "privat" }, true)).toBeNull();
    expect(stepError("key", { ...server, comment: "c" }, true)).toMatch(/dessen Key verwenden/);
  });
});

describe("withDefaults", () => {
  const defaults = (id: string) => ({
    id,
    key_path: `~/.ssh/${id}_ed25519`,
    folder: `~/dev/${id}/`,
    key_comment: `t@host-${id}`,
    key_exists: false,
    id_taken: false,
  });

  it("derives folder and comment from the current name", () => {
    const d = withDefaults({ ...emptyDraft(), name: "Kunde C" }, defaults("kunde-c"));
    expect(d.folder).toBe("~/dev/kunde-c/");
    expect(d.comment).toBe("t@host-kunde-c");
  });

  it("follows a renamed project", () => {
    const renamed = { ...emptyDraft(), name: "Kunde D" };
    // Defaults still loading for the new name: the folder already follows,
    // the stale comment of the old name is not used.
    const d = withDefaults(renamed, defaults("kunde-c"));
    expect(d.folder).toBe("~/dev/kunde-d/");
    expect(d.comment).toBe("");
    expect(withDefaults(renamed, defaults("kunde-d")).comment).toBe("t@host-kunde-d");
  });

  it("keeps values the user edited", () => {
    const edited = { ...emptyDraft(), name: "Kunde D", folder: "~/work/x/", folderEdited: true, comment: "mine", commentEdited: true };
    expect(withDefaults(edited, defaults("kunde-d"))).toMatchObject({ folder: "~/work/x/", comment: "mine" });
    // An intentionally emptied folder stays empty; servers have none.
    expect(withDefaults({ ...edited, folder: "" }, defaults("kunde-d")).folder).toBe("");
    expect(withDefaults({ ...edited, kind: "server" }, defaults("kunde-d")).folder).toBe("");
  });

  it("is empty without a name", () => {
    expect(withDefaults(emptyDraft(), null)).toMatchObject({ folder: "", comment: "" });
  });
});
