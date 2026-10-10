import { describe, expect, it } from "vitest";
import { emptyDraft, stepError, toProject, type Draft } from "./new-project";

const github: Draft = {
  ...emptyDraft(),
  name: "Kunde C",
  folder: "~/dev/kunde-c/",
  gitName: "Tristan",
  gitEmail: "t@kunde-c.de",
  comment: "t@zentrale-kunde-c",
};

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
    const p = toProject({
      ...emptyDraft("~/.ssh/id_ed25519"),
      name: "Server",
      provider: "generic",
      hostname: "1.2.3.4",
      alias: "srv",
      user: "root",
      port: "2222",
    });
    expect(p.profile.key.path).toBe("~/.ssh/id_ed25519");
    expect(p.key).toEqual({ type: "existing", path: "~/.ssh/id_ed25519" });
    expect(p.profile.git).toBeNull();
    expect(p.profile.folders).toEqual([]);
    expect(p.profile.hosts[0]).toEqual({ provider: "generic", hostname: "1.2.3.4", alias: "srv", user: "root", port: 2222 });
  });

  it("drops alias and user for git providers", () => {
    const p = toProject({ ...github, alias: "x", user: "y" });
    expect(p.profile.hosts[0].alias).toBeNull();
    expect(p.profile.hosts[0].user).toBeNull();
  });

  it("passes an empty passphrase as null", () => {
    expect(toProject(github).key).toMatchObject({ passphrase: null });
  });
});

describe("stepError", () => {
  it("validates the project step", () => {
    expect(stepError(0, { ...github, name: "" })).toMatch(/Projektnamen/);
    expect(stepError(0, { ...github, name: "!!!" })).toMatch(/mindestens/);
    expect(stepError(0, github, true)).toBe("Ein Profil „kunde-c“ gibt es schon.");
    expect(stepError(0, github)).toBeNull();
  });

  it("validates hosts", () => {
    expect(stepError(1, { ...github, hostname: " " })).toMatch(/Hostnamen/);
    expect(stepError(1, { ...github, provider: "generic", hostname: "1.2.3.4" })).toMatch(/Alias/);
    expect(stepError(1, { ...github, port: "abc" })).toMatch(/Zahl/);
    expect(stepError(1, github)).toBeNull();
  });

  it("requires a folder only for git hosts", () => {
    expect(stepError(2, { ...github, folder: "" })).toMatch(/Projektordner/);
    expect(stepError(2, { ...github, folder: "", provider: "generic" })).toBeNull();
  });

  it("validates the git identity, optional for servers", () => {
    expect(stepError(3, { ...github, gitName: "" })).toMatch(/Git-Namen/);
    expect(stepError(3, { ...github, gitEmail: "nope" })).toMatch(/E-Mail/);
    expect(stepError(3, { ...github, provider: "generic", gitName: "", gitEmail: "" })).toBeNull();
    expect(stepError(3, github)).toBeNull();
  });

  it("validates the key step", () => {
    expect(stepError(4, { ...github, comment: "" })).toMatch(/Kommentar/);
    expect(stepError(4, { ...github, keyMode: "existing", existingPath: "" })).toMatch(/vorhandenen Key/);
    expect(stepError(4, github)).toBeNull();
    expect(stepError(5, github)).toBeNull();
  });
});
