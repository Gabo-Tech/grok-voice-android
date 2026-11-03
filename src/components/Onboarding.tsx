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
import { WakeView } from "@/components/WakeView";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api, errorText, type AuthStatus, type PermissionStatus, type WakewordStatus } from "@/lib/api";
import { cn } from "@/lib/utils";

const CONSOLE_URL = "https://console.x.ai/team/default/api-keys";

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
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [perms, setPerms] = useState<PermissionStatus | null>(null);
  const [done, setDone] = useState(false);

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
        onAuth({ hasKey: true, keyHint: "xai-…demo", securityLevel: "software" });
        return;
      }
      const status = await api.saveUserToken(key);
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
        <p className="text-[11px] tracking-[0.35em] text-primary uppercase">Setup</p>
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
              <span className="text-[9px] tracking-widest text-muted-foreground uppercase">{s.label}</span>
            </li>
          );
        })}
      </ol>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {id === "hello" && <HelloStep />}
        {id === "key" && (
          <KeyStep
            auth={auth}
            value={key}
            busy={busy}
            onChange={setKey}
            onOpen={() => {
              if (preview) {
                window.open(CONSOLE_URL, "_blank", "noopener");
                return;
              }
              void openUrl(CONSOLE_URL);
            }}
            onSave={() => void saveKey()}
          />
        )}
        {id === "mic" && <MicStep perms={perms} busy={busy} onAllow={() => void allowMic()} />}
        {id === "wake" && <WakeView status={wake} onStatus={onWake} />}
        {id === "ready" && <ReadyStep wakeTrained={wake.trained} />}
      </div>

      {error && <p className="mt-3 text-sm text-destructive">{error}</p>}
      {done && preview && (
        <p className="mt-3 text-center text-sm text-primary">Preview finished. In the app this opens the assistant.</p>
      )}

      <footer className="mt-4 flex gap-2 pt-2">
        <Button variant="outline" disabled={step === 0 || busy} onClick={back}>
          <ChevronLeft /> Back
        </Button>
        <Button className="flex-1" disabled={!canNext} onClick={next}>
          {busy && id === "ready" ? <Loader2 className="animate-spin" /> : <ChevronRight />}
          {id === "wake" && !wake.trained ? "Skip for now" : id === "ready" ? "Start talking" : "Next"}
        </Button>
      </footer>
    </div>
  );
}

