import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "@/test/render";

describe("profile page", () => {
  it("renders the profile from the backend", async () => {
    await renderApp("/profile/sparkasse");
    expect(await screen.findByRole("heading", { level: 1, name: "Sparkasse" })).toBeInTheDocument();
    expect(screen.getByText("~/.ssh/sparkasse_ed25519")).toBeInTheDocument();
    expect(screen.getByTestId("public-key")).toHaveTextContent(/^ssh-ed25519 .* tristan@zentrale-sparkasse$/);
    expect(screen.getByText("Configs aktuell")).toBeInTheDocument();
    expect(screen.getByText(/zuletzt angewendet (heute|gestern), \d\d:\d\d/)).toBeInTheDocument();
    expect(screen.getByText("Im Agent geladen")).toBeInTheDocument();
  });

  it("shows when a key is not in the agent", async () => {
    await renderApp("/profile/kunde-b");
    expect(await screen.findByText("Nicht im Agent")).toBeInTheDocument();
  });

  it("copies the public key and shows feedback", async () => {
    const { user, backend } = await renderApp("/profile/sparkasse");
    const [headerButton] = await screen.findAllByRole("button", { name: "Pubkey kopieren" });
    await user.click(headerButton);
    expect(await screen.findByRole("button", { name: "Kopiert" })).toBeInTheDocument();
    expect(backend.state.clipboard).toMatch(/^ssh-ed25519 /);
  });

  it("toggling signing marks configs as outdated", async () => {
    const { user } = await renderApp("/profile/sparkasse");
    await user.click(await screen.findByRole("switch", { name: "Commits signieren" }));
    expect(await screen.findByText("1 Datei ist nicht aktuell")).toBeInTheDocument();
  });

  it("validates the e-mail before saving the identity", async () => {
    const { user } = await renderApp("/profile/sparkasse");
    const mail = await screen.findByLabelText("E-Mail");
    await user.clear(mail);
    await user.type(mail, "kaputt");
    expect(screen.getByRole("alert")).toHaveTextContent("gültige E-Mail");
    expect(screen.getByRole("button", { name: "Identität speichern" })).toBeDisabled();
  });

  it("shows a failed connection test", async () => {
    const { user } = await renderApp("/profile/kunde-b");
    await user.click(await screen.findByRole("button", { name: "Verbindung testen" }));
    expect(await screen.findByTestId("connection-status")).toHaveTextContent("GitHub kennt diesen Key nicht");
  });

  it("hides actions for archived profiles", async () => {
    await renderApp("/profile/bundesagentur");
    expect(await screen.findByText("Archiviert")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Verbindung testen" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Archivieren" })).not.toBeInTheDocument();
  });
});

