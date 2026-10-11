import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { msg } from "@/lib/mock/backend";
import { renderApp } from "@/test/render";

describe("profile page", () => {
  it("renders the profile from the backend", async () => {
    await renderApp("/profile/acme");
    expect(await screen.findByRole("heading", { level: 1, name: "Acme" })).toBeInTheDocument();
    expect(screen.getByText("~/.ssh/acme_ed25519", { selector: "dd" })).toBeInTheDocument();
    expect(screen.getByTestId("public-key")).toHaveTextContent(/^ssh-ed25519 .* tristan@zentrale-acme$/);
    expect(screen.getByText("Configs aktuell")).toBeInTheDocument();
    expect(screen.getByText(/zuletzt geschrieben (heute|gestern), \d\d:\d\d/)).toBeInTheDocument();
    expect(screen.getByText("Im Agent geladen")).toBeInTheDocument();
  });

  it("shows when a key is not in the agent", async () => {
    await renderApp("/profile/kunde-b");
    expect(await screen.findByText("Nicht im Agent")).toBeInTheDocument();
  });

  it("copies the public key and shows feedback", async () => {
    const { user, backend } = await renderApp("/profile/acme");
    const [headerButton] = await screen.findAllByRole("button", { name: "Pubkey kopieren" });
    await user.click(headerButton);
    expect(await screen.findByRole("button", { name: "Kopiert" })).toBeInTheDocument();
    expect(backend.state.clipboard).toMatch(/^ssh-ed25519 /);
  });

  it("toggling signing is written right away", async () => {
    const { user, backend } = await renderApp("/profile/acme");
    await user.click(await screen.findByRole("switch", { name: "Commits signieren" }));
    expect(await screen.findByText("Gespeichert")).toBeInTheDocument();
    expect(backend.state.applied.get("~/.config/keybund/git/acme.gitconfig")).toContain("gpgsign = true");
    expect(screen.getByText("Configs aktuell")).toBeInTheDocument();
  });

  it("keeps the key details folded", async () => {
    const { user } = await renderApp("/profile/acme");
    const details = (await screen.findByText("Pubkey, Fingerprint und Datei anzeigen")).closest("details")!;
    expect(details).not.toHaveAttribute("open");
    await user.click(screen.getByText("Pubkey, Fingerprint und Datei anzeigen"));
    expect(details).toHaveAttribute("open");
  });

  it("validates the e-mail before saving the identity", async () => {
    const { user } = await renderApp("/profile/acme");
    const mail = await screen.findByLabelText("E-Mail");
    await user.clear(mail);
    await user.type(mail, "kaputt");
    expect(screen.getByRole("alert")).toHaveTextContent("gültige E-Mail");
    expect(screen.getByRole("button", { name: "Identität speichern" })).toBeDisabled();
  });

  it("hides actions for archived profiles", async () => {
    await renderApp("/profile/altprojekt");
    expect(await screen.findByText("Archiviert")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Testen" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Archivieren" })).not.toBeInTheDocument();
  });
});