function HelloStep() {
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
        <h2 className="text-xl font-medium">A voice assistant that stays on your phone</h2>
        <p className="mt-2 text-sm text-muted-foreground">Five short steps. About two minutes.</p>
      </div>
      <ul className="space-y-3">
        <Point n="1" title="Bring your own xAI key">
          It is checked once, then locked in on-device storage. This app has no server.
        </Point>
        <Point n="2" title="Allow the microphone">
          Needed to hear you, including when the screen is off.
        </Point>
        <Point n="3" title="Teach “Hey Grok”">
          Three short recordings. They never leave the phone. You can skip and tap instead.
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
  value,
  busy,
  onChange,
  onOpen,
  onSave,
}: {
  auth: AuthStatus;
  value: string;
  busy: boolean;
  onChange: (v: string) => void;
  onOpen: () => void;
  onSave: () => void;
}) {
  return (
    <Card className="border-primary/20">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <KeyRound className="size-4 text-primary" /> Add your xAI API key
        </CardTitle>
        <CardDescription>Do these three things, in order.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <ol className="list-decimal space-y-1 pl-4 text-sm text-muted-foreground">
          <li>Open the xAI console and sign in.</li>
          <li>Create an API key and copy it.</li>
          <li>Paste it here. We verify it, then store it on this device.</li>
        </ol>
        <Button variant="outline" className="w-full" onClick={onOpen}>
          <ExternalLink /> Open console.x.ai
        </Button>
        {auth.hasKey ? (
          <p className="flex items-center gap-2 rounded-md border border-primary/30 bg-primary/10 px-3 py-2 text-sm text-primary">
            <ShieldCheck className="size-4 shrink-0" />
            Key stored{auth.keyHint ? ` (${auth.keyHint})` : ""}
            {auth.securityLevel ? ` · ${auth.securityLevel}` : ""}. Tap Next.
          </p>
        ) : (
          <>
            <div className="space-y-2">
              <Label htmlFor="xai-key">Paste the key</Label>
              <Input
                id="xai-key"
                type="password"
                autoComplete="off"
                autoCapitalize="off"
                spellCheck={false}
                placeholder="xai-…"
                value={value}
                onChange={(e) => onChange(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && value && !busy && onSave()}
              />
            </div>
            <Button className="w-full" disabled={!value || busy} onClick={onSave}>
              {busy ? <Loader2 className="animate-spin" /> : <ShieldCheck />}
              {busy ? "Verifying with xAI…" : "Verify and store"}
            </Button>
          </>
        )}
        <p className="text-[11px] leading-relaxed text-muted-foreground">
          Voice time is billed to your xAI account. Each conversation uses a 5-minute token minted
          on the phone. The key itself is not sent on the voice connection.
        </p>
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
  return (
    <Card className="border-primary/20">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Mic className="size-4 text-primary" /> Allow the microphone
        </CardTitle>
        <CardDescription>Android will show a system prompt. Choose Allow.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <p className="text-sm text-muted-foreground">
          The microphone hears “Hey Grok” and your question. A quiet notification stays up so
          Android does not switch the microphone off when the screen locks.
        </p>
        <Button className="w-full" disabled={busy || mic === "granted"} onClick={onAllow}>
          {busy ? <Loader2 className="animate-spin" /> : <Mic />}
          {mic === "granted" ? "Microphone allowed" : "Allow microphone"}
        </Button>
        <StatusLine label="Microphone" value={mic} />
        <StatusLine label="Notification" value={notes} />
        {mic === "denied" && (
          <p className="text-sm text-destructive">
            Microphone is blocked. Open Android Settings → Apps → Grok Voice → Permissions, allow
            Microphone, then come back and tap the button again.
          </p>
        )}
        {mic === "granted" && notes !== "granted" && (
          <p className="text-xs text-muted-foreground">
            Notifications are off. Listening still works while the app is open. Allow notifications
            later if you want “Hey Grok” with the screen locked.
          </p>
        )}
      </CardContent>
    </Card>
  );
}

function StatusLine({ label, value }: { label: string; value?: string }) {
  const ok = value === "granted";
  return (
    <div className="flex items-center justify-between text-xs">
      <span className="text-muted-foreground">{label}</span>
      <span className={ok ? "text-primary" : "text-muted-foreground"}>{value ?? "not requested"}</span>
    </div>
  );
}

function ReadyStep({ wakeTrained }: { wakeTrained: boolean }) {
  return (
    <div className="space-y-3">
      <Card className="border-primary/20">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <AudioLines className="size-4 text-primary" /> How to talk
          </CardTitle>
        </CardHeader>
        <CardContent>
          <ol className="space-y-3 text-sm">
            <li>
              <span className="font-medium">Tap the ring</span>
              <span className="block text-muted-foreground">Speak after it says Listening. Tap again to hang up.</span>
            </li>
            <li>
              <span className="font-medium">Ask it to do things</span>
              <span className="block text-muted-foreground">
                Notes, alarms, timers, reminders, and calendar. Each has its own tab after setup.
              </span>
            </li>
            <li>
              <span className="font-medium">{wakeTrained ? "Or say “Hey Grok”" : "Wake word is optional"}</span>
              <span className="block text-muted-foreground">
                {wakeTrained
                  ? "Turn on Listen on the Talk tab. A notification means it is listening."
                  : "You skipped enrollment. Train it anytime under More → Wake word, then flip Listen on."}
              </span>
            </li>
          </ol>
        </CardContent>
      </Card>
      <Card>
        <CardContent className="flex gap-3 pt-4">
          <Headphones className="mt-0.5 size-4 shrink-0 text-primary" />
          <p className="text-sm text-muted-foreground">
            Use headphones the first time. On a loudspeaker the microphone hears the reply and Grok
            answers itself. Echo guard is on so the mic mutes while Grok is talking. Turn it off in
            Debug if you want to interrupt.
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
