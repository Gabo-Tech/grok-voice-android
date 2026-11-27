import { useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Archive, AudioLines, Bell, Brain, Bug, ChevronDown, ChevronRight, Download, Hourglass, Languages, Music, Users, Wallet, Wand2 } from "lucide-react";
import { DebugPanel } from "@/components/DebugPanel";
import { VoiceHome } from "@/components/VoiceHome";
import { Field } from "@/components/Field";
import { CurrencyEditor } from "@/components/CurrencyEditor";
import { AppMark } from "@/components/AppMark";
import { PageChrome, RowButton } from "@/components/PageChrome";
import { InfoTip } from "@/components/ds/InfoTip";
import { ThemePicker } from "@/components/ThemePicker";
import { PeoplePage } from "@/components/PeoplePage";
import { WakeView } from "@/components/WakeView";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import type { FinanceState } from "@/hooks/useFinance";
import type { VoiceState } from "@/hooks/useVoice";
import { api, errorText, type AccessStatus, type AuthStatus, type BackupPreview, type InboxApp, type ListenHealth, type LocalMemoryBundle, type MusicPlayerChoice, type RestoreReport, type Settings, type UpdateStatus, type UsageStatus, type WakewordStatus } from "@/lib/api";
import { applyAppearance, choiceFromSettings, normalizeAppearance, prefersLight, rememberAppearance, rememberCustomTheme } from "@/lib/appearance";
import { providerLabel } from "@/lib/flow";
import { isApplyingHistory, popTags, pushStep } from "@/lib/history";
import { activeLocale, fill, setAppLocale, useT } from "@/lib/i18n";
import { countPhrase, type CountKind } from "@/lib/plurals";
import { readPersistedWeb, writePersistedWeb } from "@/lib/persistedKeys";
import { cn } from "@/lib/utils";

export type Section =
  | "menu"
  | "wake"
  | "debug"
  | "keys"
  | "voice"
  | "people"
  | "backup"
  | "memory"
  | "access"
  | "packs"
  | "spend"
  | "quiet"
  | "model"
  | "language"
  | "alerts"
  | "music"
  | "updates"
  | "finance";

