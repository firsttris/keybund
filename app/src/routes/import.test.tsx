import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "@/test/render";

describe("import from ~/.ssh/config", () => {
  it("is offered on the home page", async () => {
    const { user, router } = await renderApp("/");
    expect(await screen.findByText(/2 Einträge mit eigenem Key/)).toBeInTheDocument();
    await user.click(screen.getByRole("link", { name: "Übernehmen" }));
    expect(router.state.location.pathname).toBe("/import");
  });

  it("imports, writes the configs and removes the old entries", async () => {
    const { user, backend } = await renderApp("/import");
    const pflege = await screen.findByRole("region", { name: "Pflegeabc Account" });
    expect(within(pflege).getByText("~/.ssh/pflegeabc/id_ed25519")).toBeInTheDocument();
    expect(within(pflege).getByText("pflegeabc")).toBeInTheDocument();

    // Rename one, give the other a folder, then import both.
    await user.clear(within(pflege).getByLabelText("Name"));
    await user.type(within(pflege).getByLabelText("Name"), "Pflege ABC");
    const scomm = screen.getByRole("region", { name: "S-Communication" });
    await user.type(within(scomm).getByLabelText("Projektordner (optional)"), "~/dev/sparkasse-neu");
    await user.click(screen.getByRole("button", { name: "2 Keys übernehmen" }));

    expect(await screen.findByRole("heading", { name: "2. Keybund-Configs schreiben" })).toBeInTheDocument();
    expect(backend.state.profiles.find((p) => p.id === "pflegeabc")).toMatchObject({
      name: "Pflege ABC",
      hosts: [{ alias: "pflegeabc", hostname: "github.com" }],
    });
    expect(backend.state.profiles.find((p) => p.id === "s-communication")!.folders).toEqual(["~/dev/sparkasse-neu/"]);

    // Cleanup waits until the configs are written.
    const cleanup = screen.getByRole("button", { name: "Alte Einträge entfernen" });
    expect(cleanup).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Configs schreiben" }));
    expect(await screen.findByRole("button", { name: "Alte Einträge entfernen" })).toBeEnabled();
    expect(screen.getByText("-Host pflegeabc")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Alte Einträge entfernen" }));

    expect(await screen.findByText(/Alles übernommen/)).toBeInTheDocument();
    expect(backend.state.sshBlocks).toEqual([]);
  });

  it("can skip a proposal", async () => {
    const { user, backend } = await renderApp("/import");
    await user.click(await screen.findByRole("checkbox", { name: "Pflegeabc Account übernehmen" }));
    await user.click(screen.getByRole("button", { name: "1 Key übernehmen" }));
    await screen.findByRole("heading", { name: "2. Keybund-Configs schreiben" });
    expect(backend.state.profiles.some((p) => p.id === "pflegeabc")).toBe(false);
    expect(backend.state.profiles.some((p) => p.id === "s-communication")).toBe(true);
    expect(screen.getByRole("region", { name: "Pflegeabc Account" })).toBeInTheDocument();
  });

  it("shows when nothing is left", async () => {
    await renderApp("/import", (b) => {
      b.state.sshBlocks = [];
    });
    expect(await screen.findByText(/Alles übernommen/)).toBeInTheDocument();
  });
});

describe("connection test with another key", () => {
  const testAccount = async (user: Awaited<ReturnType<typeof renderApp>>["user"]) => {
    const card = await screen.findByRole("region", { name: "Wo wird dieser Key benutzt?" });
    const account = within(card)
      .getAllByRole("listitem")
      .find((li) => li.textContent?.includes("GitLab-Konto"))!;
    await user.click(within(account).getByRole("button", { name: "Testen" }));
    return account;
  };

  it("explains that the key was not used", async () => {
    const { user } = await renderApp("/profile/sparkasse", (b) => {
      b.state.otherKeyFor = "sparkasse";
    });
    const account = await testAccount(user);
    expect(await within(account).findByTestId("connection-status")).toHaveTextContent(
      "Angemeldet als @firsttris, aber mit ~/.ssh/privat_ed25519 statt mit diesem Key",
    );
    const alert = within(account).getByRole("alert");
    expect(alert).toHaveTextContent("Dieser Key wurde nicht benutzt");
    expect(within(alert).getByRole("link", { name: "Bestehende Config übernehmen" })).toBeInTheDocument();
  });

  it("names the key that was used on success", async () => {
    const { user } = await renderApp("/profile/sparkasse");
    const account = await testAccount(user);
    expect(await within(account).findByTestId("connection-status")).toHaveTextContent("Verbunden als @tristan");
    expect(within(account).getByRole("list", { name: "Ablauf" })).toHaveTextContent("Server akzeptiert ~/.ssh/sparkasse_ed25519");
  });
});
