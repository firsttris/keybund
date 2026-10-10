import { useMutation } from "@tanstack/react-query";
import { Check } from "lucide-react";
import { useState } from "react";
import { ColorDot } from "@/components/color-dot";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { FieldError, FieldHint, Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { api, errorMessage } from "@/lib/api";
import type { ClonePreview } from "@/lib/bindings";
import { useProfiles } from "@/lib/queries";
import { cn } from "@/lib/utils";

export function ClonePage() {
  const [url, setUrl] = useState("");
  const [profileId, setProfileId] = useState<string | null>(null);
  const [target, setTarget] = useState("");
  const profiles = useProfiles();
  const preview = useMutation({
    mutationFn: (u: string) => api.clonePreview(u),
    onSuccess: (p: ClonePreview) => {
      const first = p.candidates[0];
      setProfileId(p.matched && first ? first.profile_id : null);
      setTarget(p.matched && first?.target ? first.target : "");
    },
  });
  const clone = useMutation({ mutationFn: () => api.cloneRepo(url.trim(), profileId!, target.trim() || null) });

  const choose = (id: string) => {
    setProfileId(id);
    setTarget(preview.data?.candidates.find((c) => c.profile_id === id)?.target ?? "");
  };

  return (
    <>
      <div className="flex flex-col gap-1.5">
        <h1 className="m-0 text-[22px] font-semibold tracking-tight">Repo klonen</h1>
        <p className="m-0 text-muted-foreground">
          Beim ersten Klonen greift includeIf noch nicht – Keybund klont deshalb direkt mit dem passenden Key.
        </p>
      </div>
      <Card className="max-w-[720px]">
        <form
          className="flex flex-col gap-1.5"
          onSubmit={(e) => {
            e.preventDefault();
            clone.reset();
            if (url.trim()) preview.mutate(url.trim());
          }}
        >
          <Label htmlFor="clone-url">Repo-URL</Label>
          <div className="flex flex-wrap gap-2">
            <Input id="clone-url" className="flex-[1_1_280px] font-mono" placeholder="git@github.com:team/repo.git" value={url} onChange={(e) => setUrl(e.target.value)} />
            <Button type="submit" variant="outline" disabled={!url.trim()}>
              Prüfen
            </Button>
          </div>
        </form>
        {preview.isError && <FieldError>{errorMessage(preview.error)}</FieldError>}
        {preview.data && (
          <>
            <fieldset className="m-0 flex flex-col gap-2 border-0 p-0">
              <legend className="mb-2 text-[13px] font-medium">Key</legend>
              {!preview.data.matched && (
                <FieldHint className="mb-1">Kein Key ist für {preview.data.repo.host} eingerichtet – bitte einen wählen.</FieldHint>
              )}
              {preview.data.candidates.map((c) => {
                const color = profiles.data?.profiles.find((p) => p.profile.id === c.profile_id)?.profile.color;
                return (
                  <label key={c.profile_id} className={cn("flex items-center gap-3 rounded-lg border p-3", profileId === c.profile_id ? "border-foreground" : "border-border")}>
                    <input type="radio" name="clone-profile" checked={profileId === c.profile_id} onChange={() => choose(c.profile_id)} />
                    <ColorDot color={color} />
                    <span className="flex-1">{c.profile_name}</span>
                    <span className="font-mono text-xs text-muted-foreground">{c.target ?? "kein Ordner"}</span>
                  </label>
                );
              })}
            </fieldset>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="clone-target">Zielordner</Label>
              <Input id="clone-target" className="font-mono" value={target} onChange={(e) => setTarget(e.target.value)} />
            </div>
            <Button className="self-end" disabled={!profileId || !target.trim() || clone.isPending} onClick={() => clone.mutate()}>
              {clone.isPending && <Spinner className="size-3.5" />}
              Klonen
            </Button>
          </>
        )}
        {clone.isError && <FieldError>{errorMessage(clone.error)}</FieldError>}
        {clone.data && (
          <p role="status" className="m-0 flex items-center gap-2 text-success">
            <Check aria-hidden className="size-4" />
            Geklont nach <span className="font-mono">{clone.data}</span>
          </p>
        )}
      </Card>
    </>
  );
}
