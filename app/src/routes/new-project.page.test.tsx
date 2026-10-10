import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "@/test/render";

describe("wizard", () => {
  it("walks through all steps for a GitHub project", async () => {
    const { user, backend } = await renderApp("/new");
    const next = () => user.click(screen.getByRole("button", { name: /Weiter/ }));

    await user.type(await screen.findByLabelText("Projektname"), "Kunde C");
    expect(await screen.findByText("kunde-c")).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: "Violett" }));
    await user.click(screen.getByRole("switch", { name: "Als Default-Profil" }));
    await next();

    expect(screen.getByLabelText("Hostname")).toHaveValue("github.com");
    await user.click(screen.getByRole("radio", { name: "GitLab" }));
    expect(screen.getByLabelText("Hostname")).toHaveValue("gitlab.com");
    await user.clear(screen.getByLabelText("Hostname"));
    await user.type(screen.getByLabelText("Hostname"), "gitlab.kunde-c.de");
    await next();

    expect(screen.getByLabelText("Ordner")).toHaveValue("~/dev/kunde-c/");
    await user.click(screen.getByRole("button", { name: /Durchsuchen/ }));
    expect(await screen.findByDisplayValue("~/dev/neues-projekt/")).toBeInTheDocument();
    await next();

    await user.type(screen.getByLabelText("Name"), "Tristan");
    await user.type(screen.getByLabelText("E-Mail"), "t@kunde-c.de");
    await user.click(screen.getByRole("switch", { name: "Commits signieren" }));
    await next();

    expect(await screen.findByRole("heading", { name: "SSH-Key für Kunde C" })).toBeInTheDocument();
    expect(await screen.findByText("Vorschau der Änderungen")).toBeInTheDocument();
    const steps = screen.getByRole("list", { name: "Schritte" });
    expect(within(steps).getByText("gitlab.kunde-c.de")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Anwenden & Pubkey kopieren/ }));

    expect(await screen.findByRole("heading", { name: "Key eintragen & testen" })).toBeInTheDocument();
    expect(backend.state.openedUrls).toEqual(["https://gitlab.kunde-c.de/-/user_settings/ssh_keys"]);
    await user.click(screen.getByRole("button", { name: "GitLab-Seite öffnen" }));
    expect(backend.state.openedUrls).toHaveLength(2);
    await user.click(screen.getByRole("button", { name: "Ich hab ihn eingetragen" }));
    expect(await screen.findByText("Verbunden als @tristan")).toBeInTheDocument();

    const created = backend.state.profiles.find((p) => p.id === "kunde-c")!;
    expect(created).toMatchObject({
      color: "violet",
      folders: ["~/dev/neues-projekt/"],
      git: { name: "Tristan", email: "t@kunde-c.de", sign: true },
      hosts: [{ provider: "gitlab", hostname: "gitlab.kunde-c.de" }],
    });
    expect(backend.state.defaultProfile).toBe("kunde-c");
  });

  it("shows validation errors and a failed test", async () => {
    const { user } = await renderApp("/new");
    await user.click(await screen.findByRole("button", { name: /Weiter/ }));
    expect(screen.getByRole("alert")).toHaveTextContent("Bitte einen Projektnamen eingeben.");
    await user.type(screen.getByLabelText("Projektname"), "Kunde B");
    await user.click(screen.getByRole("button", { name: /Weiter/ }));
    expect(await screen.findByText("Ein Profil „kunde-b“ gibt es schon.")).toBeInTheDocument();
  });

  it("creates a server with an existing key", async () => {
    const { user, backend } = await renderApp("/new?key=~/.ssh/id_ed25519");
    const next = () => user.click(screen.getByRole("button", { name: /Weiter/ }));
    await user.type(await screen.findByLabelText("Projektname"), "NAS");
    await next();
    await user.click(screen.getByRole("radio", { name: "Server" }));
    await user.type(screen.getByLabelText("Hostname"), "nas.local");
    await user.type(screen.getByLabelText("SSH-Alias"), "nas");
    await user.type(screen.getByLabelText(/Port/), "2222");
    await next();
    await user.clear(screen.getByLabelText("Ordner"));
    await next();
    await next();
    expect(await screen.findByRole("radio", { name: /^~\/\.ssh\/id_ed25519/ })).toBeChecked();
    await user.click(screen.getByRole("radio", { name: /Neuen Key erzeugen/ }));
    expect(screen.getByLabelText("Kommentar")).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: /Vorhandenen Key verwenden/ }));
    await user.click(screen.getByRole("button", { name: "Zurück" }));
    await next();
    await user.click(await screen.findByRole("button", { name: /Anwenden & Pubkey kopieren/ }));
    expect(await screen.findByText(/ssh-copy-id/)).toBeInTheDocument();
    expect(backend.state.profiles.find((p) => p.id === "nas")).toMatchObject({
      key: { path: "~/.ssh/id_ed25519" },
      git: null,
      folders: [],
      hosts: [{ alias: "nas", hostname: "nas.local", port: 2222, provider: "generic" }],
    });
  });
});
