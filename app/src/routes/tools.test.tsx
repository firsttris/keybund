import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "@/test/render";

describe("which", () => {
  it("resolves examples and typed queries", async () => {
    const { user } = await renderApp("/which");
    await user.click(await screen.findByRole("button", { name: "meinserver" }));
    const result = await screen.findByTestId("which-result");
    expect(result).toHaveTextContent("privat_ed25519");
    expect(result).toHaveTextContent("tristan@1.2.3.4:22");
    expect(within(result).getByRole("link", { name: "Key Privat" })).toBeInTheDocument();

    await user.clear(screen.getByLabelText("Ordner oder Host"));
    await user.type(screen.getByLabelText("Ordner oder Host"), "~/tmp/x");
    await user.click(screen.getByRole("button", { name: "Auflösen" }));
    expect(await screen.findByText("Ordner gehört zu keinem Key, es gilt der Standard-Key.")).toBeInTheDocument();
  });

  it("answers a query from the URL right away", async () => {
    await renderApp("/which?q=github.com");
    expect(screen.getByLabelText("Ordner oder Host")).toHaveValue("github.com");
    expect(await screen.findByText(/Für Git-Repos entscheidet der Ordner/)).toBeInTheDocument();
  });
});

describe("clone", () => {
  it("clones into the selected profile", async () => {
    const { user, backend } = await renderApp("/clone");
    await user.type(await screen.findByLabelText("Repo-URL"), "git@github.com:team/app.git");
    await user.click(screen.getByRole("button", { name: "Prüfen" }));
    await user.click(await screen.findByRole("radio", { name: /Kunde B/ }));
    expect(screen.getByLabelText("Zielordner")).toHaveValue("~/dev/kunde-b/app");
    await user.click(screen.getByRole("button", { name: "Klonen" }));
    expect(await screen.findByRole("status")).toHaveTextContent("Geklont nach ~/dev/kunde-b/app");
    expect(backend.state.cloned).toEqual(["~/dev/kunde-b/app"]);
  });

  it("asks for a profile when no host matches", async () => {
    const { user } = await renderApp("/clone");
    await user.type(await screen.findByLabelText("Repo-URL"), "git@codeberg.org:x/y.git");
    await user.click(screen.getByRole("button", { name: "Prüfen" }));
    expect(await screen.findByText("Kein Key ist für codeberg.org eingerichtet – bitte einen wählen.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Klonen" })).toBeDisabled();
  });

  it("needs a new check after the URL changed", async () => {
    const { user } = await renderApp("/clone");
    await user.type(await screen.findByLabelText("Repo-URL"), "git@github.com:team/a.git");
    await user.click(screen.getByRole("button", { name: "Prüfen" }));
    expect(await screen.findByRole("button", { name: "Klonen" })).toBeInTheDocument();
    await user.type(screen.getByLabelText("Repo-URL"), "x");
    expect(screen.queryByRole("button", { name: "Klonen" })).not.toBeInTheDocument();
  });

  it("shows invalid URLs", async () => {
    const { user } = await renderApp("/clone");
    await user.type(await screen.findByLabelText("Repo-URL"), "kaputt");
    await user.click(screen.getByRole("button", { name: "Prüfen" }));
    expect(await screen.findByText("Ungültige Repo-URL: kaputt")).toBeInTheDocument();
  });
});

describe("settings", () => {
  it("changes the default profile and lists keys", async () => {
    const { user, backend } = await renderApp("/settings");
    const select = await screen.findByLabelText("Key für Repos außerhalb deiner Git-Ordner");
    await screen.findByRole("option", { name: "Sparkasse" });
    await user.selectOptions(select, "sparkasse");
    expect(backend.state.defaultProfile).toBe("sparkasse");
    await user.selectOptions(select, "");
    expect(backend.state.defaultProfile).toBeNull();

    expect(await screen.findByText("~/.ssh/id_ed25519")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "genutzt von Privat" })).toBeInTheDocument();
    const row = screen.getByText("~/.ssh/id_ed25519").closest("li")!;
    expect(within(row).getByRole("link", { name: "Als Key übernehmen" })).toHaveAttribute("href", "/new?key=%7E%2F.ssh%2Fid_ed25519");
    expect(screen.getByText("~/.config/keybund/profiles.toml")).toBeInTheDocument();
    expect(await screen.findByText(/Keybund 0\.1\.0 by Tristan Teufel/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Quellcode auf GitHub" }));
    expect(backend.state.openedUrls).toEqual(["https://github.com/firsttris/keybund"]);
  });

  it("changes the folders for new keys", async () => {
    const { user, backend, router } = await renderApp("/settings");
    const dir = await screen.findByLabelText("Ordner für Git-Repos");
    const save = screen.getByRole("button", { name: "Speichern" });
    expect(save).toBeDisabled();
    await user.clear(dir);
    await user.type(dir, "work");
    await user.click(save);
    expect(await screen.findByRole("alert")).toHaveTextContent("Ungültiger Ordner „work“");
    await user.clear(dir);
    await user.type(dir, "~/work/");
    await user.clear(screen.getByLabelText("Ordner für Keys"));
    await user.type(screen.getByLabelText("Ordner für Keys"), "~/.ssh/keybund");
    await user.click(save);
    await waitFor(() => expect(backend.state.settings.projects_dir).toBe("~/work"));
    expect(backend.state.settings).toMatchObject({ projects_dir: "~/work", keys_dir: "~/.ssh/keybund" });
    expect(await screen.findByLabelText("Ordner für Git-Repos")).toHaveValue("~/work");

    // A new key goes into the key folder, its git folder into the project folder.
    await router.navigate({ to: "/new" });
    await user.type(await screen.findByLabelText("Name"), "Kunde W");
    expect(await screen.findByText("ed25519 · ~/.ssh/keybund/kunde-w_ed25519")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Key anlegen/ }));
    expect(await screen.findByRole("heading", { level: 1, name: "Kunde W" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^Git-Ordner/ }));
    expect(screen.getByLabelText("Ordner")).toHaveAttribute("placeholder", "~/work/kunde-w/");
  });

  it("keeps special switches under Erweitert and saves them right away", async () => {
    const { user, backend } = await renderApp("/settings");
    const advanced = (await screen.findByText("Erweitert")).closest("details")!;
    expect(advanced).not.toHaveAttribute("open");
    await user.click(screen.getByText("Erweitert"));
    await user.click(screen.getByRole("switch", { name: "Server-Fingerprints pro Key" }));
    await waitFor(() => expect(backend.state.settings.known_hosts_per_profile).toBe(false));
    await user.click(screen.getByRole("switch", { name: "Include mit vollem Pfad" }));
    await waitFor(() => expect(backend.state.settings.absolute_ssh_include).toBe(true));
    // Written right away, nothing left to apply.
    expect(screen.queryByText(/nicht zu deinen Keys/)).not.toBeInTheDocument();
    expect(within(advanced).getByRole("link", { name: "Entfernen …" })).toBeInTheDocument();
  });
});
