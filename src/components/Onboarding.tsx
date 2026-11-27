import { useEffect, useRef, useState, type ReactNode } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Check, ChevronLeft, ChevronRight, ClipboardPaste, Eye, EyeOff } from "lucide-react";
import { AppMark } from "@/components/AppMark";
import { Loader } from "@/components/Loader";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api, errorText, type AuthStatus, type OfflinePlan, type PackProgress, type PermissionStatus, type PlanPiece } from "@/lib/api";
import { fill, setAppLocale, useT } from "@/lib/i18n";
import { safeHttps } from "@/lib/links";
import {
  KEY_GUIDES,
  PATHS,
  PROGRESS,
  adviceFor,
  formatAdvice,
  memorySentence,
  pathFromScreen,
  progressIndex,
  reportedPhoneBytes,
  screenFor,
  type SetupPath,
  type SetupScreen,
} from "@/lib/setupGuide";
import { englishSetupPlan, formatBytes } from "@/lib/stack";
import { cn } from "@/lib/utils";

const LANGS = [
  { id: "en", label: "English" },
  { id: "es", label: "Spanish" },
  { id: "de", label: "German" },
  { id: "pt", label: "Portuguese" },
  { id: "ca", label: "Valencian" },
] as const;

const CLOUD: Array<"xai" | "openai" | "onemin"> = ["xai", "openai", "onemin"];

function askedKeyCheck(): "ok" | "fail" | "" {
  if (typeof window === "undefined") return "";
  const value = new URLSearchParams(window.location.search).get("keyCheck") || "";
  return value === "ok" || value === "fail" ? value : "";
}

function guessLang(locale: string): (typeof LANGS)[number]["id"] {
  const tag = locale.trim().toLowerCase().replace(/_/g, "-");
  if (tag.includes("valencia") || tag === "ca" || tag.startsWith("ca-")) return "ca";
  if (tag.startsWith("pt")) return "pt";
  if (tag.startsWith("de")) return "de";
  if (tag.startsWith("es")) return "es";
  return "en";
}