export function MorePage({
  voice,
  wake,
  auth,
  initialSection = "menu",
  onSection,
  active = true,
  onAuth,
  onWake,
  onSignedOut,
  onReplayTour,
  finance,
  onAppearance,
}: {
  voice: VoiceState;
  wake: WakewordStatus;
  auth: AuthStatus;
  initialSection?: Section;
  onSection: (section: Section) => void;
  active?: boolean;
  onAuth: (auth: AuthStatus) => void;
  onWake: (s: WakewordStatus) => void;
  onSignedOut: () => void;
  onReplayTour: (step?: string) => void;
  finance: FinanceState;
  onAppearance: (choice: ReturnType<typeof normalizeAppearance>) => void;
}) {
  const t = useT();
  const section = initialSection;
  const setSection = (next: Section) => {
    if (next === section || isApplyingHistory()) return;
    if (next === "menu") {
      popTags(["sheet", "people", "section"]);
      onSection("menu");
      return;
    }
    const prev = section;
    pushStep("Back to Settings", () => onSection(prev), "section");
    onSection(next);
  };
  const [quiet, setQuiet] = useState(10);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [usage, setUsage] = useState<UsageStatus | null>(null);
  const [update, setUpdate] = useState<UpdateStatus | null>(null);
  const [settingsQuery, setSettingsQuery] = useState("");
  const [openGroups, setOpenGroups] = useState<Record<string, boolean>>({});

  useEffect(() => {
    void api
      .getSettings()
      .then((s) => {
        setQuiet(s.idleTimeoutSecs);
        setSettings(s);
        if (s.customTheme) rememberCustomTheme(s.customTheme);
        const choice = normalizeAppearance(s.appearance);
        if (s.appearance) onAppearance(choice);
      })
      .catch(() => {
        if (!isTauri()) {
          setQuiet(10);
          setSettings({
            model: "grok-voice-latest",
            voice: "eve",
            openaiVoice: "marin",
            openaiModel: "gpt-realtime",
            oneminModel: "gpt-4.1-mini",
            instructions: "",
            echoGuard: true,
            wakeThreshold: 0.5,
            idleTimeoutSecs: 10,
            onboarded: true,
            dailyBudgetMinutes: 0,
            dailyBudgetHardStop: false,
            preferLocalOnWake: false,
            monthlyBudgetMinutes: 0,
            monthlyBudgetHardStop: false,
            localSpeech: "en",
            locale: "",
            musicPlayer: "local",
            appearance: "graphite",
          });
        }
      });
    void api.usageStatus().then(setUsage).catch(() => {});
    void api.updateStatus().then(setUpdate).catch(() => {});
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    void listen<Settings>("voice://settings", (event) => {
      setSettings(event.payload);
      setQuiet(event.payload.idleTimeoutSecs);
      setAppLocale(event.payload.locale || "");
      if (event.payload.customTheme) rememberCustomTheme(event.payload.customTheme);
      if (event.payload.appearance) onAppearance(normalizeAppearance(event.payload.appearance));
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [onAppearance]);

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

  if (section === "wake") {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        <Back onClick={() => setSection("menu")} />
        <ListenStatus />
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
    return <PeoplePage active={active} onBack={() => setSection("menu")} />;
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
      <div className="backup-pad flex min-h-0 flex-1 flex-col overflow-y-auto">
        <Back onClick={() => setSection("menu")} />
        <BackupPage />
      </div>
    );
  }

  if (section === "keys" || section === "packs" || section === "model" || section === "voice") {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        <Back onClick={() => setSection("menu")} />
        {settings ? (
          <VoiceHome
            settings={settings}
            auth={auth}
            onAuth={onAuth}
            onSettings={(next) => {
              setSettings(next);
              void api.saveSettings(next).then(setSettings).catch(() => setSettings(next));
            }}
            onOpenTour={(step) => {
              const resume = step && step !== "done" ? step : "path";
              const next = { ...settings, onboarded: false, setupStep: resume };
              setSettings(next);
              void api.saveSettings(next).finally(() => onReplayTour(resume));
            }}
          />
        ) : (
          <p className="px-1 text-sm text-muted-foreground">{t("Couldn't load the budget. Go back and open Settings again.")}</p>
        )}
      </div>
    );
  }

  if (section === "quiet") {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        <Back onClick={() => setSection("menu")} />
        <h2 className="px-1 text-2xl font-medium tracking-tight">{t("Quiet gap")}</h2>
        <p className="mt-1 mb-4 px-1 text-sm leading-6 text-muted-foreground">
          {t("After this pause, the conversation ends and only the wake word is heard.")}
        </p>
        <div className="border-y border-border px-1 py-3">
          <div className="flex items-center justify-between gap-3">
            <p className="text-base text-foreground">{t("Pause")}</p>
            <p className="shrink-0 text-sm tabular-nums text-foreground">{quiet}s</p>
          </div>
          <div className="mt-3 flex items-center gap-2">
            <button type="button" className="h-12 min-w-12 border border-border px-3 text-sm" onClick={() => setQuietGap(quiet - 5)}>
              −5
            </button>
            <input
              type="range"
              min={5}
              max={120}
              step={5}
              value={quiet}
              aria-label={t("Quiet gap")}
              className="w-full accent-white"
              onChange={(e) => setQuietGap(Number(e.target.value))}
            />
            <button type="button" className="h-12 min-w-12 border border-border px-3 text-sm" onClick={() => setQuietGap(quiet + 5)}>
              +5
            </button>
          </div>
        </div>
      </div>
    );
  }

  if (section === "spend") {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto pb-6">
        <Back onClick={() => setSection("menu")} />
        <h2 className="px-1 text-2xl font-medium tracking-tight">{t("Daily voice budget")}</h2>
        {settings ? (
          <div className="mt-4 space-y-4">
            <div className="border border-border px-3 py-3">
              <p className="text-sm leading-6 text-muted-foreground">
                {t("Off by default. Near the minutes you pick, one short cue appears and can be spoken. The call keeps going unless you also turn on a hard stop.")}
              </p>
              <p className="mt-2 text-sm text-foreground">
                {usage
                  ? fill("{minutes} min across {count} {unit} today", {
                      minutes: usage.minutes,
                      count: usage.sessions,
                      unit: usage.sessions === 1 ? t("session") : t("sessions"),
                    })
                  : t("Today's time is counted on this phone.")}
              </p>
              <div className="mt-3 grid grid-cols-2 gap-2">
                {[
                  { minutes: 0, label: t("Off") },
                  { minutes: 15, label: "15" },
                  { minutes: 30, label: "30" },
                  { minutes: 60, label: "60" },
                ].map((choice) => (
                  <button
                    key={choice.label}
                    type="button"
                    onClick={() => saveBudget({ ...settings, dailyBudgetMinutes: choice.minutes })}
                    className={cn(
                      "h-12 border text-sm",
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
                <span>{settings.dailyBudgetHardStop ? t("On") : t("Off")}</span>
              </button>
            </div>
            {usage && (
              <p className="text-sm text-foreground">
                xAI {usage.xaiMinutes} · OpenAI {usage.openaiMinutes} · 1min.ai {usage.oneminMinutes ?? 0} · {t("On device")} {usage.localMinutes}
                {usage.monthMinutes ? ` · ${usage.month} ${usage.monthMinutes}` : ""}
              </p>
            )}
            <div className="border border-border px-3 py-3">
              <p className="text-base text-foreground">{t("Monthly voice budget")}</p>
              <p className="mt-1 text-sm leading-6 text-muted-foreground">
                {t("Counts xAI, OpenAI, and on-device time together, so a fallback key is not silent.")}
              </p>
              <div className="mt-3 grid grid-cols-2 gap-2">
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
                      "h-12 border text-sm",
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
                <span>{settings.monthlyBudgetHardStop ? t("On") : t("Off")}</span>
              </button>
            </div>
          </div>
        ) : (
          <p className="mt-4 px-1 text-sm text-muted-foreground">{t("Couldn't load the budget. Go back and open Settings again.")}</p>
        )}
      </div>
    );
  }

  if (section === "alerts") {
    const mode = settings?.notificationDelivery === "spoken" ? "spoken" : "normal";
    const choose = (next: "normal" | "spoken") => {
      if (!settings) return;
      const savedSettings = { ...settings, notificationDelivery: next };
      setSettings(savedSettings);
      void api.saveSettings(savedSettings).then(setSettings).catch(() => {});
    };
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto pb-6">
        <Back onClick={() => setSection("menu")} />
        <h2 className="px-1 text-2xl font-medium tracking-tight">{t("Notification alerts")}</h2>
        <p className="mt-1 mb-4 px-1 text-sm leading-6 text-muted-foreground">
          {t("Normal shows the notification as it does today. Spoken also reads a short sentence when it fires. The listening notification and downloads stay quiet.")}
        </p>
        {settings ? (
          <div className="space-y-3">
            <button
              type="button"
              onClick={() => choose("normal")}
              className={cn(
                "min-h-14 w-full border px-3 py-3 text-left",
                mode === "normal" ? "border-foreground text-foreground" : "border-border text-muted-foreground",
              )}
            >
              <span className="block text-base">{t("Normal")}</span>
              <span className="mt-1 block text-sm leading-5">{t("Show the notification. Grok stays quiet.")}</span>
            </button>
            <button
              type="button"
              onClick={() => choose("spoken")}
              className={cn(
                "min-h-14 w-full border px-3 py-3 text-left",
                mode === "spoken" ? "border-foreground text-foreground" : "border-border text-muted-foreground",
              )}
            >
              <span className="block text-base">{t("Spoken out loud")}</span>
              <span className="mt-1 block text-sm leading-5">{t("Show the notification and read it out loud.")}</span>
            </button>
            <Field
              label={t("Your name")}
              hint={t("Optional. Spoken alerts can say it. Leave this empty for a plain hey.")}
            >
              <Input
                key={settings.spokenName ?? ""}
                defaultValue={settings.spokenName ?? ""}
                autoComplete="given-name"
                onBlur={(e) => {
                  const spokenName = e.target.value.trim();
                  if ((settings.spokenName ?? "") === spokenName) return;
                  const savedSettings = { ...settings, spokenName };
                  setSettings(savedSettings);
                  void api.saveSettings(savedSettings).then(setSettings).catch(() => {});
                }}
              />
            </Field>
            <p className="px-1 text-sm leading-6 text-muted-foreground">{t("Spoken alerts use the alarm volume.")}</p>
          </div>
        ) : (
          <p className="mt-4 px-1 text-sm text-muted-foreground">{t("Couldn't load the budget. Go back and open Settings again.")}</p>
        )}
      </div>
    );
  }

  if (section === "language") {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto pb-6">
        <Back onClick={() => setSection("menu")} />
        <h2 className="px-1 text-2xl font-medium tracking-tight">{t("Language")}</h2>
        {settings ? (
          <div className="mt-4 space-y-6">
            <div>
              <p className="mb-2 text-sm text-muted-foreground">{t("Language")}</p>
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
                      "min-h-12 h-auto border px-2 py-2 text-center text-sm leading-4 whitespace-normal",
                      (settings.locale || "") === choice.id ? "border-foreground text-foreground" : "border-border text-muted-foreground",
                    )}
                  >
                    {choice.label}
                  </button>
                ))}
              </div>
            </div>
            <p className="text-sm leading-6 text-muted-foreground">{t("Listening and the voice are in Voice and AI.")}</p>
          </div>
        ) : (
          <p className="mt-4 px-1 text-sm text-muted-foreground">{t("Couldn't load the budget. Go back and open Settings again.")}</p>
        )}
      </div>
    );
  }

  if (section === "music") {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto pb-6">
        <Back onClick={() => setSection("menu")} />
        <MusicSettings
          settings={settings}
          onChange={(next) => {
            setSettings(next);
            void api.saveSettings(next).then(setSettings).catch(() => setSettings(next));
          }}
        />
      </div>
    );
  }

  if (section === "updates") {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto pb-6">
        <Back onClick={() => setSection("menu")} />
        <UpdatesSettings installed={update?.installed ?? ""} available={update?.version ?? ""} />
      </div>
    );
  }

  if (section === "finance") {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto pb-6">
        <Back onClick={() => setSection("menu")} />
        <h2 className="masthead px-1 text-2xl">{t("Finance")}</h2>
        <div className="masthead-rule mx-1" />
        <p className="mt-3 mb-4 px-1 text-sm leading-6 text-muted-foreground">
          {t("Live rate for the day. Native amounts stay on each item.")}
        </p>
        <CurrencyEditor finance={finance} />
      </div>
    );
  }

  const accounts = auth.accounts ?? [];
  const first = accounts[0];
  const query = settingsQuery.trim().toLowerCase();
  const languageDetail = localeDetail(settings?.locale, t);
  const saveLook = (next: Settings) => {
    setSettings(next);
    if (next.customTheme) rememberCustomTheme(next.customTheme);
    if (next.appearance) onAppearance(normalizeAppearance(next.appearance));
    void api.saveSettings(next).then(setSettings).catch(() => setSettings(next));
  };
  const groups: SettingsFolder[] = [
    {
      id: "appearance",
      title: t("Appearance"),
      hint: `${t("Theme")} ${t("Graphite")} ${t("Ink")} ${t("Stone")} ${t("Midnight")} ${t("Carbon")} ${t("Ivory")} ${t("Sand")} ${t("Forest")} ${t("Plum")} ${t("System")} ${t("Custom")}`,
      extra: (
        <>
          <ThemePicker
            settings={settings}
            onChange={saveLook}
            info={<InfoTip label={t("Theme")} text={t("Home screen widgets use this theme and the currency in Settings. Add Listen, reminders, the schedule, net worth, this month, or the next 30 days from the phone's home screen.")} />}
          />
        </>
      ),
      rows: [],
    },
    {
      id: "voice",
      title: t("Voice"),
      rows: [
        { section: "voice", icon: <AudioLines className="size-4" />, title: t("Voice and AI"), detail: first ? t(providerLabel(first.provider)) : "" },
        {
          section: "spend",
          icon: <Wallet className="size-4" />,
          title: t("Daily voice budget"),
          detail: settings?.dailyBudgetMinutes ? fill("{n} min", { n: settings.dailyBudgetMinutes }) : t("A soft cap on talk time. Off until you set minutes."),
        },
        { section: "quiet", icon: <Hourglass className="size-4" />, title: t("Quiet gap"), detail: "" },
      ],
    },
    {
      id: "listen",
      title: t("Listen"),
      rows: [
        {
          section: "wake",
          icon: <Wand2 className="size-4" />,
          title: t("Wake word"),
          detail: wake.trained ? t("Hey Grok enrolled") : t("Hands-free Hey Grok. It can stay on when you leave."),
        },
      ],
    },
    {
      id: "alerts",
      title: t("Spoken alerts"),
      rows: [
        {
          section: "alerts",
          icon: <Bell className="size-4" />,
          title: t("Notification alerts"),
          detail: settings?.notificationDelivery === "spoken" ? t("Spoken out loud") : t("A notification, or a spoken line that uses your name."),
        },
        {
          section: "access",
          icon: <Bell className="size-4" />,
          title: t("Phone access"),
          detail: t("Messages, calls, notifications, and Do Not Disturb."),
        },
      ],
    },
    {
      id: "music",
      title: t("Music"),
      rows: [
        {
          section: "music",
          icon: <Music className="size-4" />,
          title: t("Default music player"),
          detail: playerDetail(settings?.musicPlayer, t),
        },
      ],
    },
    {
      id: "finance",
      title: t("Finance"),
      rows: [
        {
          section: "finance",
          icon: <Wallet className="size-4" />,
          title: t("Default currency"),
          detail: finance.snapshot.defaultCurrency
            ? finance.snapshot.defaultCurrency.toUpperCase()
            : t("Used when you do not name a currency."),
        },
      ],
    },
    {
      id: "people",
      title: t("People"),
      rows: [{ section: "people", icon: <Users className="size-4" />, title: t("People"), detail: t("Nicknames, birthdays, and how you reach them.") }],
    },
    {
      id: "language",
      title: t("Language"),
      rows: [{ section: "language", icon: <Languages className="size-4" />, title: t("Language"), detail: languageDetail }],
    },
    {
      id: "privacy",
      title: t("Backup"),
      rows: [
        { section: "backup", icon: <Archive className="size-4" />, title: t("Backup"), detail: t("Everything saved on this phone, including keys.") },
        { section: "memory", icon: <Brain className="size-4" />, title: t("Memory"), detail: t("Pinned facts Talk can use on this phone.") },
      ],
    },
    {
      id: "about",
      title: t("About"),
      rows: [
        {
          section: "updates",
          icon: <Download className="size-4" />,
          title: t("Updates"),
          detail: update?.installed ? `${t("Installed version")} ${update.installed}` : t("Check for a newer version. Nothing installs from here."),
        },
        { section: "debug", icon: <Bug className="size-4" />, title: t("Debug"), detail: t("Logs and echo guard, if something sounds wrong.") },
      ],
    },
  ];

  const visibleGroups = groups
    .map((group) => ({
      ...group,
      rows: group.rows.filter((row) => !query || `${group.title} ${group.hint ?? ""} ${row.title} ${row.detail}`.toLowerCase().includes(query)),
    }))
    .filter((group) => group.rows.length > 0 || (group.extra && (!query || `${group.title} ${group.hint ?? ""}`.toLowerCase().includes(query))));

  return (
    <PageChrome mark title={t("Settings")} info={<InfoTip label={t("Settings")} text={t("Voice, music, and this phone")} />} active={active && section === "menu"}>
      <ListenStatus />
      <div className="title-line mb-4">
        <span className="text-base text-foreground">{t("Provider")}</span>
        <InfoTip
          label={t("Provider")}
          text={
            accounts.length > 1
              ? `${accounts.slice(1).map((account) => t(providerLabel(account.provider))).join(" · ")}. ${t("Add another voice if you want a fallback.")}`
              : first
                ? t("Add another voice if you want a fallback.")
                : t("Add xAI, OpenAI, or On device.")
          }
        />
        <button type="button" className="ml-auto flex min-h-12 items-center gap-1 text-sm text-muted-foreground" onClick={() => setSection("voice")}>
          <span>{first ? t(providerLabel(first.provider)) : t("No voice yet")}</span>
          <ChevronRight className="size-4" aria-hidden />
        </button>
      </div>
      {usage ? (
        <p className="mb-4 text-sm text-muted-foreground">
          {fill("{minutes} min · {count} {unit}", {
            minutes: usage.minutes,
            count: usage.sessions,
            unit: usage.sessions === 1 ? t("session") : t("sessions"),
          })}
        </p>
      ) : (
        <div className="title-line mb-4">
          <span className="text-base text-foreground">{t("Daily voice budget")}</span>
          <InfoTip label={t("Daily voice budget")} text={t("Today's time is counted on this phone.")} />
        </div>
      )}
      <label className="mb-4 block px-1">
        <span className="sr-only">{t("Find a setting")}</span>
        <Input
          value={settingsQuery}
          onChange={(event) => setSettingsQuery(event.target.value)}
          placeholder={t("Find a setting")}
          aria-label={t("Find a setting")}
        />
      </label>
      {visibleGroups.length === 0 ? <p className="px-1 text-sm text-muted-foreground">{t("No settings match that.")}</p> : null}
      {visibleGroups.map((group) => {
        const open = query.length > 0 || openGroups[group.id] !== false;
        return (
          <section key={group.id} className="mb-8 border-t border-border pt-5">
            <button
              type="button"
              className="mb-2 flex min-h-12 w-full items-center justify-between gap-3 text-left"
              aria-expanded={open}
              onClick={() => {
                if (query) return;
                setOpenGroups((current) => ({ ...current, [group.id]: current[group.id] === false }));
              }}
            >
              <span className="chapter">{group.title}</span>
              <ChevronDown className={cn("size-4 shrink-0 text-muted-foreground", !open && "-rotate-90")} />
            </button>
            {open ? (
              <div className="space-y-1">
                {group.extra}
                {group.rows.map((row) => (
                  <MenuRow
                    key={row.section}
                    icon={row.icon}
                    title={row.title}
                    detail={row.detail}
                    onClick={() => setSection(row.section)}
                  />
                ))}
              </div>
            ) : null}
          </section>
        );
      })}
    </PageChrome>
  );
}

