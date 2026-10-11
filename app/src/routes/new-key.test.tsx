import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "@/test/render";

describe("new key", () => {
  it("creates a key with a name and nothing else, then asks what it is for", async () => {
    const { user, backend, router } = await renderApp("/new");
    await user.click(screen.getByRole("button", { name: /Key anlegen/ }));
    expect(screen.getByRole("alert")).toHaveTextContent("Bitte einen Namen eingeben.");

    await user.type(screen.getByLabelText("Name"), "Kunde C");
    expect(await screen.findByText("ed25519 · ~/.ssh/kunde-c_ed25519")).toBeInTheDocument();
    await user.type(screen.getByLabelText(/Passphrase/), "geheim");
    await user.click(screen.getByText("Mehr Optionen"));
    expect(screen.getByLabelText("Kommentar")).toHaveValue("tristan@zentrale-kunde-c");
    await user.click(screen.getByRole("button", { name: /Key anlegen/ }));

    expect(await screen.findByRole("heading", { level: 1, name: "Kunde C" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/profile/kunde-c");
    const created = backend.state.profiles.find((p) => p.id === "kunde-c")!;
    expect(created).toMatchObject({
      name: "Kunde C",
      folders: [],
      hosts: [],
      git: null,
      key: { path: "~/.ssh/kunde-c_ed25519" },
    });
    expect(created.color).not.toBe("emerald");
    // An unused key starts with the choices.
    const usage = screen.getByRole("region", { name: "Wo wird dieser Key benutzt?" });
    expect(within(usage).getByRole("button", { name: /^Git-Ordner/ })).toBeInTheDocument();
    expect(within(usage).getByRole("button", { name: /^Server/ })).toBeInTheDocument();
    expect(
      within(screen.getByRole("navigation", { name: "Keys" })).getByText("noch nicht benutzt"),
    ).toBeInTheDocument();
  });

  it("refuses a name that is taken", async () => {
    const { user } = await renderApp("/new");
    await user.type(screen.getByLabelText("Name"), "Kunde B");
    await screen.findByText("ed25519 · ~/.ssh/kunde-b_ed25519");
    await user.click(screen.getByRole("button", { name: /Key anlegen/ }));
    expect(await screen.findByText("Einen Key „Kunde B“ gibt es schon.")).toBeInTheDocument();
  });

  it("takes an existing key from ~/.ssh", async () => {
    const { user, backend } = await renderApp("/new?key=~/.ssh/id_ed25519");
    expect(screen.getByRole("radio", { name: /Vorhandenen Key nehmen/ })).toHaveAttribute("aria-checked", "true");
    expect(await screen.findByRole("radio", { name: /^~\/\.ssh\/id_ed25519/ })).toBeChecked();
    await user.type(screen.getByLabelText("Name"), "Laptop");
    await user.click(screen.getByRole("button", { name: /Key anlegen/ }));
    expect(await screen.findByRole("heading", { level: 1, name: "Laptop" })).toBeInTheDocument();
    expect(backend.state.profiles.find((p) => p.id === "laptop")!.key.path).toBe("~/.ssh/id_ed25519");
  });

  it("asks to pick a key when taking an existing one", async () => {
    const { user } = await renderApp("/new");
    await user.type(screen.getByLabelText("Name"), "Alt");
    await user.click(screen.getByRole("radio", { name: /Vorhandenen Key nehmen/ }));
    await user.click(screen.getByRole("button", { name: /Key anlegen/ }));
    expect(screen.getByRole("alert")).toHaveTextContent("Bitte einen vorhandenen Key wählen.");
  });
});
