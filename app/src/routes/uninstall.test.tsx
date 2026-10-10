import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "@/test/render";

describe("uninstall", () => {
  it("is reachable from the settings", async () => {
    const { user, router } = await renderApp("/settings");
    await user.click(await screen.findByRole("link", { name: "Entfernen …" }));
    expect(router.state.location.pathname).toBe("/uninstall");
  });

  it("previews, asks and removes; profiles stay", async () => {
    const { user, backend } = await renderApp("/uninstall");
    expect(await screen.findByText("-Include ~/.ssh/config.d/keybund.conf")).toBeInTheDocument();
    expect(screen.getAllByText("· gelöscht").length).toBeGreaterThan(1);
    await user.click(screen.getByRole("button", { name: "Keybund entfernen" }));
    const dialog = await screen.findByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: "Entfernen" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Keybund kennt deine Keys noch");
    expect(backend.state.installed).toBe(false);
    expect(backend.state.profiles.length).toBeGreaterThan(0);
  });

  it("can purge the profiles too", async () => {
    const { user, backend } = await renderApp("/uninstall");
    await user.click(await screen.findByRole("checkbox", { name: /Auch Keybunds Liste der Keys löschen/ }));
    expect(await screen.findByText("~/.config/keybund/profiles.toml")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /Originale aus dem Backup/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Keybund entfernen" }));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Entfernen" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Auch die Liste deiner Keys ist gelöscht");
    expect(backend.state.profiles).toEqual([]);
  });
});
