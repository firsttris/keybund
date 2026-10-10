import { KeyRound, ListChecks, TerminalSquare, X } from "lucide-react";
import { useState } from "react";

const STORAGE_KEY = "keybund.howItWorks.hidden";

function readHidden(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

function writeHidden(hidden: boolean) {
  try {
    if (hidden) localStorage.setItem(STORAGE_KEY, "1");
    else localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Only a convenience.
  }
}

const STEPS = [
  {
    icon: KeyRound,
    title: "Ein Key pro Zweck",
    text: "Für jeden Zweck ein eigener SSH-Key – Privat, Kunde A, NAS. Der private Teil bleibt auf deinem Rechner, den öffentlichen („Pubkey“) trägst du bei GitHub, GitLab oder auf dem Server ein.",
  },
  {
    icon: ListChecks,
    title: "Regeln statt Ausprobieren",
    text: "Keybund schreibt kleine Regeln in die Config von Git und ssh: „Repos in ~/dev/kunde-a/ → Key A“, „Server nas → Key B“. So nimmt nie ein Repo versehentlich den falschen Key.",
  },
  {
    icon: TerminalSquare,
    title: "Läuft ohne Keybund",
    text: "Git und ssh lesen diese Regeln selbst. Keybund muss dafür nicht laufen – du arbeitest wie gewohnt im Terminal, in der IDE oder in Dolphin.",
  },
];

/** A short explanation of the idea behind Keybund, dismissable. */
export function HowItWorks({ forceOpen = false }: { forceOpen?: boolean }) {
  const [hidden, setHidden] = useState(() => !forceOpen && readHidden());
  if (hidden) {
    return (
      <button
        type="button"
        className="self-start text-[13px] text-muted-foreground underline-offset-2 hover:underline"
        onClick={() => {
          writeHidden(false);
          setHidden(false);
        }}
      >
        Wie funktioniert Keybund?
      </button>
    );
  }
  return (
    <section aria-label="So funktioniert Keybund" className="relative flex flex-col gap-4 rounded-xl border border-border bg-card p-5">
      <div className="flex items-start justify-between gap-3">
        <h2 className="m-0 text-[15px] font-semibold">So funktioniert Keybund</h2>
        {!forceOpen && (
          <button
            type="button"
            aria-label="Erklärung ausblenden"
            className="text-muted-foreground hover:text-foreground"
            onClick={() => {
              writeHidden(true);
              setHidden(true);
            }}
          >
            <X aria-hidden className="size-4" />
          </button>
        )}
      </div>
      <ol className="m-0 grid list-none grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-4 p-0">
        {STEPS.map(({ icon: Icon, title, text }, i) => (
          <li key={title} className="flex gap-3">
            <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground">
              <Icon aria-hidden className="size-4" />
            </span>
            <span className="flex flex-col gap-1">
              <span className="text-[13px] font-medium">
                {i + 1}. {title}
              </span>
              <span className="text-[13px] leading-normal text-muted-foreground">{text}</span>
            </span>
          </li>
        ))}
      </ol>
    </section>
  );
}