function playWav(wavBase64: string) {
  const binary = atob(wavBase64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  const url = URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
  const audio = new Audio(url);
  audio.onended = () => URL.revokeObjectURL(url);
  return audio.play();
}

async function openGuideLink(url: string) {
  if (!safeHttps(url)) return;
  if (isTauri()) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
}

function Step({ n, title, children }: { n: number; title: string; children: ReactNode }) {
  const t = useT();
  return (
    <section className="space-y-2">
      <h2 className="chapter">
        {n}. {t(title)}
      </h2>
      <div className="space-y-3 text-sm leading-6 text-muted-foreground">{children}</div>
    </section>
  );
}

function PackList({ pieces, ready }: { pieces: PlanPiece[]; ready: Set<string> }) {
  const t = useT();
  return (
    <ul className="divide-y divide-border border-y border-border">
      {pieces.map((piece) => (
        <li key={piece.id} className="flex min-h-12 items-center justify-between gap-3 text-sm text-foreground">
          <span>{t(piece.label)}</span>
          <span className="text-muted-foreground">{ready.has(piece.id) ? t("Downloaded") : formatBytes(piece.bytes)}</span>
        </li>
      ))}
    </ul>
  );
}

export function Onboarding({
  auth,
  preview,
  initialStep,
  autoResume = false,
  locale = "",
  onAuth,
  onFinished,
}: {
  auth: AuthStatus;
  preview?: boolean;
  initialStep?: string;
  autoResume?: boolean;
  locale?: string;
  onAuth: (auth: AuthStatus) => void;
  onFinished: (step: "done" | "skipped") => void;
}) {
  const t = useT();
  const demoFailed = typeof window !== "undefined" && new URLSearchParams(window.location.search).get("guide") === "error";
  const [screen, setScreen] = useState<SetupScreen>(() => screenFor(demoFailed ? "local" : initialStep));
  const [path, setPath] = useState<SetupPath>(() => pathFromScreen(screenFor(initialStep)));
  const [lang, setLang] = useState<(typeof LANGS)[number]["id"]>(() => guessLang(locale || (typeof navigator === "undefined" ? "" : navigator.language)));
  const [plan, setPlan] = useState<OfflinePlan | null>(null);
  const [ready, setReady] = useState<Set<string>>(new Set());
  const [progress, setProgress] = useState<PackProgress | null>(null);
  const [doneBytes, setDoneBytes] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(
    demoFailed ? "Download stopped. What is already on the phone stays. Try again." : askedKeyCheck() === "fail" ? "The key was refused." : null,
  );
  const [failed, setFailed] = useState(demoFailed);
  const [cloud, setCloud] = useState<"xai" | "openai" | "onemin">(() => {
    const opened = screenFor(initialStep);
    return opened === "openai" || opened === "onemin" ? opened : "xai";
  });
  const [key, setKey] = useState(() => (askedKeyCheck() === "fail" ? "xai-refused-demo-key" : ""));
  const [keySaved, setKeySaved] = useState(() => askedKeyCheck() === "ok");
  const [keyCheck, setKeyCheck] = useState<"idle" | "checking" | "works" | "failed" | "unsaved">(() => {
    const asked = askedKeyCheck();
    if (asked === "ok") return "works";
    if (asked === "fail") return "failed";
    return "idle";
  });
  const [showKey, setShowKey] = useState(false);
  const [perms, setPerms] = useState<PermissionStatus | null>(null);
  const [playing, setPlaying] = useState(false);
  const [cellular, setCellular] = useState(false);
  const resumed = useRef(false);
  const advice = adviceFor(reportedPhoneBytes());
  const sized = formatAdvice(advice);

  useEffect(() => {
    let alive = true;
    const load = preview || !isTauri() ? Promise.resolve(englishSetupPlan()) : api.offlinePlan();
    void load
      .then((next) => {
        if (alive) setPlan(next);
      })
      .catch(() => {
        if (alive) setPlan(englishSetupPlan());
      });
    return () => {
      alive = false;
    };
  }, [preview]);

  useEffect(() => {
    const connection = (navigator as Navigator & { connection?: { type?: string } }).connection;
    setCellular(connection?.type === "cellular");
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    if (auth.accounts.some((account) => account.provider === "local")) return;
    void api.addVoiceAccount("local", "on-device").then(onAuth).catch(() => {});
  }, [auth.accounts, onAuth]);

  useEffect(() => {
    if (!askedKeyCheck()) return;
    const id = window.setTimeout(() => document.querySelector("[data-key-check]")?.scrollIntoView({ block: "nearest" }), 200);
    return () => window.clearTimeout(id);
  }, [screen]);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    void listen<PackProgress>("voice://pack", (event) => setProgress(event.payload)).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    if (screen !== "test" || preview || !isTauri()) return;
    void api.permissionsStatus().then(setPerms).catch(() => {});
  }, [screen, preview]);

  const screenLocale = lang === "ca" ? "ca-ES-valencia" : lang;
  const pieces = plan?.pieces ?? [];
  const totalBytes = pieces.reduce((sum, piece) => sum + piece.bytes, 0);
  const currentBytes = progress && progress.total > 0 ? progress.received : 0;
  const ratio = totalBytes > 0 ? Math.min(100, Math.round(((doneBytes + currentBytes) / totalBytes) * 100)) : 0;
  const packsReady = pieces.length > 0 && pieces.every((piece) => ready.has(piece.id));
  const needsPacks = path === "local" || path === "mixed" || path === "onemin" || screen === "local";

  const refreshReady = async () => {
    if (!isTauri()) return new Set<string>();
    const packs = await api.localPacks();
    const voice = await api.onDeviceVoice();
    const ids = new Set<string>();
    for (const pack of packs) {
      if (pack.ready) ids.add(pack.id);
    }
    for (const model of voice.models) {
      if (model.ready) ids.add(model.id);
    }
    setReady(ids);
    return ids;
  };

  const remember = async (nextStep: string, withLang: boolean) => {
    if (!isTauri()) {
      if (withLang) setAppLocale(screenLocale);
      return;
    }
    const saved = await api.saveSetup(withLang ? lang : null, nextStep);
    if (withLang) setAppLocale(saved.locale || screenLocale);
  };

  const download = async () => {
    if (!plan) return;
    setBusy(true);
    setError(null);
    setFailed(false);
    setProgress(null);
    try {
      if (isTauri()) {
        const saved = await api.saveSetup(lang, "packs-run");
        setAppLocale(saved.locale || screenLocale);
      } else {
        setAppLocale(screenLocale);
      }
      const have = await refreshReady();
      let completed = 0;
      for (const piece of pieces) {
        if (have.has(piece.id)) {
          completed += piece.bytes;
          setDoneBytes(completed);
          continue;
        }
        let attempt = 0;
        for (;;) {
          try {
            if (!isTauri()) {
              await new Promise((resolve) => window.setTimeout(resolve, 250));
            } else if (piece.kind === "phonon") {
              await api.downloadPhononModel(piece.id);
            } else {
              await api.downloadLocalPack(piece.id);
            }
            break;
          } catch (err) {
            attempt += 1;
            if (attempt >= 3) {
              setError(errorText(err));
              setFailed(true);
              setBusy(false);
              return;
            }
          }
        }
        have.add(piece.id);
        completed += piece.bytes;
        setDoneBytes(completed);
        setProgress(null);
      }
      setReady(new Set(have));
      if (isTauri()) await api.saveSetup(null, path === "mixed" || path === "onemin" ? path : "local");
    } catch (err) {
      setError(errorText(err));
      setFailed(true);
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (!autoResume || resumed.current || !plan || screen !== "local") return;
    resumed.current = true;
    void download();
    // Resume once, after the plan is known.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoResume, plan, screen]);

  const finish = async (next: "done" | "skipped") => {
    setBusy(true);
    setError(null);
    try {
      if (!preview && isTauri()) {
        if (next === "done") {
          await remember("done", false);
        } else {
          const current = await api.getSettings();
          const step = screen === "test" ? "test" : screen;
          await api.saveSettings({
            ...current,
            onboarded: true,
            setupStep: step,
            locale: screen === "language" ? screenLocale : current.locale,
          });
          if (screen === "language") setAppLocale(screenLocale);
        }
      } else if (!preview && screen === "language") {
        setAppLocale(screenLocale);
      }
      onFinished(next);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const go = async (next: SetupScreen, savedStep: string, withLang = false) => {
    setError(null);
    setBusy(true);
    try {
      await remember(savedStep, withLang);
      if (isTauri() && (next === "local" || next === "mixed" || next === "xai" || next === "openai" || next === "onemin")) {
        const current = await api.getSettings();
        await api.saveSettings({ ...current, preferLocalOnWake: next === "local" || next === "mixed", setupStep: savedStep });
      }
      if (next === "xai" || next === "openai" || next === "onemin" || next === "mixed" || next === "local") setPath(next);
      setScreen(next);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const cloudProvider = (): "xai" | "openai" | "onemin" => {
    if (screen === "openai" || screen === "onemin" || screen === "xai") return screen;
    return cloud;
  };

  const rememberAccount = async (provider: "xai" | "openai" | "onemin", status: AuthStatus) => {
    onAuth(status);
    if (!isTauri() || screen === "mixed") return;
    const list = (await api.authStatus()).accounts;
    const index = list.findIndex((account) => account.provider === provider);
    if (index > 0) {
      const ids = list.map((account) => account.id);
      const [kept] = ids.splice(index, 1);
      ids.unshift(kept);
      onAuth(await api.reorderVoiceAccounts(ids));
    }
  };

  const demoAccount = (provider: "xai" | "openai" | "onemin"): AuthStatus => ({
    ...auth,
    hasKey: true,
    accounts: [...auth.accounts.filter((account) => account.provider !== provider), { id: `demo-${provider}`, provider, hint: "demo…key" }],
  });

  const checkKey = async () => {
    const provider = cloudProvider();
    const secret = key.trim();
    if (secret.length < 12) {
      setKeySaved(false);
      setKeyCheck("failed");
      setError(KEY_GUIDES[provider].short);
      return;
    }
    setBusy(true);
    setError(null);
    setKeyCheck("checking");
    try {
      if (preview || !isTauri()) {
        const forced = new URLSearchParams(window.location.search).get("keyCheck") === "fail";
        if (forced || /fail|refused|bad/i.test(secret)) throw new Error("The key was refused.");
        onAuth(demoAccount(provider));
      } else {
        await api.probeVoiceAccount(provider, secret);
        await rememberAccount(provider, await api.commitVoiceAccount(provider, secret));
      }
      setKey("");
      setKeySaved(true);
      setKeyCheck("works");
    } catch (err) {
      setKeySaved(false);
      setKeyCheck("failed");
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const saveAnyway = async () => {
    const provider = cloudProvider();
    const secret = key.trim();
    if (secret.length < 12) {
      setError(KEY_GUIDES[provider].short);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      if (preview || !isTauri()) onAuth(demoAccount(provider));
      else await rememberAccount(provider, await api.commitVoiceAccount(provider, secret));
      setKey("");
      setKeySaved(true);
      setKeyCheck("unsaved");
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const pasteKey = async () => {
    try {
      const text = (await navigator.clipboard.readText()).trim();
      if (!text) return;
      setKey(text);
      setKeySaved(false);
      setKeyCheck("idle");
      setError(null);
    } catch {
      /* the field stays as it is when the clipboard is closed */
    }
  };

  const allowMic = async () => {
    setBusy(true);
    setError(null);
    try {
      if (preview || !isTauri()) {
        setPerms({ microphone: "granted", notifications: "granted" });
        return;
      }
      setPerms(await api.requestPermissions());
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const play = async () => {
    setPlaying(true);
    setError(null);
    try {
      if (!isTauri()) return;
      const audio = await api.previewOnDeviceVoice("en");
      if (audio.wavBase64) await playWav(audio.wavBase64);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setPlaying(false);
    }
  };

  const back = () => {
    setError(null);
    if (screen === "path") setScreen("language");
    else if (screen === "test") setScreen(path);
    else setScreen("path");
  };

  const next = async () => {
    if (screen === "language") {
      await go("path", "path", true);
      return;
    }
    if (screen === "path") {
      await go(path, path);
      return;
    }
    if (screen === "test") {
      await finish("done");
      return;
    }
    if ((screen === "local" || screen === "mixed" || screen === "onemin") && !packsReady && !failed && pieces.length > 0) {
      await download();
      return;
    }
    if ((screen === "xai" || screen === "openai") && !keySaved) return;
    if ((screen === "mixed" || screen === "onemin") && key.trim().length > 0 && !keySaved) return;
    await go("test", "test");
  };

  const index = progressIndex(screen);
  const guide = cloud === "openai" || cloud === "onemin" || cloud === "xai" ? KEY_GUIDES[cloud] : KEY_GUIDES.xai;
  const activeKey = screen === "xai" || screen === "openai" || screen === "onemin" ? KEY_GUIDES[screen] : guide;
  const sentence = memorySentence(advice);
  const memoryPairs = { ...sentence.pairs };
  if (memoryPairs.fit) memoryPairs.fit = t(memoryPairs.fit);
  const memoryLine = fill(sentence.template, memoryPairs);

  const packBlock = (
    <>
      <p>{t("Use Wi-Fi. If the download stops, what is already here stays, and setup continues from this screen.")}</p>
      {cellular ? <p className="text-foreground">{t("You appear to be on mobile data. Wi-Fi is a better way to download this.")}</p> : null}
      <p className="text-base text-foreground">{fill("The first download is about {size}. It is Moonshine, SmolLM2, and Phonon.", { size: formatBytes(totalBytes || advice.storageBytes) })}</p>
      <PackList pieces={pieces} ready={ready} />
      {totalBytes > 0 ? <Loader size="block" value={ratio} label={t("Downloading")} /> : null}
      {failed ? (
        <Button type="button" variant="outline" className="w-full" disabled={busy} onClick={() => void download()}>
          {t("Retry download")}
        </Button>
      ) : null}
    </>
  );

  const keyBlock = (which: "xai" | "openai" | "onemin") => {
    const item = KEY_GUIDES[which];
    return (
      <>
        <p>{t(item.save)}</p>
        <div className="setup-key">
          <div className="min-w-0">
            <Input
              type={showKey ? "text" : "password"}
              autoComplete="off"
              autoCapitalize="off"
              spellCheck={false}
              value={key}
              placeholder={item.placeholder}
              aria-label={t(which === "xai" ? "Paste the xAI key" : which === "onemin" ? "Paste the 1min.ai key" : "Paste the OpenAI key")}
              onChange={(event) => {
                setKey(event.target.value);
                setKeySaved(false);
                setKeyCheck("idle");
                setError(null);
              }}
            />
          </div>
          <div className="setup-key-tools">
            <button type="button" className="grid size-12 shrink-0 place-items-center" aria-label={t("Paste")} onClick={() => void pasteKey()}>
              <ClipboardPaste className="size-5" aria-hidden />
            </button>
            <button
              type="button"
              className="grid size-12 shrink-0 place-items-center"
              aria-label={showKey ? t("Hide key") : t("Show key")}
              aria-pressed={showKey}
              onClick={() => setShowKey((open) => !open)}
            >
              {showKey ? <EyeOff className="size-5" aria-hidden /> : <Eye className="size-5" aria-hidden />}
            </button>
          </div>
        </div>
        {key.trim().length > 0 && key.trim().length < 12 ? <p className="text-foreground">{t(item.short)}</p> : null}
        <Button type="button" className="w-full" data-key-check={keyCheck} disabled={busy || key.trim().length < 12} onClick={() => void checkKey()}>
          {keyCheck === "checking" ? t("Checking the key…") : t("Check key")}
        </Button>
        {keyCheck === "works" ? (
          <p role="status" className="text-foreground">{t("Key works.")}</p>
        ) : null}
        {keyCheck === "unsaved" ? (
          <p role="status" className="text-foreground">{t("Saved without a check.")}</p>
        ) : null}
        {keyCheck === "failed" && key.trim().length >= 12 ? (
          <Button type="button" variant="outline" className="w-full" disabled={busy} onClick={() => void saveAnyway()}>
            {t("Save anyway")}
          </Button>
        ) : null}
      </>
    );
  };

  let primary = t("Next");
  if (screen === "test") primary = t("Start talking");
  else if (screen === "local" || screen === "mixed" || screen === "onemin") {
    if (failed) primary = t("Continue anyway");
    else if (!packsReady && pieces.length > 0) primary = t("Download the offline pack");
    else primary = t("Test this setup");
  } else if (screen === "xai" || screen === "openai") primary = t("Test this setup");

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden px-5 py-5 landscape:flex-row landscape:gap-8 landscape:px-6 landscape:py-4" data-setup={screen}>
      <header className="mb-5 min-w-0 shrink-0 landscape:mb-0 landscape:flex landscape:max-h-full landscape:w-52 landscape:max-w-[42%] landscape:flex-col landscape:overflow-auto">
        <AppMark className="mb-4 w-16 landscape:mb-3 landscape:w-14" alt="" />
        <p className="text-[13px] tracking-[0.18em] text-muted-foreground uppercase">{t("Setup")}</p>
        <h1 className="masthead mt-2 text-3xl break-words landscape:text-2xl">Grok Voice</h1>
        <ol className="setup-steps mt-4 landscape:mt-6">
          {PROGRESS.map((item, itemIndex) => {
            const state = itemIndex < index ? "done" : itemIndex === index ? "now" : "later";
            return (
              <li key={item} className="setup-step-item">
                <span
                  className={cn("setup-step grid place-items-center text-sm", state === "now" ? "bg-foreground text-background" : "text-muted-foreground")}
                >
                  {state === "done" ? <Check className="size-4" /> : itemIndex + 1}
                </span>
                <span className={cn("text-center text-[13px] leading-snug landscape:text-left", state === "now" ? "text-foreground" : "text-muted-foreground")}>{t(item)}</span>
              </li>
            );
          })}
        </ol>
      </header>

      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pb-3">
          {screen === "language" && (
            <>
              <p className="text-sm leading-6 text-muted-foreground">{t("This sets the screen language. You can change it later in Settings. The first offline voice is English until you add another in Voice and AI.")}</p>
              <div className="grid grid-cols-2 gap-2">
                {LANGS.map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() => setLang(item.id)}
                    className={cn("min-h-12 px-3 text-left text-sm", lang === item.id ? "bg-foreground text-background" : "text-muted-foreground")}
                  >
                    {t(item.label)}
                  </button>
                ))}
              </div>
              <p className="text-sm leading-6 text-muted-foreground">{t("Come back to this screen from Settings, Voice and AI, whenever you want.")}</p>
            </>
          )}

          {screen === "path" && (
            <>
              <h2 className="text-2xl font-medium tracking-tight">{t("How should Talk answer?")}</h2>
              <p className="text-sm leading-6 text-muted-foreground">{t("Pick one path. You can add the others later in Voice and AI.")}</p>
              <div className="flex flex-col">
                {PATHS.map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() => setPath(item.id)}
                    className={cn("min-h-12 px-3 py-2 text-left", path === item.id ? "bg-foreground text-background" : "text-foreground")}
                  >
                    <span className="block text-base">{t(item.title)}</span>
                    <span className={cn("mt-1 block text-sm leading-5", path === item.id ? "text-background" : "text-muted-foreground")}>{t(item.blurb)}</span>
                  </button>
                ))}
              </div>
              <p className="text-sm leading-6 text-muted-foreground">{t("Each path explains where to get a key, what it costs or needs, how to test it, and what to do if it fails.")}</p>
            </>
          )}

          {screen === "local" && (
            <>
              <h2 className="text-2xl font-medium tracking-tight">{t("On this phone")}</h2>
              <Step n={1} title="What it needs">
                <p>{t("Nothing to buy. The files download into this phone. No account and no token.")}</p>
                <p>{t("Moonshine turns speech into text. llama.cpp runs the chat model. Phonon or Piper speaks.")}</p>
              </Step>
              <Step n={2} title="Memory">
                <p className="text-foreground">{memoryLine}</p>
                <p>{fill("Piper English is a separate file of about {piper}. Phonon speaks first and needs no account. You can switch to Piper in Voice and AI.", { piper: sized.piper })}</p>
              </Step>
              <Step n={3} title="Storage">
                {packBlock}
              </Step>
              <Step n={4} title="Test it">
                <p>{t("Play the voice. Then open Talk and say hello with the network off. A reply means the phone can answer alone.")}</p>
              </Step>
              <Step n={5} title="If this fails">
                <p>{t("If the download stops, try it again. What is already on the phone stays. If a model does not fit, keep SmolLM2 or add a cloud key. If there is no reply, check that Moonshine, the chat model, and Phonon or Piper are on this phone.")}</p>
              </Step>
            </>
          )}

          {(screen === "xai" || screen === "openai" || screen === "onemin") && (
            <>
              <h2 className="text-2xl font-medium tracking-tight">{t(screen === "xai" ? "xAI" : screen === "openai" ? "OpenAI" : "1min.ai")}</h2>
              <p className="text-sm leading-6 text-muted-foreground">{t(PATHS.find((item) => item.id === screen)?.blurb || "")}</p>
              <Step n={1} title="Get the key">
                <p>{t(activeKey.where)}</p>
                <button type="button" className="min-h-12 text-left text-sm text-foreground underline" onClick={() => void openGuideLink(activeKey.url)}>
                  {t("Open the key page")}
                </button>
              </Step>
              <Step n={2} title="What it needs">
                <p>{t(activeKey.cost)}</p>
              </Step>
              <Step n={3} title="Save the key">
                {keyBlock(screen)}
              </Step>
              {screen === "onemin" ? (
                <Step n={4} title="Hearing and voice">
                  <p>{t("1min.ai still needs this phone to hear you and to speak. Download the offline pack before the first talk.")}</p>
                  {packBlock}
                </Step>
              ) : null}
              <Step n={screen === "onemin" ? 5 : 4} title="Test it">
                <p>{t(activeKey.test)}</p>
              </Step>
              <Step n={screen === "onemin" ? 6 : 5} title="If this fails">
                <p>{t(activeKey.error)}</p>
              </Step>
            </>
          )}

          {screen === "mixed" && (
            <>
              <h2 className="text-2xl font-medium tracking-tight">{t("Phone, then a cloud key")}</h2>
              <Step n={1} title="What it needs">
                <p>{t("Two parts. The phone answers when it can. A cloud key answers when the phone cannot.")}</p>
                <p>{t("Download the offline pack first. Then save one cloud key: xAI, OpenAI, or 1min.ai.")}</p>
                <p>{t("Talk tries this phone first when the packs are ready. If a pack is missing, the model will not load, or memory is low, Talk uses the cloud key.")}</p>
                <p className="text-foreground">{memoryLine}</p>
              </Step>
              <Step n={2} title="On this phone">
                {packBlock}
              </Step>
              <Step n={3} title="Cloud fallback">
                <p>{t("Which cloud key")}</p>
                <div className="grid grid-cols-3 gap-2">
                  {CLOUD.map((item) => (
                    <button
                      key={item}
                      type="button"
                      onClick={() => {
                        setCloud(item);
                        setKeySaved(false);
                        setKeyCheck("idle");
                      }}
                      className={cn("min-h-12 px-2 text-sm", cloud === item ? "bg-foreground text-background" : "text-muted-foreground")}
                    >
                      {t(item === "xai" ? "xAI" : item === "openai" ? "OpenAI" : "1min.ai")}
                    </button>
                  ))}
                </div>
                <p>{t(guide.where)}</p>
                <button type="button" className="min-h-12 text-left text-sm text-foreground underline" onClick={() => void openGuideLink(guide.url)}>
                  {t("Open the key page")}
                </button>
                <p>{t(guide.cost)}</p>
                {keyBlock(cloud)}
              </Step>
              <Step n={4} title="Test it">
                <p>{t("Play the voice with the network off. Then turn the network on and say hello again. If the packs are missing, the cloud key should answer instead.")}</p>
              </Step>
              <Step n={5} title="If this fails">
                <p>{t("If the phone is silent, finish the download. If the cloud key is silent, check that key and the network. The order in Voice and AI is the order Talk uses.")}</p>
              </Step>
            </>
          )}

          {screen === "test" && (
            <>
              <h2 className="text-2xl font-medium tracking-tight">{t("Try it")}</h2>
              <p className="text-sm leading-6 text-muted-foreground">
                {path === "local"
                  ? t("Play the voice. Then open Talk and say hello with the network off. A reply means the phone can answer alone.")
                  : path === "mixed"
                    ? t("Play the voice with the network off. Then turn the network on and say hello again. If the packs are missing, the cloud key should answer instead.")
                    : t(KEY_GUIDES[path === "openai" || path === "onemin" || path === "xai" ? path : "xai"].test)}
              </p>
              {needsPacks ? (
                <Button type="button" className="w-full" disabled={playing || busy} onClick={() => void play()}>
                  {playing ? t("Playing") : t("Play this voice")}
                </Button>
              ) : null}
              <p className="text-sm leading-6 text-muted-foreground">{t("Allow the microphone before the first talk. You can also do this later.")}</p>
              <Button type="button" variant="outline" className="w-full" disabled={busy || perms?.microphone === "granted"} onClick={() => void allowMic()}>
                {perms?.microphone === "granted" ? t("Microphone allowed") : t("Allow microphone")}
              </Button>
              <p className="text-sm leading-6 text-muted-foreground">{t("Hey Grok can be trained later under Wake word.")}</p>
              <p className="text-sm leading-6 text-foreground">{t("A reply in Talk means this path works. You can change it later in Voice and AI.")}</p>
              {path === "local" && (packsReady || pieces.length === 0) ? <p className="text-sm text-foreground">{t("This phone is ready to talk with no network.")}</p> : null}
              <Step n={1} title="If this fails">
                <p>
                  {path === "local"
                    ? t("If the download stops, try it again. What is already on the phone stays. If a model does not fit, keep SmolLM2 or add a cloud key. If there is no reply, check that Moonshine, the chat model, and Phonon or Piper are on this phone.")
                    : path === "mixed"
                      ? t("If the phone is silent, finish the download. If the cloud key is silent, check that key and the network. The order in Voice and AI is the order Talk uses.")
                      : t(KEY_GUIDES[path === "openai" || path === "onemin" ? path : "xai"].error)}
                </p>
              </Step>
            </>
          )}
        </div>

        {error ? (
          <p role="alert" className="mt-3 text-sm leading-6 text-foreground">
            {t(error)}
          </p>
        ) : null}

        <footer className="mt-3 flex shrink-0 flex-col gap-2 border-t border-border pt-3">
          <div className="setup-actions">
            <Button variant="outline" disabled={screen === "language" || busy} onClick={back}>
              <ChevronLeft /> {t("Back")}
            </Button>
            <Button
              className="flex-1"
              disabled={
                busy ||
                ((screen === "local" || screen === "mixed" || screen === "onemin") && !plan) ||
                ((primary === t("Test this setup") || primary === t("Continue anyway")) &&
                  (((screen === "xai" || screen === "openai") && !keySaved) ||
                    ((screen === "mixed" || screen === "onemin") && key.trim().length > 0 && !keySaved)))
              }
              onClick={() => void next()}
            >
              {busy ? <Loader size="inline" quiet label={primary} /> : <ChevronRight />}
              {primary}
            </Button>
          </div>
          {(screen === "mixed" || screen === "onemin") && primary !== t("Test this setup") ? (
            <Button type="button" variant="outline" className="w-full" disabled={busy} onClick={() => void go("test", "test")}>
              {t("Skip this key")}
            </Button>
          ) : null}
          <Button type="button" variant="outline" className="w-full" disabled={busy} onClick={() => void finish("skipped")}>
            {t("Finish later")}
          </Button>
          <p className="text-center text-[13px] leading-5 text-muted-foreground">{t("Setup can wait. Open Settings, then Voice and AI, and continue from the same path.")}</p>
        </footer>
      </div>
    </div>
  );
}
