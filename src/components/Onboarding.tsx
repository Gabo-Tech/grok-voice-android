import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  AudioLines,
  Check,
  ChevronLeft,
  ChevronRight,
  ExternalLink,
  Headphones,
  KeyRound,
  Loader2,
  Mic,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { LocalPacks } from "@/components/LocalPacks";
import { WakeView } from "@/components/WakeView";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api, errorText, type AuthStatus, type PermissionStatus, type WakewordStatus } from "@/lib/api";
import { VoiceOrder } from "@/components/VoiceOrder";
import { fill, useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

const PROVIDERS = [
  {
    id: "xai",
    label: "xAI",
    blurb: "Grok's voice. Needs a key from the xAI console.",
    placeholder: "xai-…",
    url: "https://console.x.ai/team/default/api-keys",
    link: "Open xAI console",
  },
  {
    id: "openai",
    label: "OpenAI",
    blurb: "Another cloud voice. Needs an OpenAI key.",
    placeholder: "sk-…",
    url: "https://platform.openai.com/api-keys",
    link: "Open OpenAI keys",
  },
  {
    id: "local",
    label: "On device",
    blurb: "No key. Download packs, then this phone can answer.",
    placeholder: "",
    url: "",
    link: "",
  },
] as const;

const STEPS = [
  { id: "hello", label: "Hello" },
  { id: "key", label: "Key" },
  { id: "mic", label: "Mic" },
  { id: "wake", label: "Wake" },
  { id: "ready", label: "Ready" },
] as const;

export function Onboarding({
  auth,
  wake,
  preview,
  onAuth,
  onWake,
  onFinished,
}: {
  auth: AuthStatus;
  wake: WakewordStatus;
  preview?: boolean;
  onAuth: (s: AuthStatus) => void;
  onWake: (s: WakewordStatus) => void;
  onFinished: () => void;
}) {
  const [step, setStep] = useState(0);
  const [provider, setProvider] = useState<(typeof PROVIDERS)[number]["id"]>("xai");
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [perms, setPerms] = useState<PermissionStatus | null>(null);
  const [done, setDone] = useState(false);
  const t = useT();

  const id = STEPS[step].id;
  const micGranted = perms?.microphone === "granted";

  useEffect(() => {
    if (id !== "mic" || preview) return;
    let alive = true;
    void api.permissionsStatus().then((p) => alive && setPerms(p)).catch((e) => alive && setError(errorText(e)));
    return () => {
      alive = false;
    };
  }, [id, preview]);

  const back = () => {
    setError(null);
    setStep((n) => Math.max(0, n - 1));
  };

  const finish = async () => {
    setBusy(true);
    setError(null);
    try {
      if (!preview) {
        const settings = await api.getSettings();
        await api.saveSettings({ ...settings, onboarded: true });
      }
      setDone(true);
      onFinished();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const next = () => {
    setError(null);
    if (step === STEPS.length - 1) {
      void finish();
      return;
    }
    setStep((n) => n + 1);
  };

  const saveKey = async () => {
    setBusy(true);
    setError(null);
    try {
      if (preview) {
        if (provider === "local" && auth.accounts.some((account) => account.provider === "local")) {
          setError("On device is already in the list.");
          return;
        }
        onAuth({
          hasKey: true,
          keyHint: "xai-…demo",
          securityLevel: "software",
          accounts: [
            ...auth.accounts,
            {
              id: `demo-${provider}-${auth.accounts.length}`,
              provider,
              hint: provider === "local" ? "on this phone" : "demo…key",
            },
          ],
        });
        return;
      }
      const status = await api.addVoiceAccount(provider, provider === "local" ? "on-device" : key);
      setKey("");
      onAuth(status);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const allowMic = async () => {
    setBusy(true);
    setError(null);
    try {
      if (preview) {
        setPerms({ microphone: "granted", notifications: "granted" });
        return;
      }
      setPerms(await api.requestPermissions());
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const canNext =
    (id === "hello" || id === "wake" || id === "ready" || (id === "key" && auth.hasKey) || (id === "mic" && micGranted)) &&
    !busy;

  return (
    <div className="flex h-full flex-col px-5 py-6">
      <header className="mb-6 text-center">
        <p className="text-[11px] tracking-[0.35em] text-primary uppercase">{t("Setup")}</p>
        <h1 className="mt-1 text-lg font-semibold tracking-[0.22em]">GROK//VOICE</h1>
      </header>

      <ol className="mb-6 flex items-start gap-1">
        {STEPS.map((s, i) => {
          const state = i < step ? "done" : i === step ? "now" : "later";
          return (
            <li key={s.id} className="flex min-w-0 flex-1 flex-col items-center gap-1.5">
              <span
                className={cn(
                  "grid size-7 place-items-center rounded-full border text-xs",
                  state === "done" && "border-primary bg-primary text-primary-foreground",
                  state === "now" && "border-primary text-primary",
                  state === "later" && "border-border text-muted-foreground",
                )}
              >
                {state === "done" ? <Check className="size-3.5" /> : i + 1}
              </span>
              <span className={cn("text-xs", state === "now" ? "text-foreground" : "text-muted-foreground")}>{t(s.label)}</span>
            </li>
          );
        })}
      </ol>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {id === "hello" && <HelloStep />}
        {id === "key" && (
          <KeyStep
            auth={auth}
            provider={provider}
            value={key}
            busy={busy}
            onProvider={setProvider}
            onChange={setKey}
            onOpen={(url) => {
              if (preview) {
                window.open(url, "_blank", "noopener");
                return;
              }
              void openUrl(url);
            }}
            onSave={() => void saveKey()}
            onRemove={(id) => {
              if (preview) {
                const accounts = auth.accounts.filter((account) => account.id !== id);
                onAuth({ ...auth, hasKey: accounts.length > 0, accounts });
                return;
              }
              void api.removeVoiceAccount(id).then(onAuth).catch((e) => setError(errorText(e)));
            }}
            onReorder={async (ids) => {
              if (preview) {
                const byId = new Map(auth.accounts.map((account) => [account.id, account]));
                const accounts = ids.flatMap((id) => {
                  const account = byId.get(id);
                  return account ? [account] : [];
                });
                onAuth({ ...auth, accounts });
                return;
              }
              try {
                onAuth(await api.reorderVoiceAccounts(ids));
              } catch (err) {
                setError(errorText(err));
                throw err;
              }
            }}
          />
        )}
        {id === "mic" && <MicStep perms={perms} busy={busy} onAllow={() => void allowMic()} />}
        {id === "wake" && <WakeView status={wake} onStatus={onWake} />}
        {id === "ready" && <ReadyStep wakeTrained={wake.trained} />}
      </div>

      {error && (
        <p role="alert" className="mt-3 text-sm leading-6 text-foreground">
          {t(error)}
        </p>
      )}
      {!canNext && id === "key" && (
        <p className="mt-3 text-sm leading-6 text-muted-foreground">{t("Save a voice to continue. On device counts after you tap Use on device.")}</p>
      )}
      {!canNext && id === "mic" && <p className="mt-3 text-sm leading-6 text-muted-foreground">{t("Allow the microphone to continue.")}</p>}
      {done && preview && (
        <p className="mt-3 text-center text-sm text-primary">{t("Preview finished. In the app this opens the assistant.")}</p>
      )}

      <footer className="mt-4 flex gap-2 pt-2">
        <Button variant="outline" disabled={step === 0 || busy} onClick={back}>
          <ChevronLeft /> {t("Back")}
        </Button>
        <Button className="flex-1" disabled={!canNext} onClick={next}>
          {busy && id === "ready" ? <Loader2 className="animate-spin" /> : <ChevronRight />}
          {id === "wake" && !wake.trained ? t("Skip for now") : id === "ready" ? t("Start talking") : t("Next")}
        </Button>
      </footer>
    </div>
  );
}

function HelloStep() {
  const t = useT();
  return (
    <div className="space-y-5">
      <div
        className="mark pointer-events-none mx-auto"
        style={{ width: "7rem", height: "7rem", ["--ink" as string]: "#f2f2f2" }}
        aria-hidden
      >
        <Sparkles className="size-8 text-primary" />
      </div>
      <div className="text-center">
        <h2 className="text-xl font-medium">{t("A voice assistant that stays on your phone")}</h2>
        <p className="mt-2 text-sm text-muted-foreground">{t("Five short steps. About two minutes.")}</p>
      </div>
      <ul className="space-y-3">
        <Point n="1" title={t("Choose xAI, OpenAI, or On device")}>
          {t("xAI and OpenAI need a key. On device needs a download. The first one in the list is tried first.")}
        </Point>
        <Point n="2" title={t("Allow the microphone")}>
          {t("Needed to hear you, including when the screen is off.")}
        </Point>
        <Point n="3" title={t("Teach “Hey Grok”")}>
          {t("Three short recordings. They never leave the phone. You can skip and tap instead.")}
        </Point>
      </ul>
    </div>
  );
}

function Point({ n, title, children }: { n: string; title: string; children: string }) {
  return (
    <li className="flex gap-3 rounded-lg border border-primary/15 bg-card/60 px-3 py-2.5">
      <span className="grid size-6 shrink-0 place-items-center rounded-full bg-primary/15 text-xs text-primary">{n}</span>
      <span>
        <span className="block text-sm font-medium">{title}</span>
        <span className="block text-xs leading-relaxed text-muted-foreground">{children}</span>
      </span>
    </li>
  );
}

function KeyStep({
  auth,
  provider,
  value,
  busy,
  onProvider,
  onChange,
  onOpen,
  onSave,
  onRemove,
  onReorder,
}: {
  auth: AuthStatus;
  provider: (typeof PROVIDERS)[number]["id"];
  value: string;
  busy: boolean;
  onProvider: (id: (typeof PROVIDERS)[number]["id"]) => void;
  onChange: (v: string) => void;
  onOpen: (url: string) => void;
  onSave: () => void;
  onRemove: (id: string) => void;
  onReorder: (ids: string[]) => Promise<void>;
}) {
  const t = useT();
  const chosen = PROVIDERS.find((p) => p.id === provider) ?? PROVIDERS[0];
  const accounts = auth.accounts ?? [];
  return (
    <Card className="border-primary/20">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <KeyRound className="size-4 text-primary" /> {t("Add a voice key")}
        </CardTitle>
        <CardDescription>{t("Add xAI, OpenAI, or On device.")}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="space-y-2">
          {PROVIDERS.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => onProvider(item.id)}
              className={cn(
                "flex min-h-12 w-full flex-col items-start justify-center border px-3 py-2 text-left",
                item.id === provider ? "border-foreground text-foreground" : "border-border text-muted-foreground",
              )}
            >
              <span className="text-base text-foreground">{t(item.label)}</span>
              <span className="text-sm leading-5">{t(item.blurb)}</span>
            </button>
          ))}
        </div>
        {chosen.id === "local" ? (
          <div className="space-y-3">
            <ol className="list-decimal space-y-2 pl-5 text-sm leading-6 text-muted-foreground">
              <li>{t("Tap Use on device. That adds it to the list. No key.")}</li>
              <li>{t("Leave it first if the phone should answer before a cloud key.")}</li>
              <li>{t("Download the packs below. Until they finish, Talk uses the next key.")}</li>
            </ol>
          </div>
        ) : (
          <>
            <Button variant="outline" className="w-full" onClick={() => onOpen(chosen.url)}>
              <ExternalLink /> {t(chosen.link)}
            </Button>
            <div className="space-y-2">
              <Label htmlFor="voice-key">{fill("Paste the {name} key", { name: chosen.label })}</Label>
              <Input
                id="voice-key"
                type="password"
                autoComplete="off"
                autoCapitalize="off"
                spellCheck={false}
                placeholder={chosen.placeholder}
                value={value}
                onChange={(e) => onChange(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && value && !busy && onSave()}
              />
            </div>
          </>
        )}
        <Button className="w-full" disabled={busy || (chosen.id !== "local" && value.trim().length < 12)} onClick={onSave}>
          {busy ? <Loader2 className="animate-spin" /> : <ShieldCheck />} {chosen.id === "local" ? t("Use on device") : t("Save this key")}
        </Button>
        {accounts.length > 0 && <VoiceOrder accounts={accounts} onReorder={onReorder} onRemove={onRemove} />}
        {accounts.length > 0 && (
          <p className="text-sm leading-6 text-muted-foreground">{t("Add another voice if you want a fallback, then tap Next.")}</p>
        )}
        <p className="text-sm leading-6 text-muted-foreground">
          {t("Each key stays on this phone. xAI is checked before it is stored. If the first voice fails, the next one is tried.")}
        </p>
        {chosen.id === "local" && <LocalPacks />}
      </CardContent>
    </Card>
  );
}

function MicStep({
  perms,
  busy,
  onAllow,
}: {
  perms: PermissionStatus | null;
  busy: boolean;
  onAllow: () => void;
}) {
  const mic = perms?.microphone;
  const notes = perms?.notifications;
  const t = useT();
  return (
    <Card className="border-primary/20">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Mic className="size-4 text-primary" /> {t("Allow the microphone")}
        </CardTitle>
        <CardDescription>{t("Android will show a system prompt. Choose Allow.")}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <p className="text-sm text-muted-foreground">
          {t("The microphone hears “Hey Grok” and your question. A quiet notification stays up so Android does not switch the microphone off when the screen locks.")}
        </p>
        <Button className="w-full" disabled={busy || mic === "granted"} onClick={onAllow}>
          {busy ? <Loader2 className="animate-spin" /> : <Mic />}
          {mic === "granted" ? t("Microphone allowed") : t("Allow microphone")}
        </Button>
        <StatusLine label={t("Microphone")} value={mic} />
        <StatusLine label={t("Notification")} value={notes} />
        {mic === "denied" && (
          <p className="text-sm text-destructive">
            {t("Microphone is blocked. Open Android Settings → Apps → Grok Voice → Permissions, allow Microphone, then come back and tap the button again.")}
          </p>
        )}
        {mic === "granted" && notes !== "granted" && (
          <p className="text-xs text-muted-foreground">
            {t("Notifications are off. Listening still works while the app is open. Allow notifications later if you want “Hey Grok” with the screen locked.")}
          </p>
        )}
      </CardContent>
    </Card>
  );
}

function StatusLine({ label, value }: { label: string; value?: string }) {
  const t = useT();
  return (
    <div className="flex items-center justify-between text-xs">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-foreground">{value === "granted" ? t("Allowed") : value === "denied" ? t("Blocked") : t("Not asked yet")}</span>
    </div>
  );
}

function ReadyStep({ wakeTrained }: { wakeTrained: boolean }) {
  const t = useT();
  return (
    <div className="space-y-3">
      <Card className="border-primary/20">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <AudioLines className="size-4 text-primary" /> {t("How to talk")}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <ol className="space-y-3 text-sm">
            <li>
              <span className="font-medium">{t("Tap the ring")}</span>
              <span className="block text-muted-foreground">{t("Speak after it says Listening. Tap again to hang up.")}</span>
            </li>
            <li>
              <span className="font-medium">{t("Ask it to do things")}</span>
              <span className="block text-muted-foreground">
                {t("Notes, alarms, timers, reminders, and calendar. Each has its own tab after setup.")}
              </span>
            </li>
            <li>
              <span className="font-medium">{wakeTrained ? t("Or say “Hey Grok”") : t("Wake word is optional")}</span>
              <span className="block text-muted-foreground">
                {wakeTrained
                  ? t("Turn on Listen on the Talk tab. A notification means it is listening.")
                  : t("You skipped enrollment. Train it anytime under Settings, then Wake word, and turn Listen on.")}
              </span>
            </li>
          </ol>
        </CardContent>
      </Card>
      <Card>
        <CardContent className="flex gap-3 pt-4">
          <Headphones className="mt-0.5 size-4 shrink-0 text-primary" />
          <p className="text-sm text-muted-foreground">
            {t("Use headphones the first time. On a loudspeaker the microphone hears the reply and Grok answers itself. Echo guard is on so the mic mutes while Grok is talking. Turn it off in Settings, then Debug, if you want to interrupt.")}
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
