import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "@/test/render";

type User = Awaited<ReturnType<typeof renderApp>>["user"];

async function choose(user: User, kind: RegExp) {
  await user.click(await screen.findByRole("radio", { name: kind }));
  await user.click(screen.getByRole("button", { name: /Weiter/ }));
}
const GIT = /^Git-Projekt/;
const SERVER = /^Server/;

describe("wizard", () => {
  it("walks through all steps for a GitHub project", async () => {
    const { user, backend } = await renderApp("/new");
    const next = () => user.click(screen.getByRole("button", { name: /Weiter/ }));

    await choose(user, GIT);
    await user.type(await screen.findByLabelText("Projektname"), "Kunde C");
    expect(await screen.findByText("kunde-c")).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: "Violett" }));
    await user.click(screen.getByRole("switch", { name: "Als Standard-Profil" }));
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
    // Self-hosted GitLab: a token or by hand, nothing opens by itself.
    expect(await screen.findByLabelText("Zugriffstoken")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /anmelden/ })).not.toBeInTheDocument();
    expect(backend.state.openedUrls).toEqual([]);
    await user.click(screen.getByRole("button", { name: "GitLab-Seite öffnen" }));
    expect(backend.state.openedUrls).toEqual(["https://gitlab.kunde-c.de/-/user_settings/ssh_keys"]);
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

  it("updates folder and key comment when the name changes, unless edited", async () => {
    const { user } = await renderApp("/new");
    const next = () => user.click(screen.getByRole("button", { name: /Weiter/ }));
    const back = () => user.click(screen.getByRole("button", { name: "Zurück" }));
    await choose(user, GIT);
    await user.type(await screen.findByLabelText("Projektname"), "Kunde C");
    await screen.findByText("kunde-c");
    await next();
    await next();
    expect(screen.getByLabelText("Ordner")).toHaveValue("~/dev/kunde-c/");

    await back();
    await back();
    const name = screen.getByLabelText("Projektname");
    await user.clear(name);
    await user.type(name, "Kunde D");
    await screen.findByText("kunde-d");
    await next();
    await next();
    expect(screen.getByLabelText("Ordner")).toHaveValue("~/dev/kunde-d/");

    // Edited by hand: a later rename keeps it.
    await user.clear(screen.getByLabelText("Ordner"));
    await user.type(screen.getByLabelText("Ordner"), "~/work/d/");
    await back();
    await back();
    await user.type(screen.getByLabelText("Projektname"), "2");
    await screen.findByText("kunde-d2");
    await next();
    await next();
    expect(screen.getByLabelText("Ordner")).toHaveValue("~/work/d/");
    await next();
    await user.type(screen.getByLabelText("Name"), "T");
    await user.type(screen.getByLabelText("E-Mail"), "t@d.de");
    await next();
    expect(await screen.findByLabelText("Kommentar")).toHaveValue("tristan@zentrale-kunde-d2");
    expect(screen.getByText("ed25519 · ~/.ssh/kunde-d2_ed25519")).toBeInTheDocument();
  });

  it("adds the key to GitHub after a login in the browser and tests it", async () => {
    const { user, backend } = await renderApp("/new");
    const next = () => user.click(screen.getByRole("button", { name: /Weiter/ }));
    await choose(user, GIT);
    await user.type(await screen.findByLabelText("Projektname"), "Kunde G");
    await screen.findByText("kunde-g");
    await next();
    await next();
    await next();
    await user.type(screen.getByLabelText("Name"), "T");
    await user.type(screen.getByLabelText("E-Mail"), "t@g.de");
    await next();
    expect(await screen.findByText(/trägt Keybund ihn in dein GitHub-Konto ein/)).toBeInTheDocument();
    await screen.findByText("Vorschau der Änderungen");
    await user.click(screen.getByRole("button", { name: /Anwenden & Pubkey kopieren/ }));

    await user.click(await screen.findByRole("button", { name: "Bei GitHub anmelden & eintragen" }));
    expect(await screen.findByTestId("user-code")).toHaveTextContent("WDJB-MJHT");
    expect(backend.state.openedUrls).toEqual(["https://github.com/login/device"]);
    expect(await screen.findByText("Key bei GitHub eingetragen (@tristan)")).toBeInTheDocument();
    // The connection test runs right after the upload.
    expect(await screen.findByText("Verbunden als @tristan")).toBeInTheDocument();
    expect(backend.state.uploadedKeys).toEqual(["github.com:~/.ssh/kunde-g_ed25519"]);
  });

  it("opens the page right away for Bitbucket and tells when it did not open", async () => {
    const { user } = await renderApp("/new", (b) => {
      b.state.openFails = true;
    });
    const next = () => user.click(screen.getByRole("button", { name: /Weiter/ }));
    await choose(user, GIT);
    await user.type(await screen.findByLabelText("Projektname"), "Kunde F");
    await screen.findByText("kunde-f");
    await next();
    await user.click(screen.getByRole("radio", { name: "Bitbucket" }));
    await next();
    await next();
    await user.type(screen.getByLabelText("Name"), "T");
    await user.type(screen.getByLabelText("E-Mail"), "t@f.de");
    await next();
    await screen.findByText("Vorschau der Änderungen");
    await user.click(screen.getByRole("button", { name: /Anwenden & Pubkey kopieren/ }));
    const alert = await screen.findByText("Die Bitbucket-Seite hat sich nicht geöffnet");
    expect(alert.parentElement).toHaveTextContent("https://bitbucket.org/account/settings/ssh-keys/");
    expect(alert.parentElement).toHaveTextContent("xdg-open");
    expect(screen.queryByRole("region", { name: "Automatisch eintragen" })).not.toBeInTheDocument();
  });

  it("shows validation errors", async () => {
    const { user } = await renderApp("/new");
    await user.click(await screen.findByRole("button", { name: /Weiter/ }));
    expect(screen.getByRole("alert")).toHaveTextContent("Bitte wählen, wofür der Key ist.");
    await choose(user, GIT);
    await user.click(screen.getByRole("button", { name: /Weiter/ }));
    expect(screen.getByRole("alert")).toHaveTextContent("Bitte einen Projektnamen eingeben.");
    await user.type(screen.getByLabelText("Projektname"), "Kunde B");
    await user.click(screen.getByRole("button", { name: /Weiter/ }));
    expect(await screen.findByText("Ein Profil „kunde-b“ gibt es schon.")).toBeInTheDocument();
  });

  it("creates a server profile with a new key and installs it", async () => {
    const { user, backend } = await renderApp("/new");
    const next = () => user.click(screen.getByRole("button", { name: /Weiter/ }));
    await choose(user, SERVER);
    const steps = screen.getByRole("list", { name: "Schritte" });
    expect(within(steps).getAllByRole("listitem")).toHaveLength(4);
    expect(screen.queryByLabelText("Projektname")).not.toBeInTheDocument();

    await next();
    expect(screen.getByRole("alert")).toHaveTextContent("Bitte Hostname oder IP eingeben.");
    await user.type(screen.getByLabelText("Hostname oder IP"), "192.168.1.5");
    await user.type(screen.getByLabelText(/SSH-Alias/), "nas");
    await user.type(screen.getByLabelText("Benutzer"), "admin");
    await user.type(screen.getByLabelText(/Port/), "2222");
    await next();

    expect(await screen.findByRole("heading", { name: "SSH-Key für nas" })).toBeInTheDocument();
    expect(screen.getByText("ed25519 · ~/.ssh/nas_ed25519")).toBeInTheDocument();
    expect(await screen.findByLabelText("Kommentar")).toHaveValue("tristan@zentrale-nas");
    expect(await screen.findByText("Vorschau der Änderungen")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^Anwenden/ }));

    expect(await screen.findByRole("heading", { name: "Key auf dem Server eintragen" })).toBeInTheDocument();
    expect(backend.state.openedUrls).toEqual([]);
    await user.click(screen.getByRole("button", { name: "Key auf nas eintragen" }));
    expect(await screen.findByText("Key auf nas eingetragen")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Verbindung testen" }));
    expect(await screen.findByText("Fertig – ssh nas nimmt jetzt diesen Key.")).toBeInTheDocument();
    expect(backend.state.profiles.find((p) => p.id === "nas")).toMatchObject({
      key: { path: "~/.ssh/nas_ed25519" },
      git: null,
      folders: [],
      hosts: [{ alias: "nas", hostname: "192.168.1.5", user: "admin", port: 2222, provider: "generic" }],
    });
    expect(backend.state.defaultProfile).toBe("privat");
    expect(screen.getByRole("link", { name: "Zum Profil" })).toHaveAttribute("href", "/profile/nas");
  });

  it("adds a server to an existing profile and its key", async () => {
    const { user, backend } = await renderApp("/new");
    await choose(user, SERVER);
    await user.type(screen.getByLabelText("Hostname oder IP"), "10.1.1.1");
    await user.click(screen.getByRole("button", { name: /Weiter/ }));
    await user.click(await screen.findByRole("radio", { name: /Key eines Profils verwenden/ }));
    await user.click(screen.getByRole("button", { name: /^Anwenden/ }));
    expect(screen.getByRole("alert")).toHaveTextContent("Bitte ein Profil wählen.");
    await user.selectOptions(screen.getByLabelText("Profil"), "privat");
    expect(await screen.findByText("Vorschau der Änderungen")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^Anwenden/ }));
    expect(await screen.findByRole("button", { name: "Key auf 10.1.1.1 eintragen" })).toBeInTheDocument();
    expect(backend.state.profiles.find((p) => p.id === "10-1-1-1")).toBeUndefined();
    expect(backend.state.profiles.find((p) => p.id === "privat")!.hosts.map((h) => h.hostname)).toEqual([
      "github.com",
      "1.2.3.4",
      "10.1.1.1",
    ]);
    const steps = screen.getByRole("list", { name: "Schritte" });
    expect(within(steps).getByText("Privat")).toBeInTheDocument();
  });

  it("creates a server with an existing key", async () => {
    const { user, backend } = await renderApp("/new?key=~/.ssh/id_ed25519");
    await choose(user, SERVER);
    await user.type(screen.getByLabelText("Hostname oder IP"), "nas.local");
    await user.type(screen.getByLabelText(/Name/), "NAS");
    await user.click(screen.getByRole("button", { name: /Weiter/ }));
    expect(await screen.findByRole("radio", { name: /^~\/\.ssh\/id_ed25519/ })).toBeChecked();
    await user.click(screen.getByRole("radio", { name: /Neuen Key erzeugen/ }));
    expect(screen.getByLabelText("Kommentar")).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: /Vorhandenen Key verwenden/ }));
    await user.click(await screen.findByRole("button", { name: /^Anwenden/ }));
    expect(await screen.findByRole("heading", { name: "Key auf dem Server eintragen" })).toBeInTheDocument();
    expect(backend.state.profiles.find((p) => p.id === "nas")).toMatchObject({
      name: "NAS",
      key: { path: "~/.ssh/id_ed25519" },
      hosts: [{ alias: null, hostname: "nas.local", provider: "generic" }],
    });
  });

  it("refuses a server name that is taken unless its key is reused", async () => {
    const { user } = await renderApp("/new");
    await choose(user, SERVER);
    await user.type(screen.getByLabelText("Hostname oder IP"), "10.2.2.2");
    await user.type(screen.getByLabelText(/Name/), "Privat");
    await user.click(screen.getByRole("button", { name: /Weiter/ }));
    await user.click(await screen.findByRole("button", { name: /^Anwenden/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("dessen Key verwenden");
  });
});
