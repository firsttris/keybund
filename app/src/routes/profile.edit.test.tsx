import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { Profile } from "@/lib/bindings";
import { renderApp } from "@/test/render";

const usage = () => screen.findByRole("region", { name: "Wo wird dieser Key benutzt?" });

/** The usage row whose text matches. */
async function row(text: RegExp | string): Promise<HTMLElement> {
  const items = await within(await usage()).findAllByRole("listitem");
  const match = items.find((li) => (typeof text === "string" ? li.textContent?.includes(text) : text.test(li.textContent ?? "")));
  if (!match) throw new Error(`Keine Zeile ${text}`);
  return match;
}

const emptyKey: Profile = {
  id: "neu",
  name: "Neu",
  color: "rose",
  key: { path: "~/.ssh/id_ed25519", comment: null },
  git: null,
  folders: [],
  archived: false,
  hosts: [],
};

describe("key page", () => {
  it("lists everything a key is used for", async () => {
    await renderApp("/profile/privat");
    const list = within(await usage()).getByRole("list", { name: "Verwendungen" });
    const text = list.textContent ?? "";
    expect(text).toContain("Standard für Git");
    expect(text).toContain("~/dev/privat/");
    expect(text).toContain("GitHub-Konto");
    expect(text).toContain("Server meinserver");
    expect(text).toContain("ssh meinserver verbindet zu tristan@1.2.3.4");
  });

  it("starts an unused key with the three choices and adds a git folder in one go", async () => {
    const { user, backend } = await renderApp("/profile/neu", (b) => {
      b.state.profiles.push(structuredClone(emptyKey));
    });
    const card = await usage();
    expect(within(card).queryByRole("list", { name: "Verwendungen" })).not.toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: /^Git-Ordner/ }));
    const form = within(card).getByRole("form", { name: "Git-Ordner hinzufügen" });
    // Name and e-mail are suggested from ~/.gitconfig; the customer's e-mail replaces it.
    expect(within(form).getByLabelText("Name in Commits")).toHaveValue("Tristan Teufel");
    expect(within(form).getByLabelText("E-Mail in Commits")).toHaveValue("tristanteufel@example.org");
    await user.clear(within(form).getByLabelText("E-Mail in Commits"));
    expect(within(form).getByRole("button", { name: "Ordner speichern" })).toBeDisabled();
    await user.type(within(form).getByLabelText("E-Mail in Commits"), "t@neu.de");
    await user.selectOptions(within(form).getByLabelText("Wo liegt der Code?"), "gitlab");
    await user.click(within(form).getByRole("button", { name: "Ordner speichern" }));
    expect(await row("~/dev/neu/")).toBeInTheDocument();
    expect(await row("GitLab-Konto")).toBeInTheDocument();
    expect(backend.state.profiles.find((p) => p.id === "neu")).toMatchObject({
      folders: ["~/dev/neu/"],
      git: { name: "Tristan Teufel", email: "t@neu.de", sign: false },
      hosts: [{ provider: "gitlab", hostname: "gitlab.com" }],
    });
    expect(screen.getByRole("region", { name: "Git-Identität" })).toBeInTheDocument();
  });

  it("follows ~/.gitconfig when the folder keeps the suggested identity", async () => {
    const { user, backend } = await renderApp("/profile/neu", (b) => {
      b.state.profiles.push({ ...structuredClone(emptyKey), hosts: [{ provider: "github", hostname: "github.com", alias: null, user: null, port: null }] });
    });
    await user.click(within(await usage()).getByRole("button", { name: /^Git-Ordner/ }));
    await user.click(screen.getByRole("button", { name: "Ordner speichern" }));
    expect(await row("~/dev/neu/")).toBeInTheDocument();
    expect(backend.state.profiles.find((p) => p.id === "neu")!.git).toBeNull();
    expect(backend.state.applied.get("~/.config/keybund/git/neu.gitconfig")).not.toContain("[user]");
    // The fields stay empty and show the global identity.
    const card = screen.getByRole("region", { name: "Git-Identität" });
    expect(within(card).getByLabelText("E-Mail")).toHaveValue("");
    expect(within(card).getByLabelText("E-Mail")).toHaveAttribute("placeholder", "tristanteufel@example.org");
    expect(within(card).getByText(/Dafür braucht der Key eine eigene E-Mail/)).toBeInTheDocument();
  });

  it("shows the global identity for a key without git folders", async () => {
    await renderApp("/profile/privat", (b) => {
      Object.assign(b.state.profiles.find((p) => p.id === "privat")!, { folders: [], git: null });
    });
    const card = await screen.findByRole("region", { name: "Git-Identität" });
    expect(card).toHaveTextContent("Dieser Key hat keinen Git-Ordner");
    expect(card).toHaveTextContent("Tristan Teufel <tristanteufel@example.org>");
    expect(within(card).queryByRole("textbox")).not.toBeInTheDocument();
  });

  it("says when ~/.gitconfig has no identity either", async () => {
    await renderApp("/profile/privat", (b) => {
      Object.assign(b.state.profiles.find((p) => p.id === "privat")!, { folders: [], git: null });
      b.state.globalIdentity = { name: null, email: null };
    });
    const card = await screen.findByRole("region", { name: "Git-Identität" });
    expect(card).toHaveTextContent("dort ist aber keine eingetragen, Git fragt beim ersten Commit danach");
  });

  it("hides the identity for an unused key", async () => {
    await renderApp("/profile/neu", (b) => {
      b.state.profiles.push(structuredClone(emptyKey));
      b.state.globalIdentity = { name: null, email: null };
    });
    await usage();
    expect(screen.queryByRole("region", { name: "Git-Identität" })).not.toBeInTheDocument();
  });

  it("makes an unused key the standard", async () => {
    const { user, backend } = await renderApp("/profile/neu", (b) => {
      b.state.profiles.push(structuredClone(emptyKey));
    });
    await user.click(within(await usage()).getByRole("button", { name: /als Standard für Git benutzen/ }));
    expect(await row("Standard für Git")).toBeInTheDocument();
    expect(backend.state.defaultProfile).toBe("neu");
    await user.click(within(await row("Standard für Git")).getByRole("button", { name: "Nicht mehr Standard" }));
    await waitFor(() => expect(backend.state.defaultProfile).toBeNull());
  });

  it("adds a server, installs the key there and tests it", async () => {
    const { user, backend } = await renderApp("/profile/kunde-b");
    await user.click(within(await usage()).getByRole("button", { name: "Server" }));
    const form = screen.getByRole("form", { name: "Server hinzufügen" });
    expect(within(form).queryByLabelText("Anbieter")).not.toBeInTheDocument();
    await user.type(within(form).getByLabelText("Hostname oder IP"), "10.0.0.7");
    await user.type(within(form).getByLabelText(/Alias/), "srv");
    await user.type(within(form).getByLabelText("Benutzer"), "root");
    await user.type(within(form).getByLabelText("Port"), "2200");
    await user.click(within(form).getByRole("button", { name: "Server speichern" }));
    const server = await row("Server srv");
    expect(server).toHaveTextContent("ssh srv verbindet zu root@10.0.0.7:2200");
    expect(backend.state.profiles.find((p) => p.id === "kunde-b")!.hosts[1]).toEqual({
      provider: "generic",
      hostname: "10.0.0.7",
      alias: "srv",
      user: "root",
      port: 2200,
    });

    // First contact: the host key is shown; declining stops everything.
    await user.click(within(server).getByRole("button", { name: "Key eintragen" }));
    let dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent("Erste Verbindung zu [10.0.0.7]:2200");
    expect(dialog).toHaveTextContent(/SHA256:/);
    expect(dialog).toHaveTextContent("Wird gespeichert in ~/.ssh/known_hosts.d/kunde-b");
    await user.click(within(dialog).getByRole("button", { name: "Abbrechen" }));
    expect(await within(server).findByText("Host-Key nicht bestätigt – keine Verbindung aufgebaut.")).toBeInTheDocument();
    expect(backend.state.installedKeys).toEqual([]);

    await user.click(within(server).getByRole("button", { name: "Key eintragen" }));
    dialog = await screen.findByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: "Fingerprint stimmt" }));
    expect(await within(server).findByText("Key auf srv eingetragen")).toBeInTheDocument();
    expect(backend.state.knownHosts).toContain("[10.0.0.7]:2200");
    expect(backend.state.installedKeys).toEqual(["10.0.0.7:~/.ssh/kunde-b_ed25519"]);
    await user.click(within(server).getByRole("button", { name: "Key eintragen" }));
    expect(await within(server).findByText("Der Key war auf srv schon eingetragen")).toBeInTheDocument();

    await user.click(within(server).getByRole("button", { name: "Testen" }));
    expect(await within(server).findByTestId("connection-status")).toBeInTheDocument();

    await user.click(within(server).getByRole("button", { name: "Server srv entfernen" }));
    expect(await screen.findByText("Configs aktuell")).toBeInTheDocument();
  });

  it("reports an unreachable server", async () => {
    const { user } = await renderApp("/profile/privat", (b) => {
      b.state.unreachable = ["1.2.3.4"];
    });
    const server = await row("Server meinserver");
    await user.click(within(server).getByRole("button", { name: "Key eintragen" }));
    expect(await within(server).findByText("meinserver ist nicht erreichbar")).toBeInTheDocument();
    await user.click(within(server).getByRole("button", { name: "Testen" }));
    expect(await within(server).findByTestId("connection-status")).toHaveTextContent("meinserver kennt diesen Key nicht");
    expect(within(server).getByRole("list", { name: "Ablauf" })).toHaveTextContent("Keine Verbindung: Connection timed out");
  });

  it("tests a git account and shows what happened", async () => {
    const { user } = await renderApp("/profile/kunde-b");
    const account = await row("GitHub-Konto");
    await user.click(within(account).getByRole("button", { name: "Testen" }));
    expect(await within(account).findByTestId("connection-status")).toHaveTextContent("GitHub kennt diesen Key nicht");
    const steps = within(account).getByRole("list", { name: "Ablauf" });
    expect(steps).toHaveTextContent("Anmelden als Benutzer „git“");
    expect(within(account).getByRole("button", { name: "Befehl kopieren" })).toBeInTheDocument();
    // The failed test shows up in the sidebar.
    expect(await within(screen.getByRole("navigation", { name: "Keys" })).findByText("Test fehlgeschlagen")).toBeInTheDocument();
  });

  it("adds and removes a git account", async () => {
    const { user, backend } = await renderApp("/profile/kunde-b");
    await user.click(within(await usage()).getByRole("button", { name: "Git-Konto" }));
    const form = screen.getByRole("form", { name: "Git-Konto hinzufügen" });
    expect(within(form).queryByLabelText("Benutzer")).not.toBeInTheDocument();
    await user.selectOptions(within(form).getByLabelText("Anbieter"), "gitlab");
    expect(within(form).getByLabelText("Hostname")).toHaveValue("gitlab.com");
    await user.click(within(form).getByRole("button", { name: "Konto speichern" }));
    const gitlab = await row("GitLab-Konto");
    expect(backend.state.profiles.find((p) => p.id === "kunde-b")!.hosts.map((h) => h.provider)).toEqual(["github", "gitlab"]);
    await user.click(within(gitlab).getByRole("button", { name: "Konto gitlab.com entfernen" }));
    await waitFor(() => expect(within(screen.getByRole("list", { name: "Verwendungen" })).queryByText(/GitLab-Konto/)).not.toBeInTheDocument());
  });

  it("rejects a host that another key already has", async () => {
    const { user } = await renderApp("/profile/kunde-b");
    const card = await usage();
    await user.click(within(card).getByRole("button", { name: "Server" }));
    await user.type(screen.getByLabelText("Hostname oder IP"), "1.2.3.4");
    await user.click(screen.getByRole("button", { name: "Server speichern" }));
    expect(await within(card).findByRole("alert")).toHaveTextContent("Host „1.2.3.4“ gehört schon zu einem anderen Key");
  });

  it("cancels the host form", async () => {
    const { user } = await renderApp("/profile/kunde-b");
    await user.click(within(await usage()).getByRole("button", { name: "Server" }));
    await user.click(screen.getByRole("button", { name: "Abbrechen" }));
    expect(screen.queryByRole("form", { name: "Server hinzufügen" })).not.toBeInTheDocument();
  });

  it("explains a git account behind an alias", async () => {
    await renderApp("/profile/pflege", (b) => {
      b.state.profiles.push({
        ...structuredClone(emptyKey),
        id: "pflege",
        name: "Nebenjob",
        key: { path: "~/.ssh/nebenjob/id_ed25519", comment: null },
        hosts: [{ provider: "github", hostname: "github.com", alias: "nebenjob", user: null, port: null }],
      });
    });
    expect(await row("GitHub-Konto nebenjob")).toHaveTextContent("Repos mit git@nebenjob:… nutzen diesen Key (github.com)");
  });

  it("shows no git identity for a key that only serves servers", async () => {
    await renderApp("/profile/hetzner", (b) => {
      b.state.profiles.push({
        ...structuredClone(emptyKey),
        id: "hetzner",
        name: "Hetzner",
        key: { path: "~/.ssh/hetzner_ed25519", comment: null },
        hosts: [{ provider: "generic", hostname: "5.6.7.8", alias: null, user: "root", port: null }],
      });
    });
    expect(await screen.findByRole("heading", { level: 1, name: "Hetzner" })).toBeInTheDocument();
    expect(await row("Server 5.6.7.8")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Git-Identität" })).not.toBeInTheDocument();
  });

  it("adds and removes folders", async () => {
    const { user } = await renderApp("/profile/kunde-b");
    await user.click(within(await usage()).getByRole("button", { name: "Git-Ordner" }));
    await user.type(screen.getByLabelText("Ordner"), "~/dev/privat/sub");
    await user.click(screen.getByRole("button", { name: "Ordner speichern" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Git-Ordner überschneiden sich");
    await user.clear(screen.getByLabelText("Ordner"));
    await user.click(screen.getByRole("button", { name: "Durchsuchen" }));
    expect(await screen.findByDisplayValue("~/dev/neues-projekt/")).toBeInTheDocument();
    await user.clear(screen.getByLabelText("Ordner"));
    await user.type(screen.getByLabelText("Ordner"), "~/work/b");
    await user.click(screen.getByRole("button", { name: "Ordner speichern" }));
    const folder = await row("~/work/b/");
    await user.click(within(folder).getByRole("button", { name: "Ordner ~/work/b/ entfernen" }));
    await waitFor(() => expect(screen.queryByText("~/work/b/")).not.toBeInTheDocument());
  });

  it("saves the git identity", async () => {
    const { user, backend } = await renderApp("/profile/kunde-b");
    const name = await screen.findByLabelText("Name");
    await user.clear(name);
    await user.type(name, "T. T.");
    await user.click(screen.getByRole("button", { name: "Identität speichern" }));
    expect(await screen.findByText("Gespeichert")).toBeInTheDocument();
    expect(backend.state.profiles.find((p) => p.id === "kunde-b")!.git!.name).toBe("T. T.");
    expect(backend.state.applied.get("~/.config/keybund/git/kunde-b.gitconfig")).toContain("name = T. T.");
  });

  it("sets the standard and archives", async () => {
    const { user, backend, router } = await renderApp("/profile/kunde-b");
    await user.click(within(await usage()).getByRole("button", { name: "Als Standard" }));
    expect(await screen.findByText("Standard", { selector: "header *" })).toBeInTheDocument();
    expect(backend.state.defaultProfile).toBe("kunde-b");

    await user.click(screen.getByRole("button", { name: "Archivieren" }));
    const dialog = await screen.findByRole("alertdialog");
    await user.click(within(dialog).getByRole("button", { name: "Archivieren" }));
    expect(await screen.findByRole("heading", { name: "Übersicht" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/");
    expect(backend.state.profiles.find((p) => p.id === "kunde-b")!.archived).toBe(true);
    expect(backend.state.applied.has("~/.config/keybund/git/kunde-b.gitconfig")).toBe(false);
  });

  it("offers to add the key to the account when the test fails", async () => {
    const { user, backend } = await renderApp("/profile/kunde-b");
    const github = await row("GitHub-Konto");
    await user.click(within(github).getByRole("button", { name: "Testen" }));
    expect(await within(github).findByTestId("connection-status")).toHaveTextContent("GitHub kennt diesen Key nicht");
    await user.click(within(github).getByRole("button", { name: "Pubkey kopieren & GitHub-Seite öffnen" }));
    expect(await within(github).findByText("Pubkey ist kopiert.")).toBeInTheDocument();
    expect(backend.state.clipboard).toMatch(/^ssh-ed25519 .* tristan@zentrale-kunde-b$/);
    expect(backend.state.openedUrls).toEqual(["https://github.com/settings/ssh/new"]);
    // The user pastes it on GitHub, then tests.
    backend.state.accountKeys.push("github.com:~/.ssh/kunde-b_ed25519");
    await user.click(within(github).getByRole("button", { name: "Erledigt – jetzt testen" }));
    expect(await within(github).findByText("Verbunden als @tristan")).toBeInTheDocument();
    expect(within(github).queryByText("Pubkey ist kopiert.")).not.toBeInTheDocument();
  });

  it("offers to install the key on a server when the test fails", async () => {
    const { user, backend } = await renderApp("/profile/kunde-b", (b) => {
      b.state.profiles.find((p) => p.id === "kunde-b")!.hosts.push({ provider: "generic", hostname: "10.0.0.9", alias: "build", user: "ci", port: null });
      b.state.knownHosts.push("10.0.0.9");
    });
    const server = await row("Server build");
    await user.click(within(server).getByRole("button", { name: "Testen" }));
    expect(await within(server).findByTestId("connection-status")).toHaveTextContent("build kennt diesen Key nicht");
    await user.click(within(server).getByRole("button", { name: "Key auf build eintragen" }));
    expect(await within(server).findByText("Anmeldung erfolgreich")).toBeInTheDocument();
    expect(backend.state.installedKeys).toEqual(["10.0.0.9:~/.ssh/kunde-b_ed25519"]);
  });

  it("does not offer to add the key when the server is unreachable", async () => {
    const { user } = await renderApp("/profile/acme", (b) => {
      b.state.unreachable.push("gitlab.kunde.de");
    });
    const gitlab = await row("GitLab-Konto");
    await user.click(within(gitlab).getByRole("button", { name: "Testen" }));
    expect(await within(gitlab).findByTestId("connection-status")).toBeInTheDocument();
    expect(within(gitlab).queryByRole("button", { name: /Pubkey kopieren/ })).not.toBeInTheDocument();
  });

  it("shows the generated config", async () => {
    const { user } = await renderApp("/profile/privat");
    await user.click(await screen.findByRole("tab", { name: "Generierte Config" }));
    expect(await screen.findByText("~/.ssh/config.d/keybund.conf")).toBeInTheDocument();
    expect(screen.getAllByText("Auszug")).toHaveLength(2);
    expect(screen.getByText("verwaltet von Keybund")).toBeInTheDocument();
    expect(screen.getByText("Sagt ssh, welcher Key für welchen Server oder Alias gilt.")).toBeInTheDocument();
  });

  it("copies the pubkey and opens the page of a self-hosted GitLab", async () => {
    const { user, backend } = await renderApp("/profile/acme");
    const card = await row(/Konto/);
    await user.click(within(card).getByRole("button", { name: "Key eintragen" }));
    const hint = await within(card).findByRole("status");
    expect(hint).toHaveTextContent("Auf der GitLab-Seite „New SSH key“ wählen, einfügen und speichern – angemeldet mit dem Konto für „Acme“.");
    expect(hint).not.toHaveTextContent("Signing Key");
    expect(backend.state.clipboard).toMatch(/tristan@zentrale-acme$/);
    await user.click(within(hint).getByRole("button", { name: "Seite nochmal öffnen" }));
    expect(backend.state.openedUrls).toEqual([
      "https://gitlab.kunde.de/-/user_settings/ssh_keys",
      "https://gitlab.kunde.de/-/user_settings/ssh_keys",
    ]);
  });

  it("reminds of the signing key on GitHub and opens Bitbucket's page", async () => {
    const { user, backend } = await renderApp("/profile/privat", (b) => {
      b.state.profiles.find((p) => p.id === "kunde-b")!.hosts[0] = { provider: "bitbucket", hostname: "bitbucket.org", alias: null, user: null, port: null };
    });
    const card = await row(/GitHub-Konto/);
    await user.click(within(card).getByRole("button", { name: "Key eintragen" }));
    expect(await within(card).findByText(/„Key type: Signing Key“/)).toBeInTheDocument();

    await user.click(screen.getByRole("link", { name: /Kunde B/ }));
    await screen.findByRole("heading", { level: 1, name: "Kunde B" });
    const other = await row(/Bitbucket-Konto/);
    await user.click(within(other).getByRole("button", { name: "Key eintragen" }));
    await within(other).findByText("Pubkey ist kopiert.");
    expect(backend.state.openedUrls.at(-1)).toBe("https://bitbucket.org/account/settings/ssh-keys/");
  });

  it("explains when the browser cannot be opened", async () => {
    const { user, backend } = await renderApp("/profile/acme", (b) => {
      b.state.openFails = true;
    });
    const card = await row(/Konto/);
    await user.click(within(card).getByRole("button", { name: "Key eintragen" }));
    expect(await screen.findByText("Browser konnte nicht geöffnet werden")).toBeInTheDocument();
    expect(screen.getByText(/xdg-open.*https:\/\/gitlab\.kunde\.de\/-\/user_settings\/ssh_keys/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Link kopieren" })).toBeInTheDocument();
    expect(backend.state.openedUrls).toEqual([]);
  });

  it("keeps both of two quick removals", async () => {
    const { user, backend } = await renderApp("/profile/kunde-b", (b) => {
      b.state.latency = 50;
      b.state.profiles.find((p) => p.id === "kunde-b")!.folders = ["~/dev/kunde-b/", "~/work/b/"];
    });
    const first = await screen.findByRole("button", { name: "Ordner ~/dev/kunde-b/ entfernen" });
    const second = screen.getByRole("button", { name: "Ordner ~/work/b/ entfernen" });
    await user.click(first);
    await user.click(second);
    await waitFor(() => expect(backend.state.profiles.find((p) => p.id === "kunde-b")!.folders).toEqual([]));
  });

  it("does not claim the pubkey was copied when the key file is missing", async () => {
    const { user, backend } = await renderApp("/profile/kunde-b", (b) => {
      b.state.keys.delete("~/.ssh/kunde-b_ed25519");
    });
    const github = await row("GitHub-Konto");
    await user.click(within(github).getByRole("button", { name: "Key eintragen" }));
    expect(await screen.findByText(/Key fehlt/)).toBeInTheDocument();
    expect(within(github).queryByText("Pubkey ist kopiert.")).not.toBeInTheDocument();
    expect(backend.state.openedUrls).toEqual([]);
  });

  it("shows that an archived key has no generated config", async () => {
    const { user } = await renderApp("/profile/altprojekt");
    await user.click(await screen.findByRole("tab", { name: "Generierte Config" }));
    expect(await screen.findByText(/Keybund schreibt keine Regeln mehr für ihn/)).toBeInTheDocument();
  });

  it("shows a not-found page", async () => {
    await renderApp("/profile/nope");
    expect(await screen.findByRole("heading", { name: "Key nicht gefunden" })).toBeInTheDocument();
  });
});
