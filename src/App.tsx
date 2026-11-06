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
import { setAppLocale, useT } from "@/lib/i18n";

const PREVIEW_AUTH: AuthStatus = { hasKey: false, keyHint: null, securityLevel: null, accounts: [] };
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
  const t = useT();

  useEffect(() => {
    if (!inApp) return;
    const onWidget = (ev: Event) => {
      const action = (ev as CustomEvent<string>).detail;
      if (action === "reminders") setTab("reminders");
      else if (action === "events") setTab("calendar");
      else if (action === "talk") {
        setTab("talk");
        void api.startSession().catch(() => {});
      } else if (action === "arm") {
        void api.setWakeEnabled(true).catch(() => {});
      } else if (action === "disarm") {
        void api.setWakeEnabled(false).catch(() => {});
      }
    };
    window.addEventListener("grok-widget", onWidget);
    return () => window.removeEventListener("grok-widget", onWidget);
  }, [inApp]);

  useEffect(() => {
    if (!inApp) return;
    api.authStatus().then(setAuth).catch((e) => setBootError(errorText(e)));
    void api.wakewordStatus().then(setWake);
    api
      .getSettings()
      .then((s) => {
        setAppLocale(s.locale || "");
        setOnboarded(s.onboarded);
      })
      .catch(() => setOnboarded(false));
  }, [inApp]);

  if (bootError) {
    return (
      <main className="grid h-full place-items-center p-6 text-center text-sm text-destructive">
        {t("Secure storage failed to open")}: {bootError}
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
      <header className="flex shrink-0 items-start justify-between gap-3 px-5 pt-3 pb-1">
        <h1 className="min-w-0 text-2xl leading-tight font-medium tracking-tight text-foreground">Grok Talk</h1>
        <p className="max-w-[40%] shrink-0 truncate pt-1 text-[10px] tracking-[0.12em] text-muted-foreground/80">{auth.keyHint}</p>
      </header>

      {voice.spend && (
        <div className="mx-4 mb-2 flex items-start justify-between gap-3 border border-border px-3 py-2">
          <p className="text-xs leading-5 text-foreground">{voice.spend}</p>
          <button type="button" className="shrink-0 text-[10px] tracking-[0.12em] text-muted-foreground uppercase" onClick={voice.dismissSpend}>
            {t("Hide")}
          </button>
        </div>
      )}

      <section className="flex min-h-0 flex-1 flex-col px-4 pt-2">
        <div key={tab} className="flex min-h-0 flex-1 flex-col">
          {tab === "talk" && <AssistantView voice={voice} wakeTrained={wake.trained} />}
          {tab === "notes" && <NotesPage agenda={agenda} />}
          {tab === "calendar" && <CalendarPage agenda={agenda} />}
          {tab === "reminders" && <RemindersPage agenda={agenda} />}
          {tab === "more" && (
            <MorePage
              voice={voice}
              wake={wake}
              auth={auth}
              onAuth={setAuth}
              onWake={setWake}
              onSignedOut={() => {
                setOnboarded(false);
                void api.authStatus().then(setAuth);
              }}
              onReplayTour={() => setOnboarded(false)}
            />
          )}
        </div>
      </section>

      <BottomNav tab={tab} onChange={setTab} />
    </main>
  );
}
