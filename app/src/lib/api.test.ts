import { mockIPC } from "@tauri-apps/api/mocks";
import { describe, expect, it } from "vitest";
import { api, ApiError, errorMessage } from "./api";

describe("api", () => {
  it("returns data of successful commands", async () => {
    mockIPC((cmd) => (cmd === "which" ? { query: "x" } : null));
    await expect(api.which("x")).resolves.toEqual({ query: "x" });
  });

  it("throws ApiError with the backend's kind", async () => {
    mockIPC(() => {
      throw { kind: "invalid_url", message: { key: "error_invalidUrl", args: { url: "x" } } };
    });
    const error = await api.clonePreview("x").catch((e) => e);
    expect(error).toBeInstanceOf(ApiError);
    expect(error.kind).toBe("invalid_url");
    expect(errorMessage(error)).toBe("Ungültige Repo-URL: x");
  });

  it("passes plain return values through", async () => {
    mockIPC((cmd, args) => (cmd === "normalize_folder" ? `${(args as { folder: string }).folder}/` : null));
    await expect(api.normalizeFolder("~/dev/x")).resolves.toBe("~/dev/x/");
  });

  it("formats unknown errors", () => {
    expect(errorMessage("text")).toBe("text");
    expect(errorMessage(42)).toBe("Unbekannter Fehler");
  });
});