type SettingsFolder = {
  id: string;
  title: string;
  hint?: string;
  extra?: React.ReactNode;
  rows: { section: Section; icon: React.ReactNode; title: string; detail: string }[];
};

function localeDetail(locale: string | undefined, t: (text: string) => string): string {
  if (locale === "en") return t("English");
  if (locale === "es") return t("Spanish");
  if (locale === "de") return t("German");
  if (locale === "pt") return t("Portuguese");
  if (locale === "ca-ES-valencia") return t("Valencian");
  return t("Follow the phone");
}

function playerDetail(id: string | undefined, t: (text: string) => string): string {
  if (id === "musify") return "Musify";
  if (id === "youtube_music") return t("YouTube Music");
  if (id === "youtube") return "YouTube";
  if (id === "system") return t("Ask each time");
  return t("Files on this phone");
}

function MusicSettings({ settings, onChange }: { settings: Settings | null; onChange: (next: Settings) => void }) {
  const t = useT();
  const [players, setPlayers] = useState<MusicPlayerChoice[] | null>(null);

  useEffect(() => {
    if (!isTauri()) {
      setPlayers([
        { id: "local", installed: true },
        { id: "musify", installed: false },
        { id: "youtube_music", installed: false },
        { id: "youtube", installed: false },
        { id: "system", installed: true },
      ]);
      return;
    }
    void api
      .musicPlayers()
      .then((result) => setPlayers(result.players ?? []))
      .catch(() => setPlayers([{ id: "local", installed: true }, { id: "system", installed: true }]));
  }, []);

  const installed = new Set((players ?? []).filter((player) => player.installed).map((player) => player.id));
  const current = settings?.musicPlayer || "local";
  const choices = [
    {
      id: "local",
      title: t("Files on this phone"),
      body: t("Songs and playlists already on this phone. Pause and next control that player."),
    },
    {
      id: "musify",
      title: "Musify",
      body: t("Musify searches and plays the first match. This app does not download the audio. Pause and next use Musify when its playback session is active."),
    },
    {
      id: "youtube_music",
      title: t("YouTube Music"),
      body: t("Asks YouTube Music to play. If that app cannot, YouTube opens."),
    },
    {
      id: "youtube",
      title: "YouTube",
      body: t("Opens a YouTube search. A search does not start playing by itself. A video link does open in YouTube."),
    },
    {
      id: "system",
      title: t("Ask each time"),
      body: t("The phone picks a music app, or asks when more than one can play."),
    },
  ];

  return (
    <div>
      <h2 className="px-1 text-2xl font-medium tracking-tight">{t("Default music player")}</h2>
      <p className="mt-1 mb-4 px-1 text-sm leading-6 text-muted-foreground">
        {t("Only installed apps can be chosen. Files on this phone and Ask each time are always here.")}
      </p>
      {settings ? (
        <div className="space-y-2">
          {choices.map((choice) => {
            const available = choice.id === "local" || choice.id === "system" || installed.has(choice.id) || current === choice.id;
            if (!available) return null;
            const missing = choice.id !== "local" && choice.id !== "system" && players !== null && !installed.has(choice.id);
            const selected = current === choice.id;
            return (
              <button
                key={choice.id}
                type="button"
                disabled={missing}
                onClick={() => onChange({ ...settings, musicPlayer: choice.id })}
                className={cn(
                  "min-h-14 w-full border px-3 py-3 text-left",
                  selected ? "border-foreground text-foreground" : "border-border text-muted-foreground",
                  missing && "opacity-60",
                )}
              >
                <span className="block text-base text-foreground">
                  {choice.title}
                  {missing ? ` · ${t("Not installed")}` : ""}
                </span>
                <span className="mt-1 block text-sm leading-5">{choice.body}</span>
              </button>
            );
          })}
        </div>
      ) : (
        <p className="px-1 text-sm text-muted-foreground">{t("Couldn't load the budget. Go back and open Settings again.")}</p>
      )}
      <p className="mt-4 px-1 text-sm leading-6 text-muted-foreground">
        {t("Pause, next, and previous reach the player that is actually playing. Allow notification access if those commands miss Musify.")}
      </p>
    </div>
  );
}

