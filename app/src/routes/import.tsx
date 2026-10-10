import { Link } from "@tanstack/react-router";
import { Check, GitBranch, KeyRound, Server } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { DiffView } from "@/components/diff-view";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { FieldError, FieldHint, Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { errorMessage } from "@/lib/api";
import type { ImportDone, ImportProposal } from "@/lib/bindings";
import { useCleanupApply, useCleanupPreview, useImportProfiles, useImportProposals, useProfiles } from "@/lib/queries";
import { hostLabel, providerLabel } from "@/lib/utils";

type Choice = { selected: boolean; name: string; folder: string };

export function ImportPage() {
  const proposals = useImportProposals();
  const cleanup = useCleanupPreview();
  const [done, setDone] = useState<ImportDone | null>(null);
  const removable = cleanup.data?.changes ?? [];
  const nothingLeft = proposals.data?.length === 0 && removable.length === 0;

  return (
    <>
      <div className="flex flex-col gap-1.5">
        <h1 className="m-0 text-[22px] font-semibold tracking-tight">Bestehende SSH-Config übernehmen</h1>
        <p className="m-0 max-w-[720px] leading-normal text-muted-foreground">
          Einträge mit eigenem Key aus <span className="font-mono">~/.ssh/config</span> werden zu Keys in Keybund. Die Key-Dateien
          bleiben, wo sie sind.
        </p>
      </div>
      {done && <Done done={done} />}
      {proposals.isPending && <Spinner />}
      {proposals.isError && <FieldError>{errorMessage(proposals.error)}</FieldError>}
      {cleanup.isError && <FieldError>{errorMessage(cleanup.error)}</FieldError>}
      {proposals.data && proposals.data.length > 0 && <Proposals proposals={proposals.data} onDone={setDone} />}
      {proposals.data?.length === 0 && removable.length > 0 && <Cleanup />}
      {nothingLeft && !done && (
        <p role="status" className="m-0 flex items-center gap-2 text-success">
          <Check aria-hidden className="size-4" />
          Alles übernommen – in ~/.ssh/config steht nichts mehr, das sich mit Keybund mischt.
        </p>
      )}
    </>
  );
}

function Done({ done }: { done: ImportDone }) {
  const { written, backups } = done.report;
  const profiles = useProfiles();
  const name = (id: string) => profiles.data?.profiles.find((p) => p.profile.id === id)?.profile.name ?? id;
  return (
    <div role="status" className="flex flex-col gap-2 rounded-[10px] border border-emerald-500/30 bg-emerald-500/5 p-4 text-[13px]">
      <span className="flex items-center gap-2 font-medium text-success">
        <Check aria-hidden className="size-4" />
        Alles übernommen: {done.ids.length === 1 ? "1 Key" : `${done.ids.length} Keys`}
        {written.includes("~/.ssh/config") && ", alte Einträge entfernt"}
      </span>
      <span className="text-muted-foreground">
        Git und ssh benutzen ab sofort die Regeln von Keybund. Teste die Verbindung am besten einmal auf der Seite des Keys.
      </span>
      <span className="flex flex-wrap gap-x-4 gap-y-1">
        {done.ids.map((id) => (
          <Link key={id} to="/profile/$id" params={{ id }} className="font-medium text-foreground">
            {name(id)} öffnen
          </Link>
        ))}
        <Link to="/doctor" className="text-foreground">
          Im Doctor prüfen
        </Link>
      </span>
      <details>
        <summary className="cursor-pointer text-xs text-muted-foreground">Geänderte Dateien</summary>
        <ul className="m-0 mt-2 flex list-none flex-col gap-1 p-0 font-mono text-xs text-secondary-foreground">
          {written.map((f) => (
            <li key={f}>{f}</li>
          ))}
          {backups.map((b) => (
            <li key={b} className="text-muted-foreground">
              Backup: {b}
            </li>
          ))}
        </ul>
      </details>
    </div>
  );
}

function Proposals({ proposals, onDone }: { proposals: ImportProposal[]; onDone: (d: ImportDone) => void }) {
  const [edited, setChoices] = useState<Record<string, Choice>>({});
  // Proposals can change while the page is open (refetch): new ones get defaults.
  const choices: Record<string, Choice> = Object.fromEntries(
    proposals.map((p) => [p.profile.id, edited[p.profile.id] ?? { selected: p.key_exists, name: p.profile.name, folder: "" }]),
  );
  const [removeOld, setRemoveOld] = useState(true);
  const importProfiles = useImportProfiles();
  const set = (id: string, patch: Partial<Choice>) => setChoices((c) => ({ ...c, [id]: { ...choices[id], ...c[id], ...patch } }));
  const chosen = proposals
    .filter((p) => choices[p.profile.id]?.selected)
    .map((p) => {
      const c = choices[p.profile.id];
      return { ...p, profile: { ...p.profile, name: c.name.trim() || p.profile.name, folders: c.folder.trim() ? [c.folder.trim()] : [] } };
    });

  return (
    <section aria-label="Vorschläge" className="flex flex-col gap-4">
      <div className="grid grid-cols-[repeat(auto-fit,minmax(320px,1fr))] gap-4">
        {proposals.map((p) => {
          const c = choices[p.profile.id];
          return (
            <Card key={p.profile.id} aria-label={p.profile.name} className={c.selected ? "" : "opacity-60"}>
              <CardHeader>
                <label className="flex items-center gap-2.5">
                  <input
                    type="checkbox"
                    checked={c.selected}
                    disabled={!p.key_exists}
                    onChange={(e) => set(p.profile.id, { selected: e.target.checked })}
                    aria-label={`${p.profile.name} übernehmen`}
                  />
                  <CardTitle>{p.profile.name}</CardTitle>
                </label>
                <div className="flex gap-1.5">
                  {p.make_default && <Badge>Standard</Badge>}
                  <Badge variant="muted">{providerLabel(p.profile.hosts[0].provider)}</Badge>
                </div>
              </CardHeader>
              <div className="flex items-center gap-2 text-[13px]">
                <KeyRound aria-hidden className="size-4 text-muted-foreground" />
                <span className="font-mono">{p.profile.key.path}</span>
              </div>
              {p.profile.hosts.map((h) => {
                const git = h.provider !== "generic";
                return (
                  <div key={h.alias ?? h.hostname} className="flex flex-col gap-0.5 text-[13px]">
                    <span className="flex items-center gap-2">
                      {git ? (
                        <GitBranch aria-hidden className="size-4 text-muted-foreground" />
                      ) : (
                        <Server aria-hidden className="size-4 text-muted-foreground" />
                      )}
                      <span className="font-mono">{h.alias ?? h.hostname}</span>
                      <span className="text-xs text-muted-foreground">{hostLabel(h)}</span>
                    </span>
                    {git && h.alias && (
                      <span className="pl-6 text-xs text-muted-foreground">
                        Alias: Repos mit <span className="font-mono">git@{h.alias}:…</span> nutzen diesen Key. Mit Git-Ordner gilt er
                        auch für <span className="font-mono">git@{h.hostname}:…</span> in diesem Ordner.
                      </span>
                    )}
                  </div>
                );
              })}
              <div className="grid grid-cols-2 gap-3">
                <div className="flex flex-col gap-1.5">
                  <Label htmlFor={`name-${p.profile.id}`} className="text-xs font-normal text-muted-foreground">
                    Name
                  </Label>
                  <Input id={`name-${p.profile.id}`} value={c.name} onChange={(e) => set(p.profile.id, { name: e.target.value })} />
                </div>
                <div className="flex flex-col gap-1.5">
                  <Label htmlFor={`folder-${p.profile.id}`} className="text-xs font-normal text-muted-foreground">
                    Git-Ordner (optional)
                  </Label>
                  <Input
                    id={`folder-${p.profile.id}`}
                    className="font-mono"
                    placeholder="~/dev/…"
                    value={c.folder}
                    onChange={(e) => set(p.profile.id, { folder: e.target.value })}
                  />
                </div>
              </div>
              {p.notes.map((n) => (
                <FieldHint key={n}>{n}</FieldHint>
              ))}
            </Card>
          );
        })}
      </div>
      <label className="flex max-w-[720px] items-start gap-3 text-[13px]">
        <input type="checkbox" className="mt-0.5" checked={removeOld} onChange={(e) => setRemoveOld(e.target.checked)} />
        <span className="flex flex-col gap-0.5">
          <span className="font-medium">Alte Einträge danach aus ~/.ssh/config entfernen (empfohlen)</span>
          <span className="text-muted-foreground">
            Solange sie dort stehen, bietet ssh ihre Keys zusätzlich an – eine Anmeldung kann dann mit dem falschen Key gelingen.
            Vorher wird ein Backup angelegt (~/.ssh/config.keybund-backup).
          </span>
        </span>
      </label>
      {importProfiles.isError && <FieldError>{errorMessage(importProfiles.error)}</FieldError>}
      <Button
        className="self-start"
        disabled={chosen.length === 0 || importProfiles.isPending}
        onClick={() => importProfiles.mutate({ chosen, removeOld }, { onSuccess: onDone })}
      >
        {importProfiles.isPending && <Spinner className="size-3.5" />}
        {chosen.length === 1 ? "1 Key übernehmen" : `${chosen.length} Keys übernehmen`}
      </Button>
    </section>
  );
}

/** Old blocks that Keybund covers already, e.g. after an import that kept them. */
function Cleanup() {
  const cleanup = useCleanupPreview();
  const apply = useCleanupApply();
  const changes = cleanup.data?.changes ?? [];
  return (
    <section aria-labelledby="cleanup-title" className="flex flex-col gap-3">
      <h2 id="cleanup-title" className="m-0 text-[15px] font-semibold">
        Alte Einträge aus ~/.ssh/config entfernen
      </h2>
      <p className="m-0 max-w-[720px] text-[13px] text-muted-foreground">
        Diese Einträge deckt jetzt Keybund ab. Solange sie dort stehen, bietet ssh ihre Keys zusätzlich an. Vorher wird ein Backup
        angelegt (~/.ssh/config.keybund-backup).
      </p>
      <details>
        <summary className="cursor-pointer text-[13px] text-muted-foreground">Änderung ansehen</summary>
        <div className="mt-2">
          <DiffView changes={changes} />
        </div>
      </details>
      {apply.isError && <FieldError>{errorMessage(apply.error)}</FieldError>}
      <Button variant="destructive" className="self-start" disabled={apply.isPending} onClick={() => apply.mutate(changes.map((c) => c.diff), { onSuccess: () => toast.success("Alte Einträge entfernt") })}>
        Alte Einträge entfernen
      </Button>
    </section>
  );
}
