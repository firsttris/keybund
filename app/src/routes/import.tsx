import { Link } from "@tanstack/react-router";
import { Check, KeyRound, Server } from "lucide-react";
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
import type { ImportProposal } from "@/lib/bindings";
import { useApplyChanges, useCleanupApply, useCleanupPreview, useImportProfiles, useImportProposals, usePlan } from "@/lib/queries";
import { hostLabel, providerLabel } from "@/lib/utils";

type Choice = { selected: boolean; name: string; folder: string };

export function ImportPage() {
  const proposals = useImportProposals();
  const plan = usePlan();
  const cleanup = useCleanupPreview();
  const pending = plan.data?.changes ?? [];
  const removable = cleanup.data?.changes ?? [];
  const nothingLeft = proposals.data?.length === 0 && pending.length === 0 && removable.length === 0;

  return (
    <>
      <div className="flex flex-col gap-1.5">
        <h1 className="m-0 text-[22px] font-semibold tracking-tight">Bestehende SSH-Config übernehmen</h1>
        <p className="m-0 max-w-[720px] leading-normal text-muted-foreground">
          Einträge mit eigenem Key aus <span className="font-mono">~/.ssh/config</span> werden zu Profilen. Deine Keys bleiben,
          wo sie sind. Danach entfernt Keybund die alten Einträge – solange sie dort stehen, bietet ssh ihre Keys zusätzlich an
          und eine Anmeldung kann mit dem falschen Key gelingen.
        </p>
      </div>
      {proposals.isPending && <Spinner />}
      {proposals.data && proposals.data.length > 0 && <Proposals proposals={proposals.data} />}
      {pending.length > 0 && <WriteConfigs />}
      {removable.length > 0 && <Cleanup blocked={pending.length > 0} />}
      {nothingLeft && (
        <p role="status" className="m-0 flex items-center gap-2 text-success">
          <Check aria-hidden className="size-4" />
          Alles übernommen – in ~/.ssh/config steht nichts mehr, das sich mit Keybund mischt.
        </p>
      )}
    </>
  );
}

function Proposals({ proposals }: { proposals: ImportProposal[] }) {
  const [choices, setChoices] = useState<Record<string, Choice>>(() =>
    Object.fromEntries(proposals.map((p) => [p.profile.id, { selected: p.key_exists, name: p.profile.name, folder: "" }])),
  );
  const importProfiles = useImportProfiles();
  const set = (id: string, patch: Partial<Choice>) => setChoices((c) => ({ ...c, [id]: { ...c[id], ...patch } }));
  const chosen = proposals
    .filter((p) => choices[p.profile.id]?.selected)
    .map((p) => {
      const c = choices[p.profile.id];
      return { ...p, profile: { ...p.profile, name: c.name.trim() || p.profile.name, folders: c.folder.trim() ? [c.folder.trim()] : [] } };
    });

  return (
    <section aria-labelledby="proposals-title" className="flex flex-col gap-4">
      <h2 id="proposals-title" className="m-0 text-[15px] font-semibold">
        1. Profile anlegen
      </h2>
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
                  {p.make_default && <Badge>Default</Badge>}
                  <Badge variant="muted">{providerLabel(p.profile.hosts[0].provider)}</Badge>
                </div>
              </CardHeader>
              <div className="flex items-center gap-2 text-[13px]">
                <KeyRound aria-hidden className="size-4 text-muted-foreground" />
                <span className="font-mono">{p.profile.key.path}</span>
              </div>
              {p.profile.hosts.map((h) => (
                <div key={h.alias ?? h.hostname} className="flex items-center gap-2 text-[13px]">
                  <Server aria-hidden className="size-4 text-muted-foreground" />
                  <span className="font-mono">{h.alias ?? h.hostname}</span>
                  <span className="text-xs text-muted-foreground">{hostLabel(h)}</span>
                </div>
              ))}
              <div className="grid grid-cols-2 gap-3">
                <div className="flex flex-col gap-1.5">
                  <Label htmlFor={`name-${p.profile.id}`} className="text-xs font-normal text-muted-foreground">
                    Name
                  </Label>
                  <Input id={`name-${p.profile.id}`} value={c.name} onChange={(e) => set(p.profile.id, { name: e.target.value })} />
                </div>
                <div className="flex flex-col gap-1.5">
                  <Label htmlFor={`folder-${p.profile.id}`} className="text-xs font-normal text-muted-foreground">
                    Projektordner (optional)
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
      {importProfiles.isError && <FieldError>{errorMessage(importProfiles.error)}</FieldError>}
      <Button
        className="self-start"
        disabled={chosen.length === 0 || importProfiles.isPending}
        onClick={() =>
          importProfiles.mutate(chosen, {
            onSuccess: (ids) => toast.success(`${ids.length} Profil(e) angelegt`),
          })
        }
      >
        {chosen.length === 1 ? "1 Profil übernehmen" : `${chosen.length} Profile übernehmen`}
      </Button>
    </section>
  );
}

function WriteConfigs() {
  const plan = usePlan();
  const apply = useApplyChanges();
  const changes = plan.data?.changes ?? [];
  return (
    <section aria-labelledby="write-title" className="flex flex-col gap-3">
      <h2 id="write-title" className="m-0 text-[15px] font-semibold">
        2. Keybund-Configs schreiben
      </h2>
      <DiffView changes={changes} />
      {apply.isError && <FieldError>{errorMessage(apply.error)}</FieldError>}
      <Button className="self-start" disabled={apply.isPending} onClick={() => apply.mutate(changes.map((c) => c.diff))}>
        Configs schreiben
      </Button>
    </section>
  );
}

function Cleanup({ blocked }: { blocked: boolean }) {
  const cleanup = useCleanupPreview();
  const apply = useCleanupApply();
  const changes = cleanup.data?.changes ?? [];
  return (
    <section aria-labelledby="cleanup-title" className="flex flex-col gap-3">
      <h2 id="cleanup-title" className="m-0 text-[15px] font-semibold">
        3. Alte Einträge aus ~/.ssh/config entfernen
      </h2>
      <p className="m-0 text-[13px] text-muted-foreground">
        Diese Einträge deckt jetzt Keybund ab. Vorher wird eine Sicherung angelegt (~/.ssh/config.keybund-backup).
      </p>
      <DiffView changes={changes} />
      {apply.isError && <FieldError>{errorMessage(apply.error)}</FieldError>}
      {blocked && <FieldHint>Erst die Keybund-Configs schreiben, sonst fehlen die Einträge zwischendurch.</FieldHint>}
      <Button
        variant="destructive"
        className="self-start"
        disabled={blocked || apply.isPending}
        onClick={() =>
          apply.mutate(
            changes.map((c) => c.diff),
            { onSuccess: () => toast.success("Alte Einträge entfernt") },
          )
        }
      >
        Alte Einträge entfernen
      </Button>
      <Link to="/doctor" className="text-[13px] text-muted-foreground">
        Danach im Doctor prüfen
      </Link>
    </section>
  );
}
