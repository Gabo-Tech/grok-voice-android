import { useEffect, useState } from "react";
import { Bug, ChevronRight, KeyRound, Wand2 } from "lucide-react";
import { DebugPanel } from "@/components/DebugPanel";
import { Field } from "@/components/Field";
import { PageChrome, RowButton } from "@/components/PageChrome";
import { WakeView } from "@/components/WakeView";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { VoiceState } from "@/hooks/useVoice";
import { api, errorText, type AuthStatus, type Settings, type WakewordStatus } from "@/lib/api";
import { cn } from "@/lib/utils";

type Section = "menu" | "wake" | "debug" | "keys";

const PROVIDERS = [
  { id: "xai", label: "xAI" },
  { id: "openai", label: "OpenAI" },
  { id: "azure", label: "Azure" },
  { id: "elevenlabs", label: "ElevenLabs" },
  { id: "local", label: "On device" },
] as const;

export function MorePage({
  voice,
  wake,
  auth,
  onAuth,
  onWake,
  onSignedOut,
  onReplayTour,
}: {
  voice: VoiceState;
  wake: WakewordStatus;
  auth: AuthStatus;
  onAuth: (auth: AuthStatus) => void;
  onWake: (s: WakewordStatus) => void;
  onSignedOut: () => void;
  onReplayTour: () => void;
}) {
  const [section, setSection] = useState<Section>("menu");
  const [quiet, setQuiet] = useState(10);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saved, setSaved] = useState("");

  useEffect(() => {
    void api
      .getSettings()
      .then((s) => {
        setQuiet(s.idleTimeoutSecs);
        setSettings(s);
      })
      .catch(() => {});
  }, []);

  const setQuietGap = (seconds: number) => {
    const next = Math.min(120, Math.max(5, seconds));
    setQuiet(next);
    void api.getSettings().then((current) => {
      const merged = { ...current, idleTimeoutSecs: next };
      setSettings(merged);
      return api.saveSettings(merged);
    });
  };

  const saveModel = () => {
    if (!settings) return;
    void api.saveSettings(settings).then((next) => {
      setSettings(next);
      setSaved("Saved");
    });
  };

  if (section === "wake") {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        <Back onClick={() => setSection("menu")} />
        <WakeView status={wake} onStatus={onWake} />
      </div>
    );
  }

  if (section === "debug") {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        <Back onClick={() => setSection("menu")} />
        <DebugPanel voice={voice} onSignedOut={onSignedOut} onReplayTour={onReplayTour} />
      </div>
    );
  }

  if (section === "keys") {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        <Back onClick={() => setSection("menu")} />
        <KeysPage auth={auth} onAuth={onAuth} />
      </div>
    );
  }

  return (
    <PageChrome title="Settings" subtitle="Keys, model, wake word">
      <div className="mb-6 border-y border-border px-1 py-3">
        <div className="flex items-center justify-between gap-3">
          <div className="min-w-0">
            <p className="text-[15px] text-foreground">Quiet gap</p>
            <p className="mt-0.5 text-xs leading-5 text-muted-foreground">
              After this pause, the conversation ends and only the wake word is heard.
            </p>
          </div>
          <p className="shrink-0 text-sm tabular-nums text-foreground">{quiet}s</p>
        </div>
        <div className="mt-3 flex items-center gap-2">
          <button type="button" className="h-12 border border-border px-3 text-sm" onClick={() => setQuietGap(quiet - 5)}>
            −5
          </button>
          <input
            type="range"
            min={5}
            max={120}
            step={5}
            value={quiet}
            aria-label="Quiet gap in seconds"
            className="w-full accent-white"
            onChange={(e) => setQuietGap(Number(e.target.value))}
          />
          <button type="button" className="h-12 border border-border px-3 text-sm" onClick={() => setQuietGap(quiet + 5)}>
            +5
          </button>
        </div>
      </div>
      {settings && (
        <div className="mb-6 space-y-4">
          <Field label="Voice">
            <Input value={settings.voice} onChange={(e) => setSettings({ ...settings, voice: e.target.value })} />
          </Field>
          <Field label="Model">
            <Input value={settings.model} onChange={(e) => setSettings({ ...settings, model: e.target.value })} />
          </Field>
          <div className="flex items-center gap-3">
            <Button type="button" onClick={saveModel}>
              Save model
            </Button>
            {saved && <p className="text-sm text-muted-foreground">{saved}</p>}
          </div>
        </div>
      )}
      <div className="divide-y divide-border border-y border-border">
        <MenuRow
          icon={<KeyRound className="size-4" />}
          title="Voice keys"
          detail="Order, fallback, and on device"
          onClick={() => setSection("keys")}
        />
        <MenuRow
          icon={<Wand2 className="size-4" />}
          title="Wake word"
          detail={wake.trained ? "Hey Grok enrolled" : "Not enrolled yet"}
          onClick={() => setSection("wake")}
        />
        <MenuRow
          icon={<Bug className="size-4" />}
          title="Debug"
          detail="Logs and sign out"
          onClick={() => setSection("debug")}
        />
      </div>
    </PageChrome>
  );
}

