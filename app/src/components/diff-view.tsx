import type { FileChange } from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { m } from "@/paraglide/messages";

const KIND_LABEL = { create: m.diff_create, modify: m.diff_modify, delete: m.diff_delete } as const;

/** Lines of a unified diff without the file headers. */
export function diffLines(diff: string): { kind: "add" | "del" | "ctx" | "hunk"; text: string }[] {
  return diff
    .split("\n")
    .filter((l) => l !== "" && !l.startsWith("--- ") && !l.startsWith("+++ "))
    .map((l) => {
      if (l.startsWith("@@")) return { kind: "hunk" as const, text: l };
      if (l.startsWith("+")) return { kind: "add" as const, text: l };
      if (l.startsWith("-")) return { kind: "del" as const, text: l };
      return { kind: "ctx" as const, text: l };
    });
}

export function DiffSummary({ changes }: { changes: FileChange[] }) {
  const added = changes.reduce((n, c) => n + c.added, 0);
  const removed = changes.reduce((n, c) => n + c.removed, 0);
  return (
    <span className="text-xs">
      <span className="text-success">+{added}</span>
      {removed > 0 && <span className="text-danger"> −{removed}</span>}
      <span className="text-muted-foreground"> {m.diff_summary({ n: changes.length })}</span>
    </span>
  );
}

export function DiffView({ changes, className }: { changes: FileChange[]; className?: string }) {
  return (
    <div className={cn("overflow-hidden rounded-[10px] border border-border", className)}>
      <div className="flex items-center justify-between gap-3 border-b border-border bg-background px-3.5 py-2.5">
        <span className="text-[13px] font-medium">{m.diff_title()}</span>
        <DiffSummary changes={changes} />
      </div>
      <div className="overflow-x-auto font-mono text-[12.5px] leading-[1.75]">
        {changes.map((c) => (
          <div key={c.path} data-testid="diff-file" className="pb-2">
            <div className="flex flex-wrap items-center gap-2 px-3.5 pt-2.5 pb-0.5 text-muted-foreground">
              <span>{c.path}</span>
              <span>· {KIND_LABEL[c.kind]()}</span>
              {c.user_file && <span>· {m.diff_userFile()}</span>}
            </div>
            {diffLines(c.diff).map((l, i) => (
              <div
                // biome-ignore lint/suspicious/noArrayIndexKey: the lines of a diff have no identity of their own and never move
                key={i}
                className={cn(
                  "px-3.5 whitespace-pre",
                  l.kind === "add" && "bg-emerald-500/10 text-emerald-800 dark:text-emerald-200",
                  l.kind === "del" && "bg-red-500/10 text-red-800 dark:text-red-200",
                  l.kind === "hunk" && "text-cyan-700 dark:text-cyan-300/70",
                  l.kind === "ctx" && "text-secondary-foreground",
                )}
              >
                {l.text}
              </div>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}