function UpdatesSettings({ installed, available }: { installed: string; available: string }) {
  const t = useT();
  const newer = Boolean(available && installed && available !== installed);
  return (
    <div>
      <AppMark className="mb-4 w-16" alt="Grok Voice" />
      <h2 className="px-1 text-2xl font-medium tracking-tight">{t("Updates")}</h2>
      <p className="mt-1 mb-4 px-1 text-sm leading-6 text-muted-foreground">
        {t("When a newer version is published, a banner at the top of the app offers the download.")}{" "}
        {t("Nothing is installed from this screen.")}
      </p>
      <div className="border border-border px-3 py-3">
        <p className="text-base text-foreground">{installed ? `${t("Installed version")} ${installed}` : t("The installed version")}</p>
        <p className="mt-1 text-sm leading-6 text-muted-foreground">
          {newer ? t("A newer version is posted.") : t("This is the newest version this phone has checked.")}
        </p>
      </div>
    </div>
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
        {t("On-device Talk reads these pinned facts and short notes from recent local sessions. Cloud Talk does not. Wiping this does not delete notes, People, Finance, or voice keys.")}
      </p>
      <Field label={t("Pin fact")}>
        <Input value={text} onChange={(e) => setText(e.target.value)} placeholder={t("I live in Zurich")} />
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
      {confirmWipe && <p className="text-sm leading-5 text-muted-foreground">{t("Tap again. Notes, People, Finance, and voice keys stay.")}</p>}
      {error && <p className="text-sm text-foreground">{t(error)}</p>}
    </div>
  );
}

