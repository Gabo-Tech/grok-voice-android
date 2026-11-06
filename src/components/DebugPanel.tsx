import { useEffect, useState } from "react";
import { Activity, Compass, LogOut, Save, ShieldCheck, Terminal, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import type { VoiceState } from "@/hooks/useVoice";
import { api, errorText, type DebugInfo, type Settings } from "@/lib/api";
import { cn } from "@/lib/utils";

function fmtBytes(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}

function fmtTime(ms: number | null) {
  return ms ? new Date(ms).toLocaleTimeString() : "—";
}

function Row({ k, v, tone }: { k: string; v: React.ReactNode; tone?: "ok" | "warn" | "bad" }) {
  return (
    <div className="flex items-baseline justify-between gap-4 py-1 text-xs">
      <span className="text-muted-foreground">{k}</span>
      <span
        className={cn(
          "truncate text-right font-medium",
          tone === "ok" && "text-foreground",
          tone === "warn" && "text-muted-foreground",
          tone === "bad" && "text-destructive",
        )}
      >
        {v}
      </span>
    </div>
  );
}

function vaultTone(level: string | null): "ok" | "warn" | "bad" {
  if (level === "strongbox" || level === "tee" || level === "hardware") return "ok";
  if (level === "software") return "warn";
  return "bad";
}

export function DebugPanel({
  voice,
  onSignedOut,
  onReplayTour,
}: {
  voice: VoiceState;
  onSignedOut: () => void;
  onReplayTour: () => void;
}) {
  const [info, setInfo] = useState<DebugInfo | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saved, setSaved] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    const poll = () => api.debugSnapshot().then((d) => alive && setInfo(d)).catch(() => {});
    void poll();
    const t = window.setInterval(poll, 1000);
    void api.getSettings().then(setSettings);
    return () => {
      alive = false;
      window.clearInterval(t);
    };
  }, []);

  const save = async () => {
    if (!settings) return;
    try {
      setSettings(await api.saveSettings(settings));
      setSaved("Saved — applies to the next session.");
    } catch (e) {
      setSaved(errorText(e));
    }
    window.setTimeout(() => setSaved(null), 2500);
  };

  const signOut = async () => {
    await api.clearUserToken();
    try {
      const current = settings ?? (await api.getSettings());
      await api.saveSettings({ ...current, onboarded: false });
    } catch {
      // The key is already gone, so setup shows again either way.
    }
    onSignedOut();
  };

  const replay = async () => {
    try {
      const current = settings ?? (await api.getSettings());
      await api.saveSettings({ ...current, onboarded: false });
      onReplayTour();
    } catch (e) {
      setSaved(errorText(e));
    }
  };

  const expiresIn =
    info?.tokenExpiresAt != null ? Math.round(info.tokenExpiresAt - Date.now() / 1000) : null;

  return (
    <div className="space-y-4">
      <p className="px-1 text-sm leading-6 text-muted-foreground">
        Logs, echo guard, and wake sensitivity. Voice keys, packs, and the daily budget are on the Settings screen you just left.
      </p>
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-sm">
            <ShieldCheck className="size-4 text-primary" /> Token & vault
          </CardTitle>
        </CardHeader>
        <CardContent>
          <Row k="API key" v={info?.keyHint ?? "none"} tone={info?.keyHint ? "ok" : "bad"} />
          <Row
            k="Vault key protection"
            v={info?.vaultSecurityLevel ?? "locked"}
            tone={vaultTone(info?.vaultSecurityLevel ?? null)}
          />
          <Row k="Storage" v="Stronghold snapshot (app-private)" />
          <Row k="Last ephemeral token" v={fmtTime(info?.tokenMintedAtMs ?? null)} />
          <Row
            k="Token expiry"
            v={expiresIn == null ? "—" : expiresIn > 0 ? `${expiresIn}s left` : "expired (re-minted per session)"}
            tone={expiresIn != null && expiresIn > 0 ? "ok" : undefined}
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-sm">
            <Activity className="size-4 text-primary" /> Realtime link
          </CardTitle>
        </CardHeader>
        <CardContent>
          <Row k="Phase" v={<Badge variant="outline">{info?.phase ?? voice.phase}</Badge>} />
          <Row
            k="WebSocket"
            v={info?.wsState ?? "—"}
            tone={info?.wsState === "open" ? "ok" : info?.wsState === "closed" ? undefined : "warn"}
          />
          <Row k="Model" v={info?.model ?? settings?.model ?? "—"} />
          <Row k="Sessions" v={info?.sessions ?? 0} />
          <Row k="Uplink" v={`${info?.framesSent ?? 0} frames · ${fmtBytes(info?.bytesSent ?? 0)}`} />
          <Row
            k="Dropped uplink frames"
            v={info?.framesDropped ?? 0}
            tone={(info?.framesDropped ?? 0) > 0 ? "warn" : undefined}
          />
          <Row k="Downlink audio" v={`${info?.audioDeltas ?? 0} deltas · ${fmtBytes(info?.bytesReceived ?? 0)}`} />
          <Row k="Server events" v={info?.eventsReceived ?? 0} />
          <Row k="Last event" v={info?.lastEvent ?? "—"} />
          <Separator className="my-2" />
          <Row k="Mic" v={info?.micRate ? `${info.micRate} Hz → 16 kHz` : "closed"} />
          <Row
            k="Mic path"
            v={info?.micSource || "closed"}
            tone={info?.micSource === "audiorecord" || info?.micSource === "cpal" ? "ok" : info?.micSource === "starting" ? "warn" : undefined}
          />
          <Row k="Foreground service" v={info?.foregroundService ? "running" : "stopped"} tone={info?.foregroundService ? "ok" : undefined} />
          <Row k="Screen" v={info?.screenInteractive ? "on" : "off"} />
          <Row k="Activity" v={info?.activityResumed ? "visible" : "background"} />
          <Row k="Last stop" v={info?.lastStopReason || "—"} tone={info?.lastStopReason ? "warn" : undefined} />
          <Row k="Speaker" v={info?.speakerRate ? `24 kHz → ${info.speakerRate} Hz` : "closed"} />
          <Row k="Wake word" v={info?.wakewordTrained ? "trained" : "not trained"} tone={info?.wakewordTrained ? "ok" : "warn"} />
          <Row k="Last wake score" v={info?.lastWakeScore?.toFixed(3) ?? "—"} />
          <Row k="Platform" v={info?.platform ?? "—"} />
          {info?.lastError && <Row k="Last error" v={info.lastError} tone="bad" />}
          {(info?.listenerHistory?.length ?? 0) > 0 && (
            <>
              <Separator className="my-2" />
              <p className="py-1 text-xs text-muted-foreground">Background listener</p>
              <div className="max-h-40 space-y-0.5 overflow-y-auto font-mono text-xs leading-snug">
                {info?.listenerHistory?.slice(-20).map((line, i) => (
                  <p key={`${line}-${i}`}>{line}</p>
                ))}
              </div>
            </>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="flex-row items-center justify-between">
          <CardTitle className="flex items-center gap-2 text-sm">
            <Terminal className="size-4 text-primary" /> Log
          </CardTitle>
          <Button size="sm" variant="ghost" onClick={voice.clearLogs}>
            <Trash2 />
          </Button>
        </CardHeader>
        <CardContent>
          <ScrollArea className="h-56 rounded-md border bg-black/40 p-2">
            <div className="space-y-0.5 font-mono text-xs leading-snug">
              {voice.logs.length === 0 && <p className="text-muted-foreground">No events yet.</p>}
              {voice.logs
                .slice()
                .reverse()
                .map((l, i) => (
                  <p
                    key={`${l.tsMs}-${i}`}
                    className={cn(
                      l.level === "error" && "text-destructive",
                      l.level === "warn" && "text-muted-foreground",
                    )}
                  >
                    <span className="text-muted-foreground">{new Date(l.tsMs).toLocaleTimeString()} </span>
                    {l.msg}
                  </p>
                ))}
            </div>
          </ScrollArea>
        </CardContent>
      </Card>

      {settings && (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Settings</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1">
                <Label htmlFor="voice">Voice</Label>
                <Input id="voice" value={settings.voice} onChange={(e) => setSettings({ ...settings, voice: e.target.value })} />
              </div>
              <div className="space-y-1">
                <Label htmlFor="model">Model</Label>
                <Input id="model" value={settings.model} onChange={(e) => setSettings({ ...settings, model: e.target.value })} />
              </div>
            </div>
            <div className="space-y-1">
              <Label htmlFor="instructions">Instructions</Label>
              <textarea
                id="instructions"
                rows={4}
                className="min-h-32 w-full border border-input bg-transparent px-3 py-3 text-base leading-relaxed outline-none focus-visible:border-foreground"
                value={settings.instructions}
                onChange={(e) => setSettings({ ...settings, instructions: e.target.value })}
              />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1">
                <Label htmlFor="threshold">Wake threshold ({settings.wakeThreshold.toFixed(2)})</Label>
                <input
                  id="threshold"
                  type="range"
                  min={0.3}
                  max={0.9}
                  step={0.01}
                  className="w-full accent-white"
                  value={settings.wakeThreshold}
                  onChange={(e) => setSettings({ ...settings, wakeThreshold: Number(e.target.value) })}
                />
              </div>
              <div className="space-y-1">
                <Label htmlFor="idle">Idle hang-up (s)</Label>
                <Input
                  id="idle"
                  type="number"
                  min={5}
                  max={600}
                  value={settings.idleTimeoutSecs}
                  onChange={(e) => setSettings({ ...settings, idleTimeoutSecs: Number(e.target.value) })}
                />
              </div>
            </div>
            <div className="flex items-center justify-between">
              <div>
                <Label htmlFor="echo">Speaker echo guard</Label>
                <p className="text-sm leading-5 text-muted-foreground">
                  Mutes the mic while Grok talks. Turn off with headphones to allow interrupting.
                </p>
              </div>
              <Switch id="echo" checked={settings.echoGuard} onCheckedChange={(v) => setSettings({ ...settings, echoGuard: v })} />
            </div>
            <Button className="w-full" onClick={() => void save()}>
              <Save /> Save settings
            </Button>
            {saved && <p className="text-center text-xs text-muted-foreground">{saved}</p>}
          </CardContent>
        </Card>
      )}

      <Button variant="outline" className="w-full" onClick={() => void replay()}>
        <Compass /> Replay setup tour
      </Button>
      <Button variant="destructive" className="w-full" onClick={() => void signOut()}>
        <LogOut /> Sign out (erase key from device)
      </Button>
    </div>
  );
}
