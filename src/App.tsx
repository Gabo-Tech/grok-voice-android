import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { Loader2 } from "lucide-react";
import { AssistantView } from "@/components/AssistantView";
import { BottomNav, type AppTab } from "@/components/BottomNav";
import { CalendarPage } from "@/components/CalendarPage";
import { MorePage } from "@/components/MorePage";
import { NotesPage } from "@/components/NotesPage";
import { Onboarding } from "@/components/Onboarding";
import { RemindersPage } from "@/components/RemindersPage";
import { useAgenda } from "@/hooks/useAgenda";
import { useVoice } from "@/hooks/useVoice";
import { api, errorText, type AuthStatus, type WakewordStatus } from "@/lib/api";

const PREVIEW_AUTH: AuthStatus = { hasKey: false, keyHint: null, securityLevel: null };
const EMPTY_WAKE: WakewordStatus = { trained: false, samples: [false, false, false] };

export default function App() {
  const inApp = isTauri();
  const [auth, setAuth] = useState<AuthStatus | null>(inApp ? null : PREVIEW_AUTH);
  const [onboarded, setOnboarded] = useState<boolean | null>(inApp ? null : false);
  const [bootError, setBootError] = useState<string | null>(null);
  const [wake, setWake] = useState<WakewordStatus>(EMPTY_WAKE);
  const [tab, setTab] = useState<AppTab>("talk");
  const voice = useVoice();
  const agenda = useAgenda();

  useEffect(() => {
    if (!inApp) return;
    api.authStatus().then(setAuth).catch((e) => setBootError(errorText(e)));
    void api.wakewordStatus().then(setWake);
    api
      .getSettings()
      .then((s) => setOnboarded(s.onboarded))
      .catch(() => setOnboarded(false));
  }, [inApp]);

  if (bootError) {
    return (
      <main className="grid h-full place-items-center p-6 text-center text-sm text-destructive">
        Secure storage failed to open: {bootError}
      </main>
    );
  }

  if (!auth || onboarded === null) {
    return (
      <main className="grid h-full place-items-center">
        <Loader2 className="size-6 animate-spin text-primary" />
      </main>
    );
  }

  if (!onboarded || !auth.hasKey) {
    return (
      <main className="safe-area h-full">
        <Onboarding
          preview={!inApp}
          auth={auth}
          wake={wake}
          onAuth={setAuth}
          onWake={setWake}
          onFinished={() => setOnboarded(true)}
        />
      </main>
    );
  }

  return (
    <main className="safe-area flex h-full flex-col bg-background">
      <header className="flex shrink-0 items-center justify-between px-5 pt-3 pb-1">
        <p className="text-[11px] font-medium tracking-[0.28em] text-muted-foreground">GROK</p>
        <p className="text-[10px] tracking-[0.12em] text-muted-foreground/80">{auth.keyHint}</p>
      </header>

      <section className="flex min-h-0 flex-1 flex-col px-4 pt-2">
        {tab === "talk" && <AssistantView voice={voice} wakeTrained={wake.trained} />}
        {tab === "notes" && <NotesPage agenda={agenda} />}
        {tab === "calendar" && <CalendarPage agenda={agenda} />}
        {tab === "reminders" && <RemindersPage agenda={agenda} />}
        {tab === "more" && (
          <MorePage
            voice={voice}
            wake={wake}
            onWake={setWake}
            onSignedOut={() => {
              setOnboarded(false);
              void api.authStatus().then(setAuth);
            }}
            onReplayTour={() => setOnboarded(false)}
          />
        )}
      </section>

      <BottomNav tab={tab} onChange={setTab} />
    </main>
  );
}