function AccessSection() {
  const t = useT();
  const [status, setStatus] = useState<AccessStatus | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [apps, setApps] = useState<InboxApp[]>([]);
  const [error, setError] = useState<string | null>(null);

  const refresh = () => {
    void api.accessStatus().then(setStatus).catch((e) => setError(errorText(e)));
  };

  useEffect(() => {
    refresh();
    void api.getSettings().then(setSettings).catch((e) => setError(errorText(e)));
    void api.inboxApps().then((rows) => setApps(rows.apps ?? [])).catch(() => setApps([]));
  }, []);

  const save = (patch: Partial<Settings>) => {
    if (!settings) return;
    const next = { ...settings, ...patch };
    setSettings(next);
    void api.saveSettings(next).then(setSettings).catch((e) => setError(errorText(e)));
  };

  const allowed = new Set(
    settings && !settings.messageAppsCustom
      ? apps.filter((app) => app.messaging).map((app) => app.packageName)
      : (settings?.messageApps ?? "").split(",").map((item) => item.trim()).filter(Boolean),
  );

  const toggleApp = (pkg: string) => {
    const next = new Set(allowed);
    if (next.has(pkg)) next.delete(pkg);
    else next.add(pkg);
    save({ messageAppsCustom: true, messageApps: [...next].join(",") });
  };

  return (
    <div className="space-y-4 pb-6">
      <h2 className="text-2xl font-medium tracking-tight">{t("Phone access")}</h2>
      <p className="text-sm leading-6 text-muted-foreground">
        {t("These are asked only when you turn them on. Notification access and Do Not Disturb open system settings. Call and text history is a separate permission and is not read until you allow it.")}
      </p>
      <p className="text-sm leading-6 text-muted-foreground">
        {t("Message text stays on this phone. It reaches a cloud model only while you are talking with that model, and only because you asked. Nothing is uploaded in the background.")}
      </p>
      <div className="flex min-h-12 items-center justify-between gap-3 border border-border px-3 py-2">
        <div>
          <p className="text-sm">{t("Read messages")}</p>
          <p className="text-xs leading-5 text-muted-foreground">{t("Off until you turn this on. Talk can then read messages from the apps you allow, and say who wrote them.")}</p>
        </div>
        <Switch checked={settings?.messageTriage === true} onCheckedChange={(on) => save({ messageTriage: on })} disabled={!settings} />
      </div>
      <div className="flex min-h-12 items-center justify-between gap-3 border border-border px-3 py-2">
        <div>
          <p className="text-sm">{t("Announce while Listen is on")}</p>
          <p className="text-xs leading-5 text-muted-foreground">{t("Off by default. A new direct message is spoken on this phone. Groups, Do Not Disturb, and the lock screen are respected.")}</p>
        </div>
        <Switch checked={settings?.announceMessages === true} onCheckedChange={(on) => save({ announceMessages: on })} disabled={!settings} />
      </div>
      <div className="flex min-h-12 items-center justify-between gap-3 border border-border px-3 py-2">
        <div>
          <p className="text-sm">{t("Hide text on the lock screen")}</p>
          <p className="text-xs leading-5 text-muted-foreground">{t("While the phone is locked, Talk says who wrote, not the words.")}</p>
        </div>
        <Switch checked={settings?.redactLocked !== false} onCheckedChange={(on) => save({ redactLocked: on })} disabled={!settings} />
      </div>
      <div className="space-y-2">
        <p className="text-sm">{t("Apps Talk may read")}</p>
        <p className="text-xs leading-5 text-muted-foreground">{t("Messaging apps are allowed until you change this list. Other notifications are ignored.")}</p>
        {apps.filter((app) => app.installed || !app.messaging).map((app) => (
          <button
            key={app.packageName}
            type="button"
            className="flex min-h-12 w-full items-center justify-between border border-border px-3 text-left text-sm"
            onClick={() => toggleApp(app.packageName)}
            disabled={!settings}
          >
            <span>
              {app.label}
              {!app.installed ? <span className="ml-2 text-xs uppercase text-muted-foreground">{t("Not installed")}</span> : null}
            </span>
            <span className="text-xs uppercase">{allowed.has(app.packageName) ? t("Allowed") : t("Blocked")}</span>
          </button>
        ))}
        <Button type="button" variant="outline" className="w-full" disabled={!settings} onClick={() => save({ messageAppsCustom: false, messageApps: "" })}>
          {t("Messaging apps only")}
        </Button>
      </div>
      <p className="text-sm leading-6 text-muted-foreground">{t("Notification access is required before any message is read.")}</p>
      <button type="button" className="flex h-12 w-full items-center justify-between border border-border px-3 text-left text-sm" onClick={() => void api.openAccessSettings("notifications").then(refresh)}>
        <span>{t("Notification access")}</span>
        <span className="text-xs uppercase">{accessWord(status?.notificationListener, t)}</span>
      </button>
      <button
        type="button"
        className="flex h-12 w-full items-center justify-between border border-border px-3 text-left text-sm"
        onClick={() => void api.requestCallAccess("place").then(setStatus).catch((e) => setError(errorText(e)))}
      >
        <span>{t("Place calls")}</span>
        <span className="text-xs uppercase">{accessWord(status?.placeCall, t)}</span>
      </button>
      <button
        type="button"
        className="flex h-12 w-full items-center justify-between border border-border px-3 text-left text-sm"
        onClick={() => void api.requestCallAccess("answer").then(setStatus).catch((e) => setError(errorText(e)))}
      >
        <span>{t("Answer calls")}</span>
        <span className="text-xs uppercase">{accessWord(status?.answerCall, t)}</span>
      </button>
      <div className="flex min-h-12 items-center justify-between gap-3 border border-border px-3 py-2">
        <div>
          <p className="text-sm">{t("Announce incoming calls")}</p>
          <p className="text-xs leading-5 text-muted-foreground">{t("Off by default. Listen says who is calling. The call is not answered unless you say so.")}</p>
        </div>
        <Switch checked={settings?.announceCalls === true} onCheckedChange={(on) => save({ announceCalls: on })} disabled={!settings} />
      </div>
      <button type="button" className="flex h-12 w-full items-center justify-between border border-border px-3 text-left text-sm" onClick={() => void api.openAccessSettings("dnd").then(refresh)}>
        <span>{t("Do Not Disturb")}</span>
        <span className="text-xs uppercase">{accessWord(status?.dnd, t)}</span>
      </button>
      <button
        type="button"
        className="flex h-12 w-full items-center justify-between border border-border px-3 text-left text-sm"
        onClick={() => void api.requestHistoryAccess().then(refresh).catch((e) => setError(errorText(e)))}
      >
        <span>{t("Call and text history")}</span>
        <span className="text-xs uppercase">{status ? `${accessWord(status.callLog, t)} / ${accessWord(status.sms, t)}` : ""}</span>
      </button>
      <button
        type="button"
        className="flex h-12 w-full items-center justify-between border border-border px-3 text-left text-sm"
        onClick={() => void api.requestAudioAccess().then(refresh).catch((e) => setError(errorText(e)))}
      >
        <span>{t("Music and audio")}</span>
        <span className="text-xs uppercase">{accessWord(status?.audio, t)}</span>
      </button>
      <p className="text-sm leading-6 text-muted-foreground">{t("Talk plays songs stored on this phone after you allow this.")}</p>
      {error && <p className="text-sm text-foreground">{t(error)}</p>}
    </div>
  );
}

