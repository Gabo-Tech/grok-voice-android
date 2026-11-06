import { useEffect, useState } from "react";
import { Archive, Bug, ChevronRight, KeyRound, Users, Wand2 } from "lucide-react";
import { DebugPanel } from "@/components/DebugPanel";
import { LocalPacks } from "@/components/LocalPacks";
import { Field } from "@/components/Field";
import { PageChrome, RowButton } from "@/components/PageChrome";
import { PeoplePage } from "@/components/PeoplePage";
import { WakeView } from "@/components/WakeView";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { VoiceState } from "@/hooks/useVoice";
import { api, errorText, type AuthStatus, type BackupPreview, type Settings, type UsageStatus, type WakewordStatus } from "@/lib/api";
import { cn } from "@/lib/utils";

type Section = "menu" | "wake" | "debug" | "keys" | "people" | "backup";

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
  const [usage, setUsage] = useState<UsageStatus | null>(null);

  useEffect(() => {
    void api
      .getSettings()
      .then((s) => {
        setQuiet(s.idleTimeoutSecs);
        setSettings(s);
      })
      .catch(() => {});
    void api.usageStatus().then(setUsage).catch(() => {});
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

  const saveBudget = (next: Settings) => {
    const hard = next.dailyBudgetMinutes > 0 && next.dailyBudgetHardStop;
    const merged = { ...next, dailyBudgetHardStop: hard };
    setSettings(merged);
    void api.saveSettings(merged).then((savedSettings) => {
      setSettings(savedSettings);
      void api.usageStatus().then(setUsage).catch(() => {});
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

  if (section === "people") {
    return <PeoplePage onBack={() => setSection("menu")} />;
  }

  if (section === "backup") {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        <Back onClick={() => setSection("menu")} />
        <BackupPage />
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
          <div className="border border-border px-3 py-3">
            <p className="text-[15px] text-foreground">Daily voice budget</p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              Off by default. Near the minutes you pick, one short cue appears and can be spoken. The call keeps going unless you also turn on a hard stop.
            </p>
            <p className="mt-2 text-sm text-foreground">
              {usage ? `${usage.minutes} min across ${usage.sessions} ${usage.sessions === 1 ? "session" : "sessions"} today` : "Today's time is counted on this phone."}
            </p>
            <div className="mt-3 grid grid-cols-4 gap-2">
              {[
                { minutes: 0, label: "Off" },
                { minutes: 15, label: "15" },
                { minutes: 30, label: "30" },
                { minutes: 60, label: "60" },
              ].map((choice) => (
                <button
                  key={choice.label}
                  type="button"
                  onClick={() => saveBudget({ ...settings, dailyBudgetMinutes: choice.minutes })}
                  className={cn(
                    "h-12 border text-xs tracking-[0.08em] uppercase",
                    settings.dailyBudgetMinutes === choice.minutes ? "border-foreground text-foreground" : "border-border text-muted-foreground",
                  )}
                >
                  {choice.label}
                </button>
              ))}
            </div>
            <button
              type="button"
              disabled={!settings.dailyBudgetMinutes}
              onClick={() => saveBudget({ ...settings, dailyBudgetHardStop: !settings.dailyBudgetHardStop })}
              className={cn(
                "mt-2 flex h-12 w-full items-center justify-between border px-3 text-left text-sm",
                settings.dailyBudgetHardStop ? "border-foreground text-foreground" : "border-border text-muted-foreground",
                !settings.dailyBudgetMinutes && "opacity-50",
              )}
            >
              <span>Hard stop</span>
              <span className="text-xs tracking-[0.12em] uppercase">{settings.dailyBudgetHardStop ? "On" : "Off"}</span>
            </button>
          </div>
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
          icon={<Users className="size-4" />}
          title="People"
          detail="Nicknames and how to reach them"
          onClick={() => setSection("people")}
        />
        <MenuRow
          icon={<KeyRound className="size-4" />}
          title="Voice keys"
          detail="Order, fallback, and on device"
          onClick={() => setSection("keys")}
        />
        <MenuRow
          icon={<Archive className="size-4" />}
          title="Backup"
          detail="Encrypted notes and People"
          onClick={() => setSection("backup")}
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

function BackupPage() {
  const [passphrase, setPassphrase] = useState("");
  const [again, setAgain] = useState("");
  const [includeKeys, setIncludeKeys] = useState(false);
  const [fileB64, setFileB64] = useState("");
  const [fileName, setFileName] = useState("");
  const [preview, setPreview] = useState<BackupPreview | null>(null);
  const [restoreKeys, setRestoreKeys] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);

  const exportFile = async () => {
    if (passphrase.length < 8) {
      setError("Use a passphrase of at least 8 characters.");
      return;
    }
    if (passphrase !== again) {
      setError("Type the passphrase the same way twice.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const file = await api.exportBackup(passphrase, includeKeys);
      setNotice(file.shared ? "Share sheet opened. The file stays on this phone." : "Backup ready.");
      if (!file.shared) downloadBase64(file.fileName, file.dataB64);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const readFile = async (file: File | undefined) => {
    if (!file) return;
    const bytes = new Uint8Array(await file.arrayBuffer());
    let binary = "";
    for (const byte of bytes) binary += String.fromCharCode(byte);
    setFileB64(btoa(binary));
    setFileName(file.name);
    setPreview(null);
    setRestoreKeys(false);
    setError(null);
    setNotice("");
  };

  const review = async () => {
    setBusy(true);
    setError(null);
    try {
      const next = await api.previewBackup(fileB64, passphrase);
      setPreview(next);
      setRestoreKeys(false);
      setNotice("");
    } catch (e) {
      setPreview(null);
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const restore = async () => {
    setBusy(true);
    setError(null);
    try {
      const next = await api.restoreBackup(fileB64, passphrase, true, restoreKeys);
      setPreview(next);
      setNotice(`Replaced ${next.notes} notes and ${next.people} people.`);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-4 pb-6">
      <h2 className="text-2xl font-medium tracking-tight">Backup</h2>
      <p className="text-sm leading-6 text-muted-foreground">
        An encrypted file of notes and People, including birthdays. Voice keys stay in the vault unless you include them here. Nothing is uploaded.
      </p>
      <Field label="Passphrase">
        <Input type="password" autoComplete="new-password" value={passphrase} onChange={(e) => setPassphrase(e.target.value)} />
      </Field>
      <Field label="Passphrase again">
        <Input type="password" autoComplete="new-password" value={again} onChange={(e) => setAgain(e.target.value)} />
      </Field>
      <button
        type="button"
        onClick={() => setIncludeKeys((on) => !on)}
        className={cn(
          "flex h-12 w-full items-center justify-between border px-3 text-left text-sm",
          includeKeys ? "border-foreground text-foreground" : "border-border text-muted-foreground",
        )}
      >
        <span>Include voice keys</span>
        <span className="text-xs tracking-[0.12em] uppercase">{includeKeys ? "On" : "Off"}</span>
      </button>
      <Button disabled={busy} onClick={() => void exportFile()}>
        Export backup
      </Button>
      <div className="border-t border-border pt-4">
        <p className="text-[11px] tracking-[0.16em] text-muted-foreground uppercase">Restore</p>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          Replace notes and People on this phone. Reminders that are not tied to a person stay. Confirm after you review the file.
        </p>
        <label className="mt-3 flex h-12 cursor-pointer items-center border border-border px-3 text-sm">
          <span className="truncate">{fileName || "Choose a .gvb file"}</span>
          <input
            type="file"
            accept=".gvb,application/octet-stream"
            className="sr-only"
            onChange={(e) => void readFile(e.target.files?.[0])}
          />
        </label>
        <div className="mt-3 flex flex-wrap gap-2">
          <Button variant="outline" disabled={busy || !fileB64 || passphrase.length < 8} onClick={() => void review()}>
            Review
          </Button>
          <Button disabled={busy || !preview} onClick={() => void restore()}>
            Replace notes and people
          </Button>
        </div>
        {preview && (
          <div className="mt-3 space-y-2 text-sm text-foreground">
            <p>
              {preview.notes} notes · {preview.people} people · {preview.reminders} dates
              {preview.keys ? ` · ${preview.keys} keys in the file` : ""}
            </p>
            {preview.keys > 0 && (
              <button
                type="button"
                onClick={() => setRestoreKeys((on) => !on)}
                className={cn(
                  "flex h-12 w-full items-center justify-between border px-3 text-left text-sm",
                  restoreKeys ? "border-foreground text-foreground" : "border-border text-muted-foreground",
                )}
              >
                <span>Also replace voice keys</span>
                <span className="text-xs tracking-[0.12em] uppercase">{restoreKeys ? "On" : "Off"}</span>
              </button>
            )}
          </div>
        )}
      </div>
      {error && <p className="text-sm text-foreground">{error}</p>}
      {notice && <p className="text-sm text-muted-foreground">{notice}</p>}
    </div>
  );
}

function downloadBase64(name: string, data: string) {
  const binary = atob(data);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  const url = URL.createObjectURL(new Blob([bytes]));
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  URL.revokeObjectURL(url);
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
        <p className="text-sm leading-6 text-muted-foreground">
          No key. Talk uses this phone when On device is first and the packs below are ready. If a pack is missing or the model cannot load, the next key is used.
        </p>
      ) : (
        <Field label="Key">
          <Input type="password" autoComplete="off" value={secret} onChange={(e) => setSecret(e.target.value)} />
        </Field>
      )}
      <Button disabled={busy || (provider !== "local" && secret.trim().length < 12)} onClick={() => void save()}>
        {provider === "local" ? "Use on device" : "Save this key"}
      </Button>
      {error && <p className="text-sm text-foreground">{error}</p>}
      <LocalPacks />
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
