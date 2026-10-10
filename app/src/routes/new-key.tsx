import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { ArrowRight } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { ErrorSlot, FieldHint, Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { api, errorMessage } from "@/lib/api";
import type { NewProject } from "@/lib/bindings";
import { useCreateProject, useProfiles, useScanKeys } from "@/lib/queries";
import { cn, PROFILE_COLORS, slugify } from "@/lib/utils";

const mono = "font-mono text-[13px]";

/** A free color, so keys are easy to tell apart. */
function nextColor(used: (string | null)[]): string {
  const colors = Object.keys(PROFILE_COLORS).filter((c) => c !== "zinc");
  return colors.find((c) => !used.includes(c)) ?? colors[used.length % colors.length];
}

/**
 * Creating a key takes one short form: a name and where the key comes from.
 * What it is used for (folders, GitHub, servers) is set on the key's page.
 */
export function NewKeyPage({ existingKey }: { existingKey?: string }) {
  const navigate = useNavigate();
  const profiles = useProfiles();
  const scan = useScanKeys();
  const create = useCreateProject();
  const [name, setName] = useState("");
  const [mode, setMode] = useState<"generate" | "existing">(existingKey ? "existing" : "generate");
  const [existingPath, setExistingPath] = useState(existingKey ?? "");
  const [passphrase, setPassphrase] = useState("");
  const [comment, setComment] = useState<string | null>(null);
  const [submitted, setSubmitted] = useState(false);

  const trimmed = name.trim();
  const id = slugify(trimmed);
  const defaults = useQuery({
    queryKey: ["defaults", trimmed],
    queryFn: () => api.profileDefaults(trimmed),
    enabled: trimmed.length > 0,
  });
  const fresh = defaults.data?.id === id ? defaults.data : null;
  const unused = scan.data?.filter((k) => !k.used_by) ?? [];
  const keyPath = mode === "existing" ? existingPath : (fresh?.key_path ?? `~/.ssh/${id}_ed25519`);

  const error = !trimmed
    ? "Bitte einen Namen eingeben."
    : !id
      ? "Der Name braucht mindestens einen Buchstaben oder eine Ziffer."
      : fresh?.id_taken
        ? `Einen Key „${trimmed}“ gibt es schon.`
        : mode === "existing" && !existingPath
          ? "Bitte einen vorhandenen Key wählen."
          : mode === "generate" && fresh?.key_exists
            ? `${keyPath} gibt es schon – wähle „Vorhandenen Key nehmen“ oder einen anderen Namen.`
            : null;

  const submit = () => {
    setSubmitted(true);
    if (error) return;
    const project: NewProject = {
      profile: {
        id,
        name: trimmed,
        color: nextColor((profiles.data?.profiles ?? []).map((p) => p.profile.color)),
        key: { path: keyPath, comment: null },
        git: null,
        folders: [],
        archived: false,
        hosts: [],
      },
      key:
        mode === "existing"
          ? { type: "existing", path: existingPath }
          : { type: "generate", comment: (comment ?? fresh?.key_comment ?? id).trim(), passphrase: passphrase || null },
      make_default: false,
    };
    create.mutate(
      { project, openPage: false },
      { onSuccess: (created) => navigate({ to: "/profile/$id", params: { id: created.profile.profile.id } }) },
    );
  };

  return (
    <>
      <div className="flex max-w-[640px] flex-col gap-1.5">
        <h1 className="m-0 text-[26px] font-semibold tracking-tight">Neuer SSH-Key</h1>
        <p className="m-0 leading-normal text-muted-foreground">
          Gib dem Key einen Namen, an dem du ihn wiedererkennst. Wofür er gilt – Git-Ordner, GitHub, Server – legst du gleich
          danach fest.
        </p>
      </div>
      <Card className="max-w-[640px]">
        <form
          aria-label="Neuer Key"
          className="flex flex-col gap-5"
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="key-name">Name</Label>
            <Input
              id="key-name"
              autoFocus
              placeholder="z. B. Privat, Kunde C oder NAS"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>

          <div role="radiogroup" aria-label="Key" className="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-3">
            {(
              [
                { value: "generate", title: "Neuen Key erzeugen", text: id ? `ed25519 · ${keyPath}` : "empfohlen – ein eigener Key für diesen Zweck" },
                { value: "existing", title: "Vorhandenen Key nehmen", text: "einen Key, der schon in ~/.ssh liegt" },
              ] as const
            ).map((o) => (
              <button
                key={o.value}
                type="button"
                role="radio"
                aria-checked={mode === o.value}
                onClick={() => setMode(o.value)}
                className={cn(
                  "flex gap-3 rounded-[10px] border p-4 text-left",
                  mode === o.value ? "border-foreground bg-muted" : "border-border bg-transparent",
                )}
              >
                <span className={cn("mt-px size-[18px] shrink-0 rounded-full border", mode === o.value ? "border-[5px] border-foreground" : "border-zinc-600")} />
                <span className="flex min-w-0 flex-col gap-1">
                  <span className="font-medium">{o.title}</span>
                  <span className={cn("text-[12.5px] break-all text-muted-foreground", o.value === "generate" && id && "font-mono")}>{o.text}</span>
                </span>
              </button>
            ))}
          </div>

          {mode === "generate" ? (
            <div className="flex flex-col gap-3">
              <div className="flex flex-col gap-1.5">
                <Label htmlFor="key-pass">
                  Passphrase <span className="font-normal text-muted-foreground">(optional, empfohlen)</span>
                </Label>
                <Input id="key-pass" type="password" autoComplete="new-password" value={passphrase} onChange={(e) => setPassphrase(e.target.value)} />
                <FieldHint>
                  Schützt den Key, falls jemand an deine Dateien kommt. Du gibst sie einmal pro Sitzung ein, danach merkt sich der
                  ssh-agent den Key.
                </FieldHint>
              </div>
              <details>
                <summary className="cursor-pointer text-[13px] text-muted-foreground hover:text-foreground">Mehr Optionen</summary>
                <div className="mt-3 flex flex-col gap-1.5">
                  <Label htmlFor="key-comment">Kommentar</Label>
                  <Input
                    id="key-comment"
                    className={mono}
                    value={comment ?? fresh?.key_comment ?? ""}
                    onChange={(e) => setComment(e.target.value)}
                  />
                  <FieldHint>Steht am Ende des Pubkeys – daran erkennst du den Key bei GitHub oder auf dem Server wieder.</FieldHint>
                </div>
              </details>
            </div>
          ) : (
            <fieldset className="m-0 flex flex-col gap-2 border-0 p-0">
              <legend className="mb-2 text-[13px] font-medium">Key aus ~/.ssh</legend>
              {scan.isPending && <Spinner />}
              {scan.isSuccess && unused.length === 0 && <FieldHint>Keine ungenutzten Keys gefunden.</FieldHint>}
              {unused.map((k) => (
                <label
                  key={k.key.path}
                  className={cn("flex items-center gap-3 rounded-lg border p-3", existingPath === k.key.path ? "border-foreground" : "border-border")}
                >
                  <input type="radio" name="existing-key" checked={existingPath === k.key.path} onChange={() => setExistingPath(k.key.path)} />
                  <span className="flex min-w-0 flex-col gap-0.5">
                    <span className={mono}>{k.key.path}</span>
                    <span className="truncate font-mono text-xs text-muted-foreground">
                      {k.key.fingerprint}
                      {k.key.comment && ` · ${k.key.comment}`}
                    </span>
                  </span>
                </label>
              ))}
            </fieldset>
          )}

          <ErrorSlot message={(submitted && error) || (create.isError && errorMessage(create.error))} />
          <Button type="submit" size="lg" className="self-end" disabled={create.isPending}>
            {create.isPending && <Spinner className="size-3.5" />}
            Key anlegen
            <ArrowRight aria-hidden />
          </Button>
        </form>
      </Card>
    </>
  );
}