function accessWord(value: string | undefined, t: (english: string) => string) {
  if (!value) return "";
  if (value === "granted") return t("Allowed");
  if (value === "denied") return t("Blocked");
  if (value === "unknown") return t("Unknown");
  return t(value);
}

let shownRestore: RestoreReport | null | undefined;

function takeRestoreSummary(): RestoreReport | null {
  if (shownRestore !== undefined) return shownRestore;
  try {
    const raw = sessionStorage.getItem("grok-restore-summary");
    shownRestore = raw ? (JSON.parse(raw) as RestoreReport) : null;
  } catch {
    shownRestore = null;
  }
  sessionStorage.removeItem("grok-restore-summary");
  return shownRestore;
}

let restoring = false;

export function applyBackupResult(report: RestoreReport) {
  if (restoring) return;
  restoring = true;
  writePersistedWeb(report.web, Boolean(report.hasWeb));
  if (report.customTheme) rememberCustomTheme(report.customTheme);
  const choice = choiceFromSettings(report.appearance || "");
  if (choice) {
    rememberAppearance(choice);
    applyAppearance(choice, prefersLight());
  }
  setAppLocale(report.locale || "");
  shownRestore = report;
  try {
    sessionStorage.setItem("grok-restore-summary", JSON.stringify(report));
  } catch {
    /* The summary is still on screen. */
  }
  window.dispatchEvent(new CustomEvent("grok-restore-summary", { detail: report }));
}

function previewBackupQuery() {
  if (typeof window === "undefined" || isTauri()) return { file: "", encrypted: false, summary: false };
  const params = new URLSearchParams(window.location.search);
  return { file: params.get("file") || "", encrypted: params.get("encrypted") === "1", summary: params.get("summary") === "1" };
}

const SAMPLE_RESTORE: RestoreReport = {
  notes: 2,
  people: 1,
  reminders: 1,
  keys: 4,
  facts: 1,
  sessions: 0,
  hasMemory: true,
  accounts: 1,
  transactions: 1,
  rules: 1,
  hasFinance: true,
  version: 2,
  encrypted: false,
  events: 1,
  alarms: 1,
  countdowns: 1,
  hasSettings: true,
  talkTurns: 0,
  hasTalk: false,
  hasUsage: false,
  hasAgenda: true,
  webKeys: 4,
  hasWeb: true,
  hasPacks: true,
  wakeFiles: 1,
  alarmFiles: 1,
  budgets: 0,
  referenced: [],
  models: ["Gemma 3 1B Instruct"],
  restored: ["settings.json", "local-memory.json", "vault"],
  errors: [],
  web: {},
  appearance: "graphite",
  customTheme: "",
  locale: "en",
};

