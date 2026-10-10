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
    await waitFor(() => expect(within(profiles).getAllByRole("link")).toHaveLength(3));
    expect(within(screen.getByRole("navigation", { name: "Archiv" })).getByText("Bundesagentur")).toBeInTheDocument();
    expect(await screen.findByLabelText("1 Hinweise")).toBeInTheDocument();
    expect(await screen.findByRole("alert")).toHaveTextContent("Include steht nicht ganz oben");
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
});