describe("sidebar and home", () => {
  it("lists active and archived profiles and the doctor badge", async () => {
    await renderApp("/");
    const profiles = await screen.findByRole("navigation", { name: "Profile" });
    await waitFor(() => expect(within(profiles).getAllByRole("link")).toHaveLength(4));
    expect(within(screen.getByRole("group", { name: "Standard" })).getByRole("link")).toHaveTextContent("Privat");
    const projects = within(screen.getByRole("group", { name: "Projekte & Konten" })).getAllByRole("link");
    expect(projects.map((l) => l.textContent)).toEqual(["SparkasseGitLab", "Kunde BGitHub"]);
    expect(within(screen.getByRole("group", { name: "Server" })).getByRole("link")).toHaveTextContent("meinserverPrivat");
    expect(within(screen.getByRole("navigation", { name: "Archiv" })).getByText("Bundesagentur")).toBeInTheDocument();
    expect(await screen.findByLabelText("1 Hinweise")).toBeInTheDocument();
    expect(await screen.findByRole("alert")).toHaveTextContent("Include steht nicht ganz oben");
  });

  it("groups the home page into standard, projects and servers", async () => {
    const { user } = await renderApp("/", (b) => {
      b.state.profiles.push({
        id: "hetzner",
        name: "Hetzner",
        color: "rose",
        key: { path: "~/.ssh/hetzner_ed25519", comment: null },
        git: null,
        folders: [],
        archived: false,
        hosts: [{ provider: "generic", hostname: "5.6.7.8", alias: "hetzner", user: "root", port: 2222 }],
      });
    });
    const main = await screen.findByRole("main");
    const standard = await within(main).findByRole("region", { name: "Standard" });
    expect(within(standard).getByText("Privat")).toBeInTheDocument();
    const projects = within(main).getByRole("region", { name: "Projekte & Konten" });
    expect(within(projects).getAllByRole("link").map((l) => l.textContent)).toEqual([
      expect.stringContaining("Sparkasse"),
      expect.stringContaining("Kunde B"),
    ]);
    const servers = within(main).getByRole("region", { name: "Server" });
    expect(within(servers).getAllByRole("link")).toHaveLength(2);
    expect(within(servers).getByText("root@5.6.7.8:2222")).toBeInTheDocument();
    expect(within(servers).getByText("Key von Hetzner")).toBeInTheDocument();
    // Server-only profiles are not listed as projects, also not in the sidebar.
    expect(within(screen.getByRole("group", { name: "Projekte & Konten" })).queryByText("Hetzner")).not.toBeInTheDocument();
    await user.type(screen.getByLabelText("Profile durchsuchen"), "5.6.7");
    expect(within(screen.getByRole("navigation", { name: "Profile" })).getAllByRole("link")).toHaveLength(1);
  });

  it("shows the empty state without profiles", async () => {
    await renderApp("/", (b) => {
      b.state.profiles = [];
      b.state.defaultProfile = null;
    });
    expect(await screen.findByText("Noch keine Profile", { selector: "span.text-\\[15px\\]" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Vorhandene Keys importieren" })).toBeInTheDocument();
  });
});

describe("apply page", () => {
  it("shows the diff and applies it", async () => {
    const { user } = await renderApp("/apply", (b) => {
      b.state.defaultProfile = "sparkasse";
    });
    expect(await screen.findByText(/^\+\s+sshCommand = ssh -i ~\/\.ssh\/sparkasse_ed25519 -o IdentitiesOnly=yes -o AddKeysToAgent=yes$/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Anwenden" }));
    expect(await screen.findByText("Alle Configs sind aktuell.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Anwenden" })).toBeDisabled();
  });

  it("asks to reload when the plan changed meanwhile", async () => {
    const { user, backend } = await renderApp("/apply", (b) => {
      b.state.defaultProfile = "sparkasse";
    });
    await screen.findByText(/sparkasse_ed25519/);
    backend.state.defaultProfile = "kunde-b";
    await user.click(screen.getByRole("button", { name: "Anwenden" }));
    expect(await screen.findByText(/Bitte Vorschau neu laden/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Vorschau neu laden" }));
    expect(await screen.findByText(/kunde-b_ed25519 -o/)).toBeInTheDocument();
  });
});

describe("doctor", () => {
  it("fixes a problem", async () => {
    const { user } = await renderApp("/doctor");
    await user.click(await screen.findByRole("button", { name: "Include steht nicht ganz oben beheben" }));
    expect(await screen.findByText("Alles in Ordnung")).toBeInTheDocument();
  });

  it("offers the full Include path for Dolphin with an old libssh", async () => {
    const { user, backend } = await renderApp("/doctor", (b) => {
      b.state.doctorIncludeOnTop = true;
      b.state.dolphinLibssh = "0.9.6";
    });
    expect(await screen.findByText("Dolphin sieht die Keybund-Config nicht (libssh 0.9.6)")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Dolphin sieht die Keybund-Config nicht.*beheben/ }));
    expect(await screen.findByText("Dolphin (libssh 0.9.6) liest die Keybund-Config")).toBeInTheDocument();
    expect(backend.state.settings.absolute_ssh_include).toBe(true);
  });
});
