import { useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AssistantView } from "@/components/AssistantView";
import { UpdateBanner } from "@/components/UpdateBanner";
import { BottomNav, useThumbBar, type AppTab } from "@/components/BottomNav";
import { CalendarPage } from "@/components/CalendarPage";
import { FinancePage } from "@/components/FinancePage";
import { FlowShot } from "@/components/FlowShot";
import { Loader } from "@/components/Loader";
import { LoaderShot } from "@/components/LoaderShot";
import { applyBackupResult, MorePage, type Section } from "@/components/MorePage";
import { NotesPage } from "@/components/NotesPage";
import { Onboarding } from "@/components/Onboarding";
import { RemindersPage } from "@/components/RemindersPage";
import { useAgenda } from "@/hooks/useAgenda";
import { useFinance } from "@/hooks/useFinance";
import { useVoice } from "@/hooks/useVoice";
import { api, errorText, type AuthStatus, type RestoreReport, type Settings, type WakewordStatus } from "@/lib/api";
import { applyAppearance, choiceFromSettings, normalizeAppearance, prefersLight, readStoredAppearance, rememberAppearance, rememberCustomTheme, type AppearanceChoice } from "@/lib/appearance";
import { isPersistedWebKey, readPersistedWeb } from "@/lib/persistedKeys";
import { popUntil, pushStep } from "@/lib/history";
import { setAppLocale, useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

function cnShell(thumb: boolean) {
  return cn("app-safe flex h-full flex-col bg-background", thumb && "app-safe-bar");
}

function previewGuide(inApp: boolean, appPreview: boolean): string {
  if (inApp || appPreview || typeof window === "undefined") return "language";
  const guide = new URLSearchParams(window.location.search).get("guide") || "";
  if (!guide) return "language";
  return guide === "error" ? "local" : guide;
}

function previewRoute(inApp: boolean): { app: boolean; tab: AppTab; section: Section } {
  const fallback = { app: false, tab: "talk" as AppTab, section: "menu" as Section };
  if (inApp || !import.meta.env.DEV || typeof window === "undefined") return fallback;
  const params = new URLSearchParams(window.location.search);
  if (!params.has("app")) return fallback;
  const tabs: AppTab[] = ["talk", "notes", "calendar", "reminders", "finance", "more"];
  const asked = params.get("tab");
  const tab = tabs.includes(asked as AppTab) ? (asked as AppTab) : "talk";
  const sections: Section[] = ["menu", "voice", "model", "wake", "people", "spend", "language", "keys", "packs", "music", "updates", "finance", "backup"];
  const sectionAsked = params.get("section");
  const section = sections.includes(sectionAsked as Section) ? (sectionAsked as Section) : "menu";
  return { app: true, tab, section };
}

const PREVIEW_AUTH: AuthStatus = { hasKey: false, keyHint: null, securityLevel: null, accounts: [] };
const PREVIEW_APP_AUTH: AuthStatus = {
  hasKey: true,
  keyHint: null,
  securityLevel: null,
  accounts: [
    { id: "preview", provider: "xai", hint: "xai" },
    { id: "preview-openai", provider: "openai", hint: "openai" },
    { id: "preview-onemin", provider: "onemin", hint: "1min" },
  ],
};
const EMPTY_WAKE: WakewordStatus = { trained: false, samples: [false, false, false] };

export default function App() {
  const inApp = isTauri();
  const preview = previewRoute(inApp);
  const previewApp = preview.app;
  const [auth, setAuth] = useState<AuthStatus | null>(inApp ? null : previewApp ? PREVIEW_APP_AUTH : PREVIEW_AUTH);
  const [onboarded, setOnboarded] = useState<boolean | null>(inApp ? null : previewApp);
  const [setupStep, setSetupStep] = useState<string | null>(inApp ? null : previewApp ? "done" : previewGuide(inApp, previewApp));
  const [bootLocale, setBootLocale] = useState("");
  const [bootError, setBootError] = useState<string | null>(null);
  const [wake, setWake] = useState<WakewordStatus>(EMPTY_WAKE);
  const restorePending = typeof window !== "undefined" && sessionStorage.getItem("grok-restore-summary") != null;
  const [tab, setTab] = useState<AppTab>(restorePending ? "more" : preview.tab);
  const [moreFocus, setMoreFocus] = useState<Section>(restorePending ? "backup" : preview.section);
  const [seen, setSeen] = useState({
    talk: true,
    notes: preview.tab === "notes",
    calendar: preview.tab === "calendar",
    reminders: preview.tab === "reminders",
    finance: preview.tab === "finance",
    more: restorePending || preview.tab === "more",
  });
  const goTab = useRef<(next: AppTab) => void>(() => {});
  const show = (id: AppTab) => {
    setSeen((current) => (current[id] ? current : { ...current, [id]: true }));
  };
  const voice = useVoice();
  const agenda = useAgenda();
  const finance = useFinance();
  const t = useT();
  const thumb = useThumbBar();
  const [appearance, setAppearance] = useState<AppearanceChoice>(() => readStoredAppearance());

  useEffect(() => {
    if (inApp || typeof window === "undefined") return;
    const asked = new URLSearchParams(window.location.search).get("appearance");
    if (!asked) return;
    const choice = normalizeAppearance(asked);
    rememberAppearance(choice);
    setAppearance(choice);
    applyAppearance(choice, prefersLight());
  }, [inApp]);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const paint = () => applyAppearance(appearance, media.matches);
    paint();
    if (appearance !== "system") return;
    media.addEventListener("change", paint);
    return () => media.removeEventListener("change", paint);
  }, [appearance]);

  useEffect(() => {
    if (!preview.app || preview.section === "menu") return;
    pushStep("Back to Settings", () => setMoreFocus("menu"), "section");
  }, [preview.app, preview.section]);

  useEffect(() => {
    if (!inApp) return;
    const onWidget = (ev: Event) => {
      const action = (ev as CustomEvent<string>).detail;
      if (action === "reminders") goTab.current("reminders");
      else if (action === "events") goTab.current("calendar");
      else if (action === "finance") goTab.current("finance");
      else if (action === "talk") {
        goTab.current("talk");
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
        setBootLocale(s.locale || "");
        setSetupStep(s.setupStep || (s.onboarded ? "done" : "language"));
        setOnboarded(s.onboarded);
        if (s.customTheme) rememberCustomTheme(s.customTheme);
        const choice = choiceFromSettings(s.appearance);
        if (choice) {
          rememberAppearance(choice);
          setAppearance(choice);
          applyAppearance(choice, prefersLight());
        }
      })
      .catch(() => {
        setSetupStep("language");
        setOnboarded(false);
      });
    let unlisten: (() => void) | undefined;
    void listen<Settings>("voice://settings", (event) => {
      setAppLocale(event.payload.locale || "");
      if (event.payload.customTheme) rememberCustomTheme(event.payload.customTheme);
      const choice = choiceFromSettings(event.payload.appearance);
      if (choice) {
        rememberAppearance(choice);
        setAppearance(choice);
        applyAppearance(choice, prefersLight());
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [inApp]);

  useEffect(() => {
    if (!inApp) return;
    let pending = 0;
    const flush = () => {
      window.clearTimeout(pending);
      pending = window.setTimeout(() => {
        void api.saveWebPrefs(readPersistedWeb()).catch(() => {});
      }, 40);
    };
    flush();
    const write = localStorage.setItem.bind(localStorage);
    const remove = localStorage.removeItem.bind(localStorage);
    localStorage.setItem = (key, value) => {
      write(key, value);
      if (isPersistedWebKey(key)) flush();
    };
    localStorage.removeItem = (key) => {
      remove(key);
      if (isPersistedWebKey(key)) flush();
    };
    return () => {
      window.clearTimeout(pending);
      localStorage.setItem = write;
      localStorage.removeItem = remove;
    };
  }, [inApp]);

  useEffect(() => {
    if (!inApp) return;
    let unlisten: (() => void) | undefined;
    void listen<RestoreReport>("voice://restored", (event) => {
      applyBackupResult(event.payload);
      show("more");
      setTab("more");
      setMoreFocus("backup");
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [inApp]);

  useEffect(() => {
    if (!inApp) return;
    let unlisten: (() => void) | undefined;
    void listen<{ view?: string; chart?: string; action?: string; name?: string }>("voice://finance", (event) => {
      show("finance");
      setTab("finance");
      window.dispatchEvent(new CustomEvent("grok-finance-voice", { detail: event.payload || {} }));
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [inApp]);

  if (typeof window !== "undefined" && new URLSearchParams(window.location.search).has("flowshot")) {
    return <FlowShot />;
  }

  const previewSearch = !inApp && typeof window !== "undefined" ? new URLSearchParams(window.location.search) : null;
  const loaderShot = previewSearch?.get("loadershot");
  if (loaderShot) return <LoaderShot which={loaderShot} />;

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

  if (previewSearch?.has("boot") || !auth || onboarded === null || setupStep === null) {
    return (
      <main className="grid h-full min-h-0">
        <Loader size="full" label={t("Opening Grok Voice")} />
      </main>
    );
  }

  if (!onboarded) {
    return (
      <main className="setup-safe flex h-full flex-col">
        <UpdateBanner />
        <div className="min-h-0 flex-1">
          <Onboarding
            preview={!inApp}
            auth={auth}
            initialStep={setupStep}
            autoResume={setupStep === "packs-run"}
            locale={bootLocale}
            onAuth={setAuth}
            onFinished={(step) => {
              setSetupStep(step);
              setOnboarded(true);
            }}
          />
        </div>
      </main>
    );
  }

  const openSettings = (where: Section) => {
    if (tab !== "more") {
      const prev = tab;
      const prevFocus = moreFocus;
      pushStep("Back", () => {
        setMoreFocus(prevFocus);
        setTab(prev);
      }, "tab");
      show("more");
      setTab("more");
    }
    if (moreFocus !== where) {
      const prev = moreFocus;
      pushStep("Back to Settings", () => setMoreFocus(prev), "section");
      setMoreFocus(where);
    }
  };

  const changeTab = (next: AppTab) => {
    if (next === tab) {
      popUntil("tab");
      if (next === "more") setMoreFocus("menu");
      return;
    }
    const prev = tab;
    const prevFocus = moreFocus;
    pushStep("Back", () => {
      setMoreFocus(prevFocus);
      setTab(prev);
    }, "tab");
    if (next === "more") setMoreFocus("menu");
    show(next);
    setTab(next);
  };

  goTab.current = changeTab;

  return (
    <main className={cnShell(thumb)}>
      <UpdateBanner />
      {voice.spend && (
        <div className="mx-4 mt-3 flex items-center justify-between gap-3 border border-border px-3 py-2" role="status">
          <p className="text-sm leading-5 text-foreground">{voice.spend}</p>
          <button type="button" className="min-h-12 shrink-0 px-3 text-sm text-foreground" onClick={voice.dismissSpend}>
            {t("Hide")}
          </button>
        </div>
      )}

      <section className="app-stage flex min-h-0 flex-1 flex-col overflow-hidden px-5 pt-6">
        <div className={tab === "talk" ? "flex min-h-0 flex-1 flex-col" : "hidden"}>
          <AssistantView
            voice={voice}
            wakeTrained={wake.trained}
            accounts={auth.accounts ?? []}
            onOpenSettings={openSettings}
          />
        </div>
        {seen.notes ? (
        <div className={tab === "notes" ? "flex min-h-0 flex-1 flex-col" : "hidden"}>
          <NotesPage agenda={agenda} active={tab === "notes"} />
        </div>
        ) : null}
        {seen.calendar ? (
        <div className={tab === "calendar" ? "flex min-h-0 flex-1 flex-col" : "hidden"}>
          <CalendarPage agenda={agenda} active={tab === "calendar"} />
        </div>
        ) : null}
        {seen.reminders ? (
        <div className={tab === "reminders" ? "flex min-h-0 flex-1 flex-col" : "hidden"}>
          <RemindersPage agenda={agenda} active={tab === "reminders"} />
        </div>
        ) : null}
        {seen.finance ? (
        <div className={tab === "finance" ? "flex min-h-0 flex-1 flex-col" : "hidden"}>
          <FinancePage finance={finance} active={tab === "finance"} onOpenCurrency={() => openSettings("finance")} />
        </div>
        ) : null}
        {seen.more ? (
        <div className={tab === "more" ? "flex min-h-0 flex-1 flex-col" : "hidden"}>
          <MorePage
            voice={voice}
            wake={wake}
            auth={auth}
            active={tab === "more"}
            initialSection={moreFocus}
            onSection={setMoreFocus}
            onAuth={setAuth}
            onWake={setWake}
            onSignedOut={() => {
              setSetupStep("language");
              setOnboarded(false);
              void api.authStatus().then(setAuth);
            }}
            onReplayTour={(step) => {
              setSetupStep(step || "language");
              setOnboarded(false);
            }}
            finance={finance}
            onAppearance={setAppearance}
          />
        </div>
        ) : null}
      </section>

      <BottomNav tab={tab} onChange={changeTab} />
    </main>
  );
}