describe("sidebar and home", () => {
  it("lists the keys with what they are used for, the archive and the doctor badge", async () => {
    await renderApp("/");
    const keys = await screen.findByRole("navigation", { name: "Keys" });
    await waitFor(() => expect(within(keys).getAllByRole("link")).toHaveLength(3));
    const [first, ...rest] = within(keys).getAllByRole("link");
    // The standard key comes first.
    expect(first).toHaveTextContent("Privat");
    expect(first).toHaveTextContent("Standard · ~/dev/privat/ · GitHub · meinserver");
    expect(rest.map((l) => l.textContent)).toEqual(["Acme~/dev/acme/ · GitLab", "Kunde B~/dev/kunde-b/ · GitHub"]);
    expect(within(screen.getByRole("navigation", { name: "Archiv" })).getByText("Altprojekt")).toBeInTheDocument();
    expect(await screen.findByRole("link", { name: /Doctor.*1 Hinweis/ })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Welcher Key?" })).not.toBeInTheDocument();
  });

  it("shows what needs attention on the home page", async () => {
    await renderApp("/", (b) => {
      b.state.lastTests.set("kunde-b", {
        success: false,
        account: null,
        message: msg("probe_keyUnknown", { provider: "GitHub" }),
        at: Date.now(),
      });
    });
    const todo = await screen.findByRole("list", { name: "Zu tun" });
    await waitFor(() => expect(within(todo).getAllByRole("listitem")).toHaveLength(3));
    const [test, doctor, imported] = within(todo).getAllByRole("listitem");
    expect(test).toHaveTextContent("Kunde B: GitHub kennt diesen Key nicht");
    expect(within(test).getByRole("link", { name: "Beheben" }).getAttribute("href")).toMatch(/\/profile\/kunde-b$/);
    expect(doctor).toHaveTextContent("Include steht nicht ganz oben");
    expect(imported).toHaveTextContent("2 Einträge mit eigenem Key");
  });

  it("says when everything is fine", async () => {
    await renderApp("/", (b) => {
      b.state.doctorIncludeOnTop = true;
      b.state.sshBlocks = [];
    });
    expect(
      await screen.findByText(/Alles in Ordnung – Git und ssh nehmen automatisch den richtigen Key/),
    ).toBeInTheDocument();
  });

  it("answers which key applies from the search", async () => {
    const { user, router } = await renderApp("/");
    await user.type(await screen.findByLabelText("Keys durchsuchen"), "~/dev/acme/app");
    expect(screen.getByText("Kein Key passt")).toBeInTheDocument();
    await user.click(screen.getByRole("link", { name: "Welcher Key gilt für „~/dev/acme/app“?" }));
    expect(router.state.location.pathname).toBe("/which");
    expect(await screen.findByTestId("which-result")).toHaveTextContent("acme_ed25519");
    // Enter does the same.
    await user.clear(screen.getByLabelText("Keys durchsuchen"));
    await user.type(screen.getByLabelText("Keys durchsuchen"), "meinserver{Enter}");
    expect(await screen.findByTestId("which-result")).toHaveTextContent("tristan@1.2.3.4:22");
  });

  it("shows a card per key and finds keys by folder or server", async () => {
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
      b.state.profiles.push({
        id: "leer",
        name: "Leer",
        color: "cyan",
        key: { path: "~/.ssh/id_ed25519", comment: null },
        git: null,
        folders: [],
        archived: false,
        hosts: [],
      });
    });
    const main = await screen.findByRole("main");
    const cards = within(main).getByRole("list", { name: "Keys" });
    expect(within(cards).getByRole("list", { name: "Privat wird benutzt für" })).toHaveTextContent("Standard für Git");
    expect(within(cards).getByRole("list", { name: "Hetzner wird benutzt für" })).toHaveTextContent("ssh hetzner");
    expect(within(cards).getByText(/Noch nirgends benutzt/)).toBeInTheDocument();
    const sidebar = screen.getByRole("navigation", { name: "Keys" });
    expect(within(sidebar).getByText("noch nicht benutzt")).toBeInTheDocument();
    await user.type(screen.getByLabelText("Keys durchsuchen"), "5.6.7");
    expect(within(sidebar).getAllByRole("link")).toHaveLength(2);
    expect(within(sidebar).getAllByRole("link")[0]).toHaveTextContent("Hetzner");
    await user.clear(screen.getByLabelText("Keys durchsuchen"));
    await user.type(screen.getByLabelText("Keys durchsuchen"), "kunde-b");
    expect(within(sidebar).getAllByRole("link")[0]).toHaveTextContent("Kunde B");
  });

  it("explains how Keybund works and can hide it", async () => {
    localStorage.clear();
    const { user } = await renderApp("/");
    const box = await screen.findByRole("region", { name: "So funktioniert Keybund" });
    expect(box).toHaveTextContent("Ein Key pro Zweck");
    expect(box).toHaveTextContent("Läuft ohne Keybund");
    await user.click(within(box).getByRole("button", { name: "Erklärung ausblenden" }));
    expect(screen.queryByRole("region", { name: "So funktioniert Keybund" })).not.toBeInTheDocument();
    expect(localStorage.getItem("keybund.howItWorks.hidden")).toBe("1");
    await user.click(screen.getByRole("button", { name: "Wie funktioniert Keybund?" }));
    expect(screen.getByRole("region", { name: "So funktioniert Keybund" })).toBeInTheDocument();
  });

  it("shows a load error once instead of refetching forever", async () => {
    let backend: Awaited<ReturnType<typeof renderApp>>["backend"] | undefined;
    await renderApp("/", (b) => {
      b.state.configError = "Zeile 3: unerwartetes Zeichen";
      backend = b;
      vi.spyOn(b, "invoke");
    });
    const alerts = await screen.findAllByRole("alert");
    expect(alerts.map((a) => a.textContent)).toEqual([
      "profiles.toml ist fehlerhaft: Zeile 3: unerwartetes Zeichen",
      "profiles.toml ist fehlerhaft: Zeile 3: unerwartetes Zeichen",
    ]);
    expect(within(screen.getByRole("main")).getByRole("alert")).toBeInTheDocument();
    expect(within(screen.getByRole("navigation", { name: "Keys" })).getByRole("alert")).toBeInTheDocument();
    expect(screen.queryByRole("status", { name: "Lädt …" })).not.toBeInTheDocument();
    // Before: every remount of the page's children refetched, hundreds of times a second.
    await new Promise((r) => setTimeout(r, 300));
    const loads = vi.mocked(backend!.invoke).mock.calls.filter(([cmd]) => cmd === "load_profiles");
    expect(loads).toHaveLength(1);
  });

  it("shows the empty state without profiles", async () => {
    await renderApp("/", (b) => {
      b.state.profiles = [];
      b.state.defaultProfile = null;
    });
    expect(await screen.findByText("Noch keine Keys", { selector: "span.text-\\[15px\\]" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Vorhandene Keys übernehmen" })).toBeInTheDocument();
    // Without keys the explanation is always there.
    expect(screen.getByRole("region", { name: "So funktioniert Keybund" })).toBeInTheDocument();
  });
});

describe("apply page", () => {
  it("restores files that were changed outside Keybund", async () => {
    const { user } = await renderApp("/", (b) => {
      b.state.applied.set("~/.config/keybund/git/acme.gitconfig", "kaputt\n");
    });
    expect(await screen.findByText(/1 Config-Datei passt nicht zu deinen Keys/)).toBeInTheDocument();
    await user.click(screen.getByRole("link", { name: "Ansehen & neu schreiben" }));
    expect(await screen.findByRole("heading", { name: "Configs neu schreiben" })).toBeInTheDocument();
    expect(await screen.findByText("-kaputt")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Neu schreiben" }));
    expect(await screen.findByText("Alle Configs passen zu deinen Keys.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Neu schreiben" })).toBeDisabled();
  });

  it("asks to reload when the plan changed meanwhile", async () => {
    const { user, backend } = await renderApp("/apply", (b) => {
      b.state.applied.set("~/.config/keybund/git/acme.gitconfig", "kaputt\n");
    });
    await screen.findByText("-kaputt");
    backend.state.applied.set("~/.config/keybund/git/kunde-b.gitconfig", "auch kaputt\n");
    await user.click(screen.getByRole("button", { name: "Neu schreiben" }));
    expect(await screen.findByText(/seit der Vorschau geändert/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Vorschau neu laden" }));
    expect(await screen.findByText("-auch kaputt")).toBeInTheDocument();
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
