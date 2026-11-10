import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
import { fill, useT } from "@/lib/i18n";
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
  const t = useT();
  const [info, setInfo] = useState<DebugInfo | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saved, setSaved] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    const poll = () => api.debugSnapshot().then((d) => alive && setInfo(d)).catch(() => {});
    void poll();
    const t = window.setInterval(poll, 1000);
    void api.getSettings().then(setSettings);
    let unlisten: (() => void) | undefined;
    if (isTauri()) {
      void listen<Settings>("voice://settings", (event) => setSettings(event.payload)).then((fn) => {
        unlisten = fn;
      });
    }
    return () => {
      alive = false;
      window.clearInterval(t);
      unlisten?.();
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
        {t("Logs, echo guard, and wake sensitivity. Voice keys, packs, and the daily budget are on the Settings screen you just left.")}
      </p>
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-sm">
            <ShieldCheck className="size-4 text-primary" /> {t("Token & vault")}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <Row k={t("API key")} v={info?.keyHint ?? t("none")} tone={info?.keyHint ? "ok" : "bad"} />
          <Row
            k={t("Vault key protection")}
            v={info?.vaultSecurityLevel ?? t("locked")}
            tone={vaultTone(info?.vaultSecurityLevel ?? null)}
          />
          <Row k={t("Storage")} v={t("Stronghold snapshot (app-private)")} />
          <Row k={t("Last ephemeral token")} v={fmtTime(info?.tokenMintedAtMs ?? null)} />
          <Row
            k={t("Token expiry")}
            v={expiresIn == null ? "—" : expiresIn > 0 ? fill("{n}s left", { n: expiresIn }) : t("expired (re-minted per session)")}
            tone={expiresIn != null && expiresIn > 0 ? "ok" : undefined}
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-sm">
            <Activity className="size-4 text-primary" /> {t("Realtime link")}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <Row k={t("Phase")} v={<Badge variant="outline">{info?.phase ?? voice.phase}</Badge>} />
          <Row
            k={t("WebSocket")}
            v={info?.wsState ?? "—"}
            tone={info?.wsState === "open" ? "ok" : info?.wsState === "closed" ? undefined : "warn"}
          />
          <Row k={t("Model")} v={info?.model ?? settings?.model ?? "—"} />
          <Row k={t("Sessions")} v={info?.sessions ?? 0} />
          <Row k={t("Uplink")} v={`${info?.framesSent ?? 0} ${t("frames")} · ${fmtBytes(info?.bytesSent ?? 0)}`} />
          <Row
            k={t("Dropped uplink frames")}
            v={info?.framesDropped ?? 0}
            tone={(info?.framesDropped ?? 0) > 0 ? "warn" : undefined}
          />
          <Row k={t("Downlink audio")} v={`${info?.audioDeltas ?? 0} ${t("deltas")} · ${fmtBytes(info?.bytesReceived ?? 0)}`} />
          <Row k={t("Server events")} v={info?.eventsReceived ?? 0} />
          <Row k={t("Last event")} v={info?.lastEvent ?? "—"} />
          <Separator className="my-2" />
          <Row k={t("Mic")} v={info?.micRate ? `${info.micRate} Hz → 16 kHz` : t("closed")} />
          <Row
            k={t("Mic path")}
            v={info?.micSource ? (info.micSource === "starting" ? t("starting") : info.micSource) : t("closed")}
            tone={info?.micSource === "audiorecord" || info?.micSource === "cpal" ? "ok" : info?.micSource === "starting" ? "warn" : undefined}
          />
          <Row k={t("Listen preference")} v={info?.listenPreference ? t("on") : t("off")} tone={info?.listenPreference ? "ok" : undefined} />
          <Row k={t("Service armed")} v={info?.serviceArmed ? t("on") : t("off")} tone={info?.serviceArmed ? "ok" : undefined} />
          <Row k={t("Foreground service")} v={info?.foregroundService ? t("running") : t("stopped")} tone={info?.foregroundService ? "ok" : undefined} />
          <Row k={t("Screen")} v={info?.screenInteractive ? t("on") : t("off")} />
          <Row k={t("Activity")} v={info?.activityResumed ? t("visible") : t("background")} />
          <Row k={t("Last stop")} v={info?.lastStopReason || "—"} tone={info?.lastStopReason ? "warn" : undefined} />
          <Row k={t("Speaker")} v={info?.speakerRate ? `24 kHz → ${info.speakerRate} Hz` : t("closed")} />
          <Row k={t("Wake word")} v={info?.wakewordTrained ? t("trained") : t("not trained")} tone={info?.wakewordTrained ? "ok" : "warn"} />
          <Row k={t("Last wake score")} v={info?.lastWakeScore?.toFixed(3) ?? "—"} />
          <Row k={t("Platform")} v={info?.platform ?? "—"} />
          {info?.lastError && <Row k={t("Last error")} v={t(info.lastError)} tone="bad" />}
          {(info?.listenerHistory?.length ?? 0) > 0 && (
            <>
              <Separator className="my-2" />
              <p className="py-1 text-xs text-muted-foreground">{t("Background listener")}</p>
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
            <Terminal className="size-4 text-primary" /> {t("Log")}
          </CardTitle>
          <Button size="sm" variant="ghost" onClick={voice.clearLogs}>
            <Trash2 />
          </Button>
        </CardHeader>
        <CardContent>
          <ScrollArea className="h-56 rounded-md border bg-black/40 p-2">
            <div className="space-y-0.5 font-mono text-xs leading-snug">
              {voice.logs.length === 0 && <p className="text-muted-foreground">{t("No events yet.")}</p>}
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
            <CardTitle className="text-sm">{t("Settings")}</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1">
                <Label htmlFor="voice">{t("Voice")}</Label>
                <Input id="voice" value={settings.voice} onChange={(e) => setSettings({ ...settings, voice: e.target.value })} />
              </div>
              <div className="space-y-1">
                <Label htmlFor="model">{t("Model")}</Label>
                <Input id="model" value={settings.model} onChange={(e) => setSettings({ ...settings, model: e.target.value })} />
              </div>
            </div>
            <div className="space-y-1">
              <Label htmlFor="instructions">{t("Instructions")}</Label>
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
                <Label htmlFor="threshold">{fill("Wake threshold ({n})", { n: settings.wakeThreshold.toFixed(2) })}</Label>
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
                <Label htmlFor="idle">{t("Idle hang-up (s)")}</Label>
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
                <Label htmlFor="echo">{t("Speaker echo guard")}</Label>
                <p className="text-sm leading-5 text-muted-foreground">
                  {t("Mutes the mic while Grok talks. Turn off with headphones to allow interrupting.")}
                </p>
              </div>
              <Switch id="echo" checked={settings.echoGuard} onCheckedChange={(v) => setSettings({ ...settings, echoGuard: v })} />
            </div>
            <Button className="w-full" onClick={() => void save()}>
              <Save /> {t("Save settings")}
            </Button>
            {saved && <p className="text-center text-xs text-muted-foreground">{t(saved)}</p>}
          </CardContent>
        </Card>
      )}

      <Button variant="outline" className="w-full" onClick={() => void replay()}>
        <Compass /> {t("Replay setup tour")}
      </Button>
      <Button variant="destructive" className="w-full" onClick={() => void signOut()}>
        <LogOut /> {t("Sign out (erase key from device)")}
      </Button>
    </div>
  );
}
