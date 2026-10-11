import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { FileChange } from "@/lib/bindings";
import { DiffSummary, DiffView, diffLines } from "./diff-view";

const change: FileChange = {
  path: "~/.ssh/config",
  kind: "modify",
  diff: "--- ~/.ssh/config\n+++ ~/.ssh/config\n@@ -1,1 +1,3 @@\n+Include ~/.ssh/config.d/keybund.conf\n+\n Host a\n-    User x\n",
  added: 2,
  removed: 1,
  user_file: true,
};

describe("diffLines", () => {
  it("classifies lines and drops headers", () => {
    expect(diffLines(change.diff).map((l) => l.kind)).toEqual(["hunk", "add", "add", "ctx", "del"]);
  });
});

describe("DiffView", () => {
  it("renders files, kinds and totals", () => {
    render(
      <DiffView
        changes={[change, { ...change, path: "~/.gitconfig", kind: "create", user_file: false, removed: 0 }]}
      />,
    );
    expect(screen.getAllByTestId("diff-file")).toHaveLength(2);
    expect(screen.getByText("· geändert")).toBeInTheDocument();
    expect(screen.getByText("· neu")).toBeInTheDocument();
    expect(screen.getByText("· deine Datei, einmaliges Backup")).toBeInTheDocument();
    expect(screen.getAllByText("+Include ~/.ssh/config.d/keybund.conf")[0]).toHaveClass("bg-emerald-500/10");
  });

  it("summarizes a single file", () => {
    render(<DiffSummary changes={[change]} />);
    expect(screen.getByText("+2")).toBeInTheDocument();
    expect(screen.getByText("−1")).toBeInTheDocument();
    expect(screen.getByText(/1 Datei$/)).toBeInTheDocument();
  });
});
