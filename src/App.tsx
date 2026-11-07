import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { Loader2 } from "lucide-react";
import { AssistantView } from "@/components/AssistantView";
import { UpdateBanner } from "@/components/UpdateBanner";
import { BottomNav, type AppTab } from "@/components/BottomNav";
import { CalendarPage } from "@/components/CalendarPage";
import { MorePage } from "@/components/MorePage";
import { NotesPage } from "@/components/NotesPage";
import { Onboarding } from "@/components/Onboarding";
import { RemindersPage } from "@/components/RemindersPage";
import { useAgenda } from "@/hooks/useAgenda";
import { useVoice } from "@/hooks/useVoice";
import { api, errorText, type AuthStatus, type WakewordStatus } from "@/lib/api";
import type { SettingsDestination } from "@/lib/flow";
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
  const [moreFocus, setMoreFocus] = useState<SettingsDestination>("menu");
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
      <main className="safe-area grid h-full place-items-center p-6 text-center">
        <div className="max-w-sm">
          <h1 className="text-xl leading-7 font-medium">{t("Secure storage failed to open")}</h1>
          <p className="mt-3 text-sm leading-6 text-muted-foreground">{bootError}</p>
          <p className="mt-3 text-base leading-6 text-foreground">
            {t("Force-close Grok Voice and open it again. Keys already saved stay in the vault on this phone.")}
          </p>
        </div>
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
      <main className="safe-area flex h-full flex-col">
        <UpdateBanner />
        <div className="min-h-0 flex-1">
          <Onboarding
            preview={!inApp}
            auth={auth}
            wake={wake}
            onAuth={setAuth}
            onWake={setWake}
            onFinished={() => setOnboarded(true)}
          />
        </div>
      </main>
    );
  }

  const openSettings = (where: SettingsDestination) => {
    setMoreFocus(where);
    setTab("more");
  };

  return (
    <main className="safe-area flex h-full flex-col bg-background">
      <UpdateBanner />
      {voice.spend && (
        <div className="mx-4 mt-3 flex items-center justify-between gap-3 border border-border px-3 py-2" role="status">
          <p className="text-sm leading-5 text-foreground">{voice.spend}</p>
          <button type="button" className="min-h-12 shrink-0 px-3 text-sm text-foreground" onClick={voice.dismissSpend}>
            {t("Hide")}
          </button>
        </div>
      )}

      <section className="flex min-h-0 flex-1 flex-col px-4 pt-3">
        <div key={tab} className="flex min-h-0 flex-1 flex-col">
          {tab === "talk" && (
            <AssistantView
              voice={voice}
              wakeTrained={wake.trained}
              accounts={auth.accounts ?? []}
              onOpenSettings={openSettings}
            />
          )}
          {tab === "notes" && <NotesPage agenda={agenda} />}
          {tab === "calendar" && <CalendarPage agenda={agenda} />}
          {tab === "reminders" && <RemindersPage agenda={agenda} />}
          {tab === "more" && (
            <MorePage
              voice={voice}
              wake={wake}
              auth={auth}
              initialSection={moreFocus}
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

      <BottomNav
        tab={tab}
        onChange={(next) => {
          if (next === "more") setMoreFocus("menu");
          setTab(next);
        }}
      />
    </main>
  );
}
