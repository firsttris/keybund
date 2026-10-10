import { useQueryClient } from "@tanstack/react-query";
import { Copy, ExternalLink, KeyRound, LogIn } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { api, errorMessage } from "@/lib/api";
import type { DeviceFlowStart, Profile, UploadResult } from "@/lib/bindings";
import { openExternal } from "@/lib/open";
import { keys, useForgetToken, useUploadKey, useUploadTarget } from "@/lib/queries";
import { cn, providerLabel } from "@/lib/utils";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { FieldError, FieldHint, Label } from "./ui/label";
import { Spinner } from "./ui/spinner";
import { Switch } from "./ui/switch";

/**
 * Adds the profile's key to a GitHub/GitLab account: login in the browser
 * (device flow), a personal access token, or by hand. The token stays in
 * the backend.
 */
export function KeyUpload({
  profile,
  host,
  manual = true,
  onUploaded,
}: {
  profile: Profile;
  host: string;
  /** Offer the "add by hand" link (the wizard has its own). */
  manual?: boolean;
  onUploaded?: (result: UploadResult) => void;
}) {
  const target = useUploadTarget(profile.id, host);
  const upload = useUploadKey();
  const forget = useForgetToken();
  const client = useQueryClient();
  const tokenId = useId();
  const [flow, setFlow] = useState<DeviceFlowStart | null>(null);
  const [starting, setStarting] = useState(false);
  const [result, setResult] = useState<UploadResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [token, setToken] = useState("");
  const [showToken, setShowToken] = useState(false);
  const [remember, setRemember] = useState(false);
  // The poll loop reads the latest values without restarting.
  const rememberRef = useRef(remember);
  const done = useRef(onUploaded);
  useEffect(() => {
    rememberRef.current = remember;
    done.current = onUploaded;
  });

  useEffect(() => {
    if (!flow) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const finish = (r: UploadResult) => {
      setResult(r);
      void client.invalidateQueries({ queryKey: keys.uploadTarget(profile.id, host) });
      if (r.success) done.current?.(r);
    };
    const poll = (seconds: number) => {
      timer = setTimeout(async () => {
        try {
          const status = await api.pollDeviceFlow(flow.flow_id, rememberRef.current);
          if (!active) return;
          if (status.status === "pending") return poll(status.interval);
          active = false;
          setFlow(null);
          if (status.status === "done") finish(status.result);
          else setError(status.status === "denied" ? "Anmeldung abgelehnt." : "Der Code ist abgelaufen – bitte neu anmelden.");
        } catch (e) {
          if (!active) return;
          active = false;
          setFlow(null);
          setError(errorMessage(e));
        }
      }, seconds * 1000);
    };
    poll(flow.interval);
    return () => {
      clearTimeout(timer);
      if (active) void api.cancelDeviceFlow(flow.flow_id).catch(() => undefined);
      active = false;
    };
  }, [flow, client, profile.id, host]);

  if (target.isPending) return <Spinner label="Lade Optionen" />;
  if (target.isError) return <FieldError>{errorMessage(target.error)}</FieldError>;
  const t = target.data;
  const name = providerLabel(t.provider);
  const reset = () => {
    setResult(null);
    setError(null);
  };

  const startLogin = async () => {
    reset();
    setStarting(true);
    try {
      const f = await api.startDeviceFlow(profile.id, host);
      setFlow(f);
      void navigator.clipboard?.writeText(f.user_code).catch(() => undefined);
      await openExternal(f.verification_uri);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setStarting(false);
    }
  };

  const uploadWith = (value: string | null) => {
    reset();
    upload.mutate(
      { id: profile.id, host, token: value, remember },
      {
        onSuccess: (r) => {
          setResult(r);
          if (r.success) {
            setToken("");
            onUploaded?.(r);
          }
        },
        onError: (e) => setError(errorMessage(e)),
      },
    );
  };

  const manualLink = manual && t.manual_url && (
    <button className="self-start text-[13px] text-muted-foreground underline-offset-2 hover:underline" onClick={() => openExternal(t.manual_url!)}>
      Lieber von Hand: {name}-Seite öffnen
    </button>
  );

  if (!t.supported) {
    return (
      <div className="flex flex-col gap-2 text-[13px]">
        <span className="text-muted-foreground">
          {name} kann Keybund nicht automatisch eintragen. Kopiere den Pubkey und füge ihn auf der {name}-Seite ein.
        </span>
        {t.manual_url && (
          <Button variant="outline" size="sm" className="self-start" onClick={() => openExternal(t.manual_url!)}>
            <ExternalLink aria-hidden />
            {name}-Seite öffnen
          </Button>
        )}
      </div>
    );
  }

  if (result?.success) {
    return (
      <div role="status" className="flex flex-col gap-1 rounded-[10px] border border-emerald-500/30 bg-emerald-500/5 p-3.5 text-[13px]">
        <span className="font-medium text-success">{result.message}</span>
        {result.account && (
          <span className="text-muted-foreground">
            Gehört @{result.account} zu „{profile.name}“? Sonst den Key dort wieder entfernen und mit dem richtigen Konto anmelden.
          </span>
        )}
      </div>
    );
  }

  if (flow) {
    return (
      <div role="group" aria-label={`Anmeldung bei ${name}`} className="flex flex-col gap-3 rounded-[10px] border border-border bg-background p-4">
        <span className="text-[13px] text-muted-foreground">
          Gib diesen Code auf der {name}-Seite ein – angemeldet mit dem Konto für „{profile.name}“.
        </span>
        <div className="flex flex-wrap items-center gap-3">
          <code data-testid="user-code" className="font-mono text-[22px] font-semibold tracking-[0.12em]">
            {flow.user_code}
          </code>
          <Button variant="outline" size="sm" onClick={() => void navigator.clipboard?.writeText(flow.user_code).catch(() => undefined)}>
            <Copy aria-hidden />
            Code kopieren
          </Button>
          <Button variant="outline" size="sm" onClick={() => openExternal(flow.verification_uri)}>
            <ExternalLink aria-hidden />
            Seite öffnen
          </Button>
        </div>
        <div className="flex items-center justify-between gap-3">
          <span className="flex items-center gap-2 text-[13px] text-secondary-foreground">
            <Spinner className="size-3.5" label="Warte" />
            Warte auf Bestätigung im Browser …
          </span>
          <Button variant="ghost" size="sm" onClick={() => setFlow(null)}>
            Abbrechen
          </Button>
        </div>
      </div>
    );
  }

  const tokenForm = (
    <form
      aria-label="Mit Token eintragen"
      className="flex flex-col gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (token.trim()) uploadWith(token);
      }}
    >
      <Label htmlFor={tokenId}>Zugriffstoken</Label>
      <div className="flex flex-wrap gap-2">
        <Input
          id={tokenId}
          type="password"
          autoComplete="off"
          className="min-w-[220px] flex-1 font-mono text-[13px]"
          value={token}
          onChange={(e) => setToken(e.target.value)}
        />
        <Button type="submit" variant={t.device_flow ? "outline" : "default"} disabled={!token.trim() || upload.isPending}>
          {upload.isPending && <Spinner className="size-3.5" />}
          Mit Token eintragen
        </Button>
      </div>
      <FieldHint>
        Braucht die Berechtigung {t.scopes.map((s) => `„${s}“`).join(" und ")}.{" "}
        {t.token_url && (
          <button type="button" className="text-foreground underline-offset-2 hover:underline" onClick={() => openExternal(t.token_url!)}>
            Token erstellen
          </button>
        )}
      </FieldHint>
    </form>
  );

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap gap-2">
        {t.device_flow && (
          <Button onClick={startLogin} disabled={starting}>
            {starting ? <Spinner className="size-3.5" /> : <LogIn aria-hidden />}
            Bei {name} anmelden & eintragen
          </Button>
        )}
        {t.remembered && (
          <>
            <Button variant={t.device_flow ? "outline" : "default"} onClick={() => uploadWith(null)} disabled={upload.isPending}>
              <KeyRound aria-hidden />
              Mit gespeichertem Token eintragen
            </Button>
            <Button variant="ghost" onClick={() => forget.mutate({ id: profile.id, host })}>
              Token vergessen
            </Button>
          </>
        )}
      </div>
      {t.device_flow && !showToken ? (
        <button className="self-start text-[13px] text-muted-foreground underline-offset-2 hover:underline" onClick={() => setShowToken(true)}>
          Stattdessen ein Token verwenden
        </button>
      ) : (
        tokenForm
      )}
      {t.keyring && (showToken || !t.device_flow) && (
        <label className="flex items-center gap-2.5 text-[13px] text-secondary-foreground">
          <Switch aria-label="Token im Schlüsselbund merken" checked={remember} onCheckedChange={setRemember} />
          Token im Schlüsselbund merken (sonst wird es nach dem Eintragen vergessen)
        </label>
      )}
      <div aria-live="polite" className={cn("flex flex-col gap-2", !result && !error && "hidden")}>
        {result && !result.success && (
          <div className="flex flex-col gap-1 rounded-[10px] border border-red-400/30 bg-red-500/5 p-3.5 text-[13px]">
            <span className="font-medium text-danger">{result.message}</span>
            {result.auth_failed && <span className="text-muted-foreground">Bitte neu anmelden oder ein neues Token verwenden.</span>}
          </div>
        )}
        {error && <FieldError>{error}</FieldError>}
      </div>
      {manualLink}
    </div>
  );
}