function KeysPage({ auth, onAuth }: { auth: AuthStatus; onAuth: (auth: AuthStatus) => void }) {
  const [provider, setProvider] = useState<(typeof PROVIDERS)[number]["id"]>("xai");
  const [secret, setSecret] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const accounts = auth.accounts ?? [];

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const status = await api.addVoiceAccount(provider, provider === "local" ? "on-device" : secret);
      setSecret("");
      onAuth(status);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const move = async (id: string, up: boolean) => {
    onAuth(await api.moveVoiceAccount(id, up));
  };

  const remove = async (id: string) => {
    onAuth(await api.removeVoiceAccount(id));
  };

  return (
    <div className="space-y-4 pb-6">
      <h2 className="text-2xl font-medium tracking-tight">Voice keys</h2>
      <p className="text-sm leading-6 text-muted-foreground">First is tried first. If it fails, the next one is used.</p>
      <div className="grid grid-cols-2 gap-2">
        {PROVIDERS.map((item) => (
          <button
            key={item.id}
            type="button"
            onClick={() => setProvider(item.id)}
            className={cn(
              "h-12 border px-2 text-[11px] tracking-[0.08em] uppercase",
              item.id === provider ? "border-foreground text-foreground" : "border-border text-muted-foreground",
            )}
          >
            {item.label}
          </button>
        ))}
      </div>
      {provider === "local" ? (
        <p className="text-sm leading-6 text-muted-foreground">No key. The on-device voice is saved until that connection is ready.</p>
      ) : (
        <Field label="Key">
          <Input type="password" autoComplete="off" value={secret} onChange={(e) => setSecret(e.target.value)} />
        </Field>
      )}
      <Button disabled={busy || (provider !== "local" && secret.trim().length < 12)} onClick={() => void save()}>
        {provider === "local" ? "Use on device" : "Save this key"}
      </Button>
      {error && <p className="text-sm text-foreground">{error}</p>}
      <ul className="divide-y divide-border border-y border-border">
        {accounts.map((account, index) => (
          <li key={account.id} className="flex items-center gap-2 py-3 text-sm">
            <span className="w-5 tabular-nums text-muted-foreground">{index + 1}</span>
            <span className="min-w-0 flex-1 truncate">
              {account.provider} · {account.hint}
            </span>
            <button type="button" className="h-10 px-2 text-xs uppercase" onClick={() => void move(account.id, true)}>
              Up
            </button>
            <button type="button" className="h-10 px-2 text-xs uppercase" onClick={() => void move(account.id, false)}>
              Down
            </button>
            <button type="button" className="h-10 px-2 text-xs uppercase" onClick={() => void remove(account.id)}>
              Remove
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}

function Back({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      className="mb-3 shrink-0 px-1 text-left text-xs leading-5 tracking-[0.16em] text-muted-foreground uppercase"
      onClick={onClick}
    >
      ← Settings
    </button>
  );
}
function MenuRow({
  icon,
  title,
  detail,
  onClick,
}: {
  icon: React.ReactNode;
  title: string;
  detail: string;
  onClick: () => void;
}) {
  return (
    <RowButton onClick={onClick} className="items-center">
      <span className={cn("grid size-9 place-items-center border border-border text-foreground")}>{icon}</span>
      <div className="min-w-0 flex-1">
        <p className="text-[15px] text-foreground">{title}</p>
        <p className="mt-0.5 text-xs text-muted-foreground">{detail}</p>
      </div>
      <ChevronRight className="size-4 text-muted-foreground" />
    </RowButton>
  );
}
