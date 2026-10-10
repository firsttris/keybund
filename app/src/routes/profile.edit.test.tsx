import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "@/test/render";

describe("profile editing", () => {
  it("adds a server host", async () => {
    const { user, backend } = await renderApp("/profile/kunde-b");
    await user.click(await screen.findByRole("button", { name: "+ Host" }));
    const form = screen.getByRole("form", { name: "Host hinzufügen" });
    await user.selectOptions(within(form).getByLabelText("Provider"), "generic");
    await user.type(within(form).getByLabelText("Hostname"), "10.0.0.7");
    await user.type(within(form).getByLabelText("Alias"), "srv");
    await user.type(within(form).getByLabelText("Benutzer"), "root");
    await user.type(within(form).getByLabelText("Port"), "2200");
    await user.click(within(form).getByRole("button", { name: "Host speichern" }));
    expect(await screen.findByText("srv")).toBeInTheDocument();
    expect(backend.state.profiles.find((p) => p.id === "kunde-b")!.hosts[1]).toEqual({
      provider: "generic",
      hostname: "10.0.0.7",
      alias: "srv",
      user: "root",
      port: 2200,
    });
    await user.click(screen.getByRole("button", { name: "Host srv entfernen" }));
    expect(await screen.findByText("Configs aktuell")).toBeInTheDocument();
  });

  it("cancels the host form", async () => {
    const { user } = await renderApp("/profile/kunde-b");
    await user.click(await screen.findByRole("button", { name: "+ Host" }));
    await user.click(screen.getByRole("button", { name: "Abbrechen" }));
    expect(screen.queryByRole("form", { name: "Host hinzufügen" })).not.toBeInTheDocument();
  });

  it("adds and removes folders", async () => {
    const { user } = await renderApp("/profile/kunde-b");
    await user.click(await screen.findByRole("button", { name: "+ Ordner" }));
    await user.type(screen.getByLabelText("Neuer Ordner"), "~/dev/privat/sub");
    await user.click(screen.getByRole("button", { name: "Speichern" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Projektordner überschneiden sich");
    await user.clear(screen.getByLabelText("Neuer Ordner"));
    await user.type(screen.getByLabelText("Neuer Ordner"), "~/work/b");
    await user.click(screen.getByRole("button", { name: "Speichern" }));
    expect(await screen.findByText("~/work/b/")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Ordner ~/work/b/ entfernen" }));
    expect(screen.queryByText("~/work/b/")).not.toBeInTheDocument();
  });

  it("saves the git identity", async () => {
    const { user, backend } = await renderApp("/profile/kunde-b");
    const name = await screen.findByLabelText("Name");
    await user.clear(name);
    await user.type(name, "T. T.");
    await user.click(screen.getByRole("button", { name: "Identität speichern" }));
    expect(await screen.findByText("1 Datei ist nicht aktuell")).toBeInTheDocument();
    expect(backend.state.profiles.find((p) => p.id === "kunde-b")!.git!.name).toBe("T. T.");
  });

  it("sets the default and archives", async () => {
    const { user, backend, router } = await renderApp("/profile/kunde-b");
    await user.click(await screen.findByRole("button", { name: "Als Default setzen" }));
    expect(await screen.findByText("Default")).toBeInTheDocument();
    expect(backend.state.defaultProfile).toBe("kunde-b");

    await user.click(screen.getByRole("button", { name: "Archivieren" }));
    const dialog = await screen.findByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: "Archivieren" }));
    expect(await screen.findByRole("heading", { name: "Änderungen anwenden" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/apply");
    expect(backend.state.profiles.find((p) => p.id === "kunde-b")!.archived).toBe(true);
  });

  it("shows the generated config", async () => {
    const { user } = await renderApp("/profile/privat");
    await user.click(await screen.findByRole("tab", { name: "Generierte Config" }));
    expect(await screen.findByText("~/.ssh/config.d/keybund.conf")).toBeInTheDocument();
    expect(screen.getAllByText("Auszug")).toHaveLength(2);
    expect(screen.getByText("verwaltet von Keybund")).toBeInTheDocument();
  });

  it("opens the provider page from a host row", async () => {
    const { user, backend } = await renderApp("/profile/sparkasse");
    await user.click(await screen.findByRole("button", { name: /Key eintragen/ }));
    expect(backend.state.openedUrls).toEqual(["https://gitlab.kunde.de/-/user_settings/ssh_keys"]);
  });

  it("shows a not-found page", async () => {
    await renderApp("/profile/nope");
    expect(await screen.findByRole("heading", { name: "Profil nicht gefunden" })).toBeInTheDocument();
  });
});
