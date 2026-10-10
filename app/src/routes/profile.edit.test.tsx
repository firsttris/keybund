import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "@/test/render";

describe("profile editing", () => {
  it("adds a server, installs the key there and tests it", async () => {
    const { user, backend } = await renderApp("/profile/kunde-b");
    const card = await screen.findByRole("region", { name: "Server" });
    expect(within(card).getByText(/Kein Server/)).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "+ Server" }));
    const form = screen.getByRole("form", { name: "Server hinzufügen" });
    expect(within(form).queryByLabelText("Anbieter")).not.toBeInTheDocument();
    await user.type(within(form).getByLabelText("Hostname oder IP"), "10.0.0.7");
    await user.type(within(form).getByLabelText(/Alias/), "srv");
    await user.type(within(form).getByLabelText("Benutzer"), "root");
    await user.type(within(form).getByLabelText("Port"), "2200");
    await user.click(within(form).getByRole("button", { name: "Server speichern" }));
    expect(await within(card).findByText("srv")).toBeInTheDocument();
    expect(backend.state.profiles.find((p) => p.id === "kunde-b")!.hosts[1]).toEqual({
      provider: "generic",
      hostname: "10.0.0.7",
      alias: "srv",
      user: "root",
      port: 2200,
    });
    expect(screen.getByText(/Gilt für/)).toHaveTextContent("Server srv");

    await user.click(within(card).getByRole("button", { name: "Key eintragen" }));
    expect(await within(card).findByText("Key auf srv eingetragen")).toBeInTheDocument();
    expect(backend.state.installedKeys).toEqual(["10.0.0.7:~/.ssh/kunde-b_ed25519"]);
    await user.click(within(card).getByRole("button", { name: "Key eintragen" }));
    expect(await within(card).findByText("Der Key war auf srv schon eingetragen")).toBeInTheDocument();

    await user.click(within(card).getByRole("button", { name: "Testen" }));
    expect(await within(card).findByText(/^Test: /)).toBeInTheDocument();

    await user.click(within(card).getByRole("button", { name: "Server srv entfernen" }));
    expect(await screen.findByText("Configs aktuell")).toBeInTheDocument();
  });

  it("reports an unreachable server", async () => {
    const { user } = await renderApp("/profile/privat", (b) => {
      b.state.unreachable = ["1.2.3.4"];
    });
    const card = await screen.findByRole("region", { name: "Server" });
    await user.click(within(card).getByRole("button", { name: "Key eintragen" }));
    expect(await within(card).findByText("meinserver ist nicht erreichbar")).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Testen" }));
    expect(await within(card).findByText("Test: meinserver kennt diesen Key nicht")).toBeInTheDocument();
  });

  it("adds a git account", async () => {
    const { user, backend } = await renderApp("/profile/kunde-b");
    const card = await screen.findByRole("region", { name: "Git-Konten" });
    await user.click(within(card).getByRole("button", { name: "+ Konto" }));
    const form = screen.getByRole("form", { name: "Git-Konto hinzufügen" });
    expect(within(form).queryByLabelText("Benutzer")).not.toBeInTheDocument();
    await user.selectOptions(within(form).getByLabelText("Anbieter"), "gitlab");
    expect(within(form).getByLabelText("Hostname")).toHaveValue("gitlab.com");
    await user.click(within(form).getByRole("button", { name: "Konto speichern" }));
    expect(await within(card).findByText("gitlab.com")).toBeInTheDocument();
    expect(backend.state.profiles.find((p) => p.id === "kunde-b")!.hosts.map((h) => h.provider)).toEqual(["github", "gitlab"]);
    await user.click(within(card).getByRole("button", { name: "Konto gitlab.com entfernen" }));
    expect(within(card).queryByText("gitlab.com")).not.toBeInTheDocument();
  });

  it("rejects a host that another profile already has", async () => {
    const { user } = await renderApp("/profile/kunde-b");
    const card = await screen.findByRole("region", { name: "Server" });
    await user.click(within(card).getByRole("button", { name: "+ Server" }));
    await user.type(screen.getByLabelText("Hostname oder IP"), "1.2.3.4");
    await user.click(screen.getByRole("button", { name: "Server speichern" }));
    expect(await within(card).findByRole("alert")).toHaveTextContent("mehreren Profilen");
  });

  it("cancels the host form", async () => {
    const { user } = await renderApp("/profile/kunde-b");
    await user.click(await screen.findByRole("button", { name: "+ Server" }));
    await user.click(screen.getByRole("button", { name: "Abbrechen" }));
    expect(screen.queryByRole("form", { name: "Server hinzufügen" })).not.toBeInTheDocument();
  });

  it("shows a server-only profile without git cards", async () => {
    await renderApp("/profile/hetzner", (b) => {
      b.state.profiles.push({
        id: "hetzner",
        name: "Hetzner",
        color: "rose",
        key: { path: "~/.ssh/hetzner_ed25519", comment: null },
        git: null,
        folders: [],
        archived: false,
        hosts: [{ provider: "generic", hostname: "5.6.7.8", alias: null, user: "root", port: null }],
      });
    });
    expect(await screen.findByRole("heading", { level: 1, name: "Hetzner" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Server" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Git-Identität" })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Projektordner" })).not.toBeInTheDocument();
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
    await user.click(await screen.findByRole("button", { name: "Als Standard setzen" }));
    expect(await screen.findByText("Standard", { selector: "header *" })).toBeInTheDocument();
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

  it("explains when the browser cannot be opened", async () => {
    const { user, backend } = await renderApp("/profile/sparkasse", (b) => {
      b.state.openFails = true;
    });
    await user.click(await screen.findByRole("button", { name: "Bei GitLab hinzufügen" }));
    expect(await screen.findByText("Browser konnte nicht geöffnet werden")).toBeInTheDocument();
    expect(screen.getByText(/xdg-open.*https:\/\/gitlab\.kunde\.de\/-\/user_settings\/ssh_keys/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Link kopieren" })).toBeInTheDocument();
    expect(backend.state.openedUrls).toEqual([]);
  });

  it("shows a not-found page", async () => {
    await renderApp("/profile/nope");
    expect(await screen.findByRole("heading", { name: "Profil nicht gefunden" })).toBeInTheDocument();
  });
});
