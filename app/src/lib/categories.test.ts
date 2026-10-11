import { describe, expect, it } from "vitest";
import type { ProfileView } from "./bindings";
import { activeKeys, usageParts } from "./categories";

const view = (patch: Partial<ProfileView["profile"]>, isDefault = false): ProfileView =>
  ({
    profile: {
      id: "a",
      name: "A",
      color: null,
      key: { path: "~/.ssh/a", comment: null },
      git: null,
      folders: [],
      archived: false,
      hosts: [],
      ...patch,
    },
    is_default: isDefault,
  }) as ProfileView;

describe("usageParts", () => {
  it("says in short words what a key is used for", () => {
    expect(usageParts(view({}))).toEqual([]);
    const gh = { provider: "github" as const, hostname: "github.com", alias: null, user: null, port: null };
    const srv = { provider: "generic" as const, hostname: "1.2.3.4", alias: "nas", user: null, port: null };
    expect(usageParts(view({ folders: ["~/dev/a/"], hosts: [gh, gh, srv] }, true))).toEqual([
      "Standard",
      "~/dev/a/",
      "GitHub",
      "nas",
    ]);
    expect(
      usageParts(view({ folders: ["~/a/", "~/b/"], hosts: [srv, { ...srv, alias: null, hostname: "5.6.7.8" }] })),
    ).toEqual(["2 Ordner", "2 Server"]);
  });

  it("puts the standard key first and leaves archived keys out", () => {
    const keys = activeKeys([view({ id: "x" }), view({ id: "old", archived: true }), view({ id: "std" }, true)]);
    expect(keys.map((k) => k.profile.id)).toEqual(["std", "x"]);
  });
});