function BackupPage() {
  const t = useT();
  const previewQuery = previewBackupQuery();
  const [passphrase, setPassphrase] = useState("");
  const [again, setAgain] = useState("");
  const [reveal, setReveal] = useState(false);
  const [openPass, setOpenPass] = useState("");
  const [revealOpen, setRevealOpen] = useState(false);
  const [fileB64, setFileB64] = useState(previewQuery.file ? "preview" : "");
  const [fileName, setFileName] = useState(previewQuery.file);
  const [fileEncrypted, setFileEncrypted] = useState(previewQuery.encrypted);
  const [preview, setPreview] = useState<BackupPreview | null>(null);
  const [report, setReport] = useState<RestoreReport | null>(previewQuery.summary ? SAMPLE_RESTORE : takeRestoreSummary);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState("");
  const [warnings, setWarnings] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const onSummary = (event: Event) => {
      const detail = (event as CustomEvent<RestoreReport>).detail;
      if (detail) setReport(detail);
    };
    window.addEventListener("grok-restore-summary", onSummary);
    return () => window.removeEventListener("grok-restore-summary", onSummary);
  }, []);
  useEffect(() => {
    if (!previewQuery.file) return;
    const node = document.querySelector(".backup-pad");
    if (!(node instanceof HTMLElement)) return;
    const show = () => node.scrollTo({ top: node.scrollHeight });
    show();
    const later = window.setTimeout(show, 120);
    return () => window.clearTimeout(later);
  }, [previewQuery.file]);

  const exportFile = async () => {
    const typed = passphrase.trim();
    if (typed && typed.length < 8) {
      setError("Use a passphrase of at least 8 characters.");
      return;
    }
    if (typed !== again.trim()) {
      setError("Type the passphrase the same way twice.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const file = await api.exportBackup(typed, readPersistedWeb());
      const ready = file.encrypted ? "Backup saved." : "Backup ready.";
      setNotice(file.shared ? "Share sheet opened. The file stays on this phone." : ready);
      setWarnings((file.warnings ?? []).filter(Boolean));
      if (!file.shared && file.dataB64) downloadBase64(file.fileName, file.dataB64);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const readFile = async (file: File | undefined) => {
    if (!file) return;
    const bytes = new Uint8Array(await file.arrayBuffer());
    const magic = String.fromCharCode(bytes[0] ?? 0, bytes[1] ?? 0, bytes[2] ?? 0, bytes[3] ?? 0);
    let binary = "";
    for (const byte of bytes) binary += String.fromCharCode(byte);
    setFileB64(btoa(binary));
    setFileName(file.name);
    setFileEncrypted(magic === "GVB1");
    setOpenPass("");
    setPreview(null);
    setConfirming(false);
    setReport(null);
    setError(null);
    setNotice("");
  };

  const review = async () => {
    setBusy(true);
    setError(null);
    try {
      const next = await api.previewBackup(fileB64, fileEncrypted ? openPass.trim() : "");
      setPreview(next);
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
      const next = await api.restoreBackup(fileB64, fileEncrypted ? openPass.trim() : "", true);
      applyBackupResult(next);
      setReport(next);
      setConfirming(false);
      setNotice("");
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const typed = passphrase.trim();
  const openTyped = openPass.trim();
  const openReady = !fileEncrypted || openTyped.length >= 8;
  const canAct = Boolean(fileB64) && !busy && openReady;
  const onPass = (value: string) => {
    setPassphrase(value);
    setConfirming(false);
  };
  const onOpen = (value: string) => {
    setOpenPass(value);
    setPreview(null);
    setConfirming(false);
  };

  if (report) return <RestoreSummary report={report} />;

  return (
    <div className="space-y-6 pb-4 landscape:grid landscape:grid-cols-2 landscape:items-start landscape:gap-8 landscape:space-y-0">
      <section className="space-y-4">
        <h2 className="text-2xl font-medium tracking-tight">{t("Backup")}</h2>
        <p className="text-sm leading-6 text-muted-foreground">
          {t("A file of everything saved on this phone, including API keys. A passphrase encrypts the file. Without one, anyone who opens the file can read the keys.")}
        </p>
        <PassphraseFields
          passphrase={passphrase}
          again={again}
          reveal={reveal}
          onPass={onPass}
          onAgain={setAgain}
          onReveal={() => setReveal((on) => !on)}
        />
        {!typed && (
          <p className="text-sm leading-6 text-foreground">{t("Leave this blank only if you accept an unencrypted file. That file contains your API keys.")}</p>
        )}
        <Button className="w-full" disabled={busy} onClick={() => void exportFile()}>
          {t("Export backup")}
        </Button>
      </section>
      <section className="space-y-4 border-t border-border pt-4 landscape:border-t-0 landscape:pt-0">
        <p className="text-[13px] tracking-[0.16em] text-muted-foreground uppercase">{t("Restore")}</p>
        <p className="text-sm leading-6 text-muted-foreground">{t("This replaces everything saved on this phone, then reloads.")}</p>
        <button
          type="button"
          className="flex min-h-12 w-full items-center rounded-sm bg-[var(--surface-2)] px-3 text-left text-base text-foreground"
          onClick={() => fileRef.current?.click()}
        >
          <span className="truncate">{fileName || t("Choose a .gvb file")}</span>
        </button>
        <input
          ref={fileRef}
          type="file"
          accept=".gvb,application/octet-stream"
          className="sr-only"
          onChange={(e) => void readFile(e.target.files?.[0])}
        />
        {fileEncrypted && (
          <label className="block">
            <span className="mb-2 block text-sm leading-5 text-foreground">{t("This file is encrypted.")}</span>
            <p className="mb-2 text-sm leading-6 text-foreground">{t("Type a passphrase of at least 8 characters to open this file.")}</p>
            <Input
              className="w-full border-b-2 border-[var(--text-tertiary)]"
              type={revealOpen ? "text" : "password"}
              autoComplete="current-password"
              value={openPass}
              onChange={(e) => onOpen(e.target.value)}
            />
            <button type="button" className="mt-1 min-h-12 px-1 text-sm text-foreground underline underline-offset-4" aria-pressed={revealOpen} onClick={() => setRevealOpen((on) => !on)}>
              {revealOpen ? t("Hide passphrase") : t("Show passphrase")}
            </button>
          </label>
        )}
        <div className="backup-actions flex flex-wrap items-center gap-3">
          <Button variant="outline" disabled={!canAct} onClick={() => void review()}>
            {t("Review")}
          </Button>
          <Button variant="destructive" className="bg-[var(--negative)] text-[var(--surface-0)]" disabled={!canAct} onClick={() => setConfirming(true)}>
            {t("Replace everything on this phone")}
          </Button>
        </div>
        {confirming && (
          <div className="space-y-3">
            <p className="text-sm leading-6 text-foreground">{t("Replace everything on this phone?")}</p>
            <div className="backup-actions flex flex-wrap items-center gap-3">
              <Button variant="destructive" className="bg-[var(--negative)] text-[var(--surface-0)]" disabled={!canAct} onClick={() => void restore()}>
                {t("Yes, replace everything")}
              </Button>
              <Button variant="outline" disabled={busy} onClick={() => setConfirming(false)}>
                {t("Cancel")}
              </Button>
            </div>
          </div>
        )}
        {preview && <BackupSummary preview={preview} />}
      </section>
      {error && <p className="text-sm text-foreground landscape:col-span-2">{t(error)}</p>}
      {notice && <p className="text-sm text-muted-foreground landscape:col-span-2">{t(notice)}</p>}
      {warnings.map((line) => (
        <p key={line} className="text-sm text-foreground landscape:col-span-2">{line}</p>
      ))}
    </div>
  );
}

function PassphraseFields({
  passphrase,
  again,
  reveal,
  onPass,
  onAgain,
  onReveal,
}: {
  passphrase: string;
  again: string;
  reveal: boolean;
  onPass: (value: string) => void;
  onAgain: (value: string) => void;
  onReveal: () => void;
}) {
  const t = useT();
  const type = reveal ? "text" : "password";
  return (
    <div className="space-y-4">
      <label className="block">
        <span className="mb-2 block text-sm leading-5 text-foreground">{t("Optional passphrase")}</span>
        <Input className="w-full border-b-2 border-[var(--text-tertiary)]" type={type} autoComplete="new-password" value={passphrase} onChange={(e) => onPass(e.target.value)} />
        <button type="button" className="mt-1 min-h-12 px-1 text-sm text-foreground underline underline-offset-4" aria-pressed={reveal} onClick={onReveal}>
          {reveal ? t("Hide passphrase") : t("Show passphrase")}
        </button>
      </label>
      <label className="block">
        <span className="mb-2 block text-sm leading-5 text-foreground">{t("Passphrase again")}</span>
        <Input className="w-full border-b-2 border-[var(--text-tertiary)]" type={type} autoComplete="new-password" value={again} onChange={(e) => onAgain(e.target.value)} />
      </label>
    </div>
  );
}

function RestoreSummary({ report }: { report: RestoreReport }) {
  const t = useT();
  return (
    <div className="space-y-4 pb-4">
      <h2 className="text-2xl font-medium tracking-tight">{t("Restored on this phone.")}</h2>
      <BackupSummary preview={report} />
      <p className="text-sm leading-6 text-foreground">{t("Talk history and voice time on this phone stay as they are.")}</p>
      {report.errors.length > 0 && (
        <div className="space-y-1">
          <p className="text-sm text-foreground">{t("Could not restore everything.")}</p>
          {report.errors.map((line) => (
            <p key={line} className="text-sm leading-6 text-foreground">{line}</p>
          ))}
        </div>
      )}
      <Button className="w-full" onClick={() => window.location.reload()}>{t("Done")}</Button>
    </div>
  );
}

function BackupSummary({ preview }: { preview: BackupPreview }) {
  const t = useT();
  const locale = activeLocale();
  const phrase = (kind: CountKind, count: number) => countPhrase(locale, kind, count);
  const models = preview.models ?? [];
  const files = (preview.referenced ?? []).filter((name) => !name.startsWith("on-device models"));
  return (
    <div className="space-y-2 text-sm leading-6 text-foreground">
      <p>
        {[
          phrase("note", preview.notes),
          phrase("person", preview.people),
          phrase("event", preview.events ?? 0),
          phrase("reminder", preview.reminders),
          phrase("alarm", preview.alarms ?? 0),
          phrase("countdown", preview.countdowns ?? 0),
        ].join(", ")}
        .
      </p>
      {preview.hasSettings && <p>{t("Settings, theme and language.")}</p>}
      {preview.hasFinance && (
        <p>
          {fill("Finance: {accounts}, {transactions}, {schedules}.", {
            accounts: phrase("account", preview.accounts ?? 0),
            transactions: phrase("transaction", preview.transactions ?? 0),
            schedules: phrase("schedule", preview.rules ?? 0),
          })}
        </p>
      )}
      {preview.hasMemory && <p>{phrase("fact", preview.facts)}.</p>}
      <p>{phrase("key", preview.keys)}.</p>
      <p>{preview.encrypted ? t("This file is encrypted.") : t("This file is not encrypted and contains API keys.")}</p>
      {models.length > 0 && (
        <p>{fill("On-device models not included: {names}.", { names: models.join(", ") })}</p>
      )}
      {files.length > 0 && <p>{t("Some files were too large to include. They are named below.")}</p>}
      {files.map((name) => (
        <p key={name}>{name}</p>
      ))}
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


function Back(_: { onClick: () => void }) {
  return null;
}


function ListenStatus() {
  const t = useT();
  const [health, setHealth] = useState<ListenHealth | null>(null);
  useEffect(() => {
    if (!isTauri()) {
      if (import.meta.env.DEV) {
        setHealth({ health: "battery", batteryUnrestricted: false, armed: true, serviceRunning: false, micOpen: false });
      }
      return;
    }
    let stop = false;
    const load = () => {
      void api.listenHealth().then((next) => {
        if (!stop) setHealth(next);
      }).catch(() => {});
    };
    load();
    const id = window.setInterval(load, 5000);
    return () => {
      stop = true;
      window.clearInterval(id);
    };
  }, []);
  if (!health) return null;
  return (
    <div className="app-card mb-6 flex items-center gap-2">
      <div className="title-line min-w-0 flex-1">
        <p className="min-w-0 text-base text-foreground">{listenTitle(health.health, t)}</p>
        <InfoTip label={t("Listen")} text={listenBody(health.health, t)} />
      </div>
      {health.health === "battery" ? (
        <button type="button" className="shrink-0 text-sm text-foreground underline underline-offset-4" onClick={() => void api.requestBatteryExemption().then(setHealth).catch(() => {})}>
          {t("Allow unrestricted battery")}
        </button>
      ) : null}
    </div>
  );
}

function listenTitle(health: string, t: (text: string) => string): string {
  if (health === "ok") return t("Listening in background: OK");
  if (health === "battery") return t("Listening in background: blocked by battery optimization");
  if (health === "mic") return t("Listening in background: microphone blocked until the app is open");
  if (health === "starting") return t("Listening in background: starting");
  if (health === "needs-app") return t("Listening in background: needs the app open");
  return t("Listening in background: off");
}

function listenBody(health: string, t: (text: string) => string): string {
  if (health === "ok") return t("Background listening looks healthy.");
  if (health === "battery") return t("Android can stop Hey Grok when the screen is off. Allow unrestricted battery use.");
  if (health === "mic") return t("The microphone is silent in the background. Open the app once so listening starts in the foreground.");
  if (health === "needs-app") return t("Hey Grok needs this app open once to arm listening again.");
  if (health === "starting") return t("Listen is on. The microphone is still starting.");
  return t("Turn Listen on to keep Hey Grok available.");
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
      <span className="grid size-6 shrink-0 place-items-center text-muted-foreground">{icon}</span>
      <p className="min-w-0 flex-1 text-base text-foreground">{title}</p>
      {detail ? <span className="max-w-[46%] truncate text-sm text-muted-foreground">{detail}</span> : null}
      <ChevronRight className="size-4 shrink-0 text-muted-foreground" />
    </RowButton>
  );
}
