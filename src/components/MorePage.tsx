import { useEffect, useState } from "react";
import { Archive, Bell, Brain, Bug, ChevronRight, KeyRound, Users, Wand2 } from "lucide-react";
import { DebugPanel } from "@/components/DebugPanel";
import { LocalPacks } from "@/components/LocalPacks";
import { Field } from "@/components/Field";
import { PageChrome, RowButton } from "@/components/PageChrome";
import { PeoplePage } from "@/components/PeoplePage";
import { WakeView } from "@/components/WakeView";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { VoiceState } from "@/hooks/useVoice";
import { api, errorText, type AccessStatus, type AuthStatus, type BackupPreview, type LocalMemoryBundle, type Settings, type UsageStatus, type WakewordStatus } from "@/lib/api";
import { setAppLocale, useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

type Section = "menu" | "wake" | "debug" | "keys" | "people" | "backup" | "memory" | "access";

const PROVIDERS = [
  { id: "xai", label: "xAI" },
  { id: "openai", label: "OpenAI" },
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
  const t = useT();
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

  if (section === "memory") {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        <Back onClick={() => setSection("menu")} />
        <MemorySection />
      </div>
    );
  }

  if (section === "access") {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        <Back onClick={() => setSection("menu")} />
        <AccessSection />
      </div>
    );
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
    <PageChrome title={t("Settings")} subtitle={t("Voice keys")}>
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
            <p className="text-[15px] text-foreground">{t("Daily voice budget")}</p>
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
              <span>{t("Hard stop")}</span>
              <span className="text-xs tracking-[0.12em] uppercase">{settings.dailyBudgetHardStop ? "On" : "Off"}</span>
            </button>
          </div>
          {usage && (
            <p className="text-sm text-foreground">
              xAI {usage.xaiMinutes} · OpenAI {usage.openaiMinutes} · {t("On device")} {usage.localMinutes}
              {usage.monthMinutes ? ` · ${usage.month} ${usage.monthMinutes}` : ""}
            </p>
          )}
          <div className="border border-border px-3 py-3">
            <p className="text-[15px] text-foreground">{t("Monthly voice budget")}</p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              Counts xAI, OpenAI, and on-device time together, so a fallback key is not silent.
            </p>
            <div className="mt-3 grid grid-cols-4 gap-2">
              {[
                { minutes: 0, label: t("Off") },
                { minutes: 60, label: "60" },
                { minutes: 300, label: "300" },
                { minutes: 1000, label: "1000" },
              ].map((choice) => (
                <button
                  key={choice.label}
                  type="button"
                  onClick={() => saveBudget({ ...settings, monthlyBudgetMinutes: choice.minutes })}
                  className={cn(
                    "h-12 border text-xs tracking-[0.08em] uppercase",
                    settings.monthlyBudgetMinutes === choice.minutes ? "border-foreground text-foreground" : "border-border text-muted-foreground",
                  )}
                >
                  {choice.label}
                </button>
              ))}
            </div>
            <button
              type="button"
              disabled={!settings.monthlyBudgetMinutes}
              onClick={() => saveBudget({ ...settings, monthlyBudgetHardStop: !settings.monthlyBudgetHardStop })}
              className={cn(
                "mt-2 flex h-12 w-full items-center justify-between border px-3 text-left text-sm",
                settings.monthlyBudgetHardStop ? "border-foreground text-foreground" : "border-border text-muted-foreground",
                !settings.monthlyBudgetMinutes && "opacity-50",
              )}
            >
              <span>{t("Hard stop")}</span>
              <span className="text-xs tracking-[0.12em] uppercase">{settings.monthlyBudgetHardStop ? "On" : t("Off")}</span>
            </button>
          </div>
          <div>
            <p className="mb-2 text-[11px] tracking-[0.16em] text-muted-foreground uppercase">{t("Language")}</p>
            <div className="grid grid-cols-2 gap-2">
              {[
                { id: "", label: t("Follow the phone") },
                { id: "en", label: t("English") },
                { id: "es", label: t("Spanish") },
                { id: "de", label: t("German") },
                { id: "pt", label: t("Portuguese") },
                { id: "ca-ES-valencia", label: t("Valencian") },
              ].map((choice) => (
                <button
                  key={choice.id || "system"}
                  type="button"
                  onClick={() => {
                    setAppLocale(choice.id);
                    saveBudget({ ...settings, locale: choice.id });
                  }}
                  className={cn(
                    "h-12 border px-2 text-xs",
                    (settings.locale || "") === choice.id ? "border-foreground text-foreground" : "border-border text-muted-foreground",
                  )}
                >
                  {choice.label}
                </button>
              ))}
            </div>
          </div>
          <div>
            <p className="mb-2 text-[11px] tracking-[0.16em] text-muted-foreground uppercase">{t("Local speech")}</p>
            <div className="grid grid-cols-2 gap-2">
              {[
                { id: "en", label: t("English") },
                { id: "de", label: t("German") },
              ].map((choice) => (
                <button
                  key={choice.id}
                  type="button"
                  onClick={() => saveBudget({ ...settings, localSpeech: choice.id })}
                  className={cn(
                    "h-12 border px-2 text-xs",
                    (settings.localSpeech || "en") === choice.id ? "border-foreground text-foreground" : "border-border text-muted-foreground",
                  )}
                >
                  {choice.label}
                </button>
              ))}
            </div>
            <p className="mt-2 text-xs leading-5 text-muted-foreground">
              English uses Moonshine Tiny. German uses a separate download. Moonshine does not understand German.
            </p>
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
          title={t("People")}
          detail="Nicknames and how to reach them"
          onClick={() => setSection("people")}
        />
        <MenuRow
          icon={<KeyRound className="size-4" />}
          title={t("Voice keys")}
          detail="Order, fallback, and on device"
          onClick={() => setSection("keys")}
        />
        <MenuRow
          icon={<Archive className="size-4" />}
          title={t("Backup")}
          detail="Encrypted notes, People, and memory"
          onClick={() => setSection("backup")}
        />
        <MenuRow
          icon={<Brain className="size-4" />}
          title={t("Memory")}
          detail="Pinned facts for on-device Talk"
          onClick={() => setSection("memory")}
        />
        <MenuRow
          icon={<Bell className="size-4" />}
          title={t("Phone access")}
          detail="Notifications, Do Not Disturb, history"
          onClick={() => setSection("access")}
        />
        <MenuRow
          icon={<Wand2 className="size-4" />}
          title={t("Wake word")}
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

function MemorySection() {
  const t = useT();
  const [bundle, setBundle] = useState<LocalMemoryBundle | null>(null);
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [confirmWipe, setConfirmWipe] = useState(false);

  useEffect(() => {
    void api.localMemory().then(setBundle).catch((e) => setError(errorText(e)));
  }, []);

  const pin = async () => {
    const next = text.trim();
    if (!next) return;
    try {
      setBundle(await api.pinLocalFact(next));
      setText("");
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  };

  return (
    <div className="space-y-4 pb-6">
      <h2 className="text-2xl font-medium tracking-tight">{t("Memory")}</h2>
      <p className="text-sm leading-6 text-muted-foreground">
        On-device Talk reads these pinned facts and short notes from recent local sessions. Cloud Talk does not. Wiping this does not delete notes, People, or voice keys.
      </p>
      <Field label={t("Pin fact")}>
        <Input value={text} onChange={(e) => setText(e.target.value)} placeholder="I live in Zurich" />
      </Field>
      <Button type="button" onClick={() => void pin()}>
        {t("Pin fact")}
      </Button>
      <ul className="divide-y divide-border border-y border-border">
        {(bundle?.facts ?? []).map((fact) => (
          <li key={fact.id} className="flex items-center gap-2 py-3 text-sm">
            <span className="min-w-0 flex-1">{fact.text}</span>
            <button
              type="button"
              className="h-10 px-2 text-xs uppercase"
              onClick={() => void api.forgetLocalFact(fact.id).then(setBundle).catch((e) => setError(errorText(e)))}
            >
              {t("Forget")}
            </button>
          </li>
        ))}
      </ul>
      <div className="space-y-2">
        {(bundle?.sessions ?? []).map((note) => (
          <p key={note.id} className="text-sm leading-6 text-muted-foreground">
            {note.text}
          </p>
        ))}
      </div>
      <Button
        type="button"
        variant="outline"
        onClick={() => {
          if (!confirmWipe) {
            setConfirmWipe(true);
            return;
          }
          void api.wipeLocalMemory().then((next) => {
            setBundle(next);
            setConfirmWipe(false);
          });
        }}
      >
        {confirmWipe ? t("Wipe memory") : t("Wipe memory")}
      </Button>
      {confirmWipe && <p className="text-xs leading-5 text-muted-foreground">Tap again. Notes, People, and voice keys stay.</p>}
      {error && <p className="text-sm text-foreground">{error}</p>}
    </div>
  );
}

function AccessSection() {
  const t = useT();
  const [status, setStatus] = useState<AccessStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = () => {
    void api.accessStatus().then(setStatus).catch((e) => setError(errorText(e)));
  };

  useEffect(() => {
    refresh();
  }, []);

  return (
    <div className="space-y-4 pb-6">
      <h2 className="text-2xl font-medium tracking-tight">{t("Phone access")}</h2>
      <p className="text-sm leading-6 text-muted-foreground">
        These are asked only when you turn them on. Notification access and Do Not Disturb open system settings. Call and text history is a separate permission and is not read until you allow it.
      </p>
      <button type="button" className="flex h-12 w-full items-center justify-between border border-border px-3 text-left text-sm" onClick={() => void api.openAccessSettings("notifications").then(refresh)}>
        <span>{t("Notification access")}</span>
        <span className="text-xs uppercase">{status?.notificationListener || ""}</span>
      </button>
      <button type="button" className="flex h-12 w-full items-center justify-between border border-border px-3 text-left text-sm" onClick={() => void api.openAccessSettings("dnd").then(refresh)}>
        <span>{t("Do Not Disturb")}</span>
        <span className="text-xs uppercase">{status?.dnd || ""}</span>
      </button>
      <button
        type="button"
        className="flex h-12 w-full items-center justify-between border border-border px-3 text-left text-sm"
        onClick={() => void api.requestHistoryAccess().then(refresh).catch((e) => setError(errorText(e)))}
      >
        <span>{t("Call and text history")}</span>
        <span className="text-xs uppercase">{status ? `${status.callLog} / ${status.sms}` : ""}</span>
      </button>
      {error && <p className="text-sm text-foreground">{error}</p>}
    </div>
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
        An encrypted file of notes, People, and the small on-device memory. Older files have no memory section, so restoring one leaves the memory on this phone. Voice keys stay in the vault unless you include them here. Nothing is uploaded.
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
              {preview.hasMemory ? ` · ${preview.facts} facts` : " · no memory section"}
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

function providerName(id: string) {
  return PROVIDERS.find((item) => item.id === id)?.label ?? id;
}

function KeysPage({ auth, onAuth }: { auth: AuthStatus; onAuth: (auth: AuthStatus) => void }) {
  const [provider, setProvider] = useState<(typeof PROVIDERS)[number]["id"]>("xai");
  const [secret, setSecret] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [settings, setSettings] = useState<Settings | null>(null);
  const accounts = auth.accounts ?? [];

  useEffect(() => {
    void api.getSettings().then(setSettings).catch(() => {});
  }, []);

  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const value = provider === "local" ? "on-device" : secret;
      const status = editing
        ? await api.updateVoiceAccount(editing, provider, value)
        : await api.addVoiceAccount(provider, value);
      setSecret("");
      setEditing(null);
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
    if (editing === id) setEditing(null);
    onAuth(await api.removeVoiceAccount(id));
  };

  const edit = (id: string, current: string) => {
    setEditing(id);
    setSecret("");
    setError(null);
    if (PROVIDERS.some((item) => item.id === current)) {
      setProvider(current as (typeof PROVIDERS)[number]["id"]);
    }
  };

  const togglePrefer = () => {
    if (!settings) return;
    const next = { ...settings, preferLocalOnWake: !settings.preferLocalOnWake };
    setSettings(next);
    void api.saveSettings(next).then(setSettings).catch((err) => setError(errorText(err)));
  };

  const editingAccount = accounts.find((account) => account.id === editing);
  const secretReady =
    provider === "local" ||
    secret.trim().length >= 12 ||
    (editingAccount !== undefined && editingAccount.provider === provider && secret.trim().length === 0);

  return (
    <div className="space-y-4 pb-6">
      <h2 className="text-2xl font-medium tracking-tight">Voice keys</h2>
      <p className="text-sm leading-6 text-muted-foreground">
        First is tried first. If that key fails to sign in, reach the network, hits a rate limit, or the model is unavailable, the next one is used. On device can sit anywhere in the list.
      </p>
      <div className="grid grid-cols-3 gap-2">
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
          No key. Talk uses this phone when the packs below are ready. If a pack is missing or the model cannot load, the next key is used.
        </p>
      ) : (
        <Field label={editing ? "New key" : "Key"}>
          <Input
            type="password"
            autoComplete="off"
            placeholder={editing ? "Leave blank to keep the saved key" : ""}
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
          />
        </Field>
      )}
      <div className="flex gap-2">
        <Button disabled={busy || !secretReady} onClick={() => void save()}>
          {provider === "local" ? (editing ? "Update on device" : "Use on device") : editing ? "Update this key" : "Save this key"}
        </Button>
        {editing && (
          <Button
            type="button"
            variant="outline"
            onClick={() => {
              setEditing(null);
              setSecret("");
            }}
          >
            Cancel
          </Button>
        )}
      </div>
      <button
        type="button"
        onClick={togglePrefer}
        disabled={!settings}
        className={cn(
          "flex h-12 w-full items-center justify-between border px-3 text-left text-sm",
          settings?.preferLocalOnWake ? "border-foreground text-foreground" : "border-border text-muted-foreground",
        )}
      >
        <span>Prefer local when packs ready</span>
        <span className="text-xs tracking-[0.12em] uppercase">{settings?.preferLocalOnWake ? "On" : "Off"}</span>
      </button>
      <p className="text-sm leading-6 text-muted-foreground">
        Off follows the list for every session. On uses On device first for the wake word, when the packs are downloaded and On device is in this list. Talk still follows the order above.
      </p>
      {error && <p className="text-sm text-foreground">{error}</p>}
      <LocalPacks />
      <ul className="divide-y divide-border border-y border-border">
        {accounts.map((account, index) => (
          <li key={account.id} className="flex items-center gap-2 py-3 text-sm">
            <span className="w-5 tabular-nums text-muted-foreground">{index + 1}</span>
            <span className="min-w-0 flex-1 truncate">
              {providerName(account.provider)} · {account.hint}
            </span>
            <button type="button" className="h-10 px-2 text-xs uppercase" onClick={() => edit(account.id, account.provider)}>
              Edit
            </button>
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
