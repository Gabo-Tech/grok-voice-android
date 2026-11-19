import { useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Check, ChevronLeft, ChevronRight, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api, errorText, type AuthStatus, type OfflinePlan, type PackProgress, type PermissionStatus } from "@/lib/api";
import { fill, setAppLocale, useT } from "@/lib/i18n";
import { englishSetupPlan, formatBytes } from "@/lib/stack";
import { cn } from "@/lib/utils";

const LANGS = [
  { id: "en", label: "English" },
  { id: "es", label: "Spanish" },
  { id: "de", label: "German" },
  { id: "pt", label: "Portuguese" },
  { id: "ca", label: "Valencian" },
] as const;

const STEPS = [
  { id: "language", label: "Language" },
  { id: "packs", label: "Offline pack" },
  { id: "preview", label: "Voice preview" },
  { id: "cloud", label: "Cloud key" },
] as const;

function stepIndex(raw: string | undefined): number {
  if (raw === "preview") return 2;
  if (raw === "cloud") return 3;
  if (raw === "packs" || raw === "packs-run") return 1;
  return 0;
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
  const [step, setStep] = useState(() => stepIndex(initialStep));
  const [lang, setLang] = useState<(typeof LANGS)[number]["id"]>(() => guessLang(locale || (typeof navigator === "undefined" ? "" : navigator.language)));
  const [plan, setPlan] = useState<OfflinePlan | null>(null);
  const [ready, setReady] = useState<Set<string>>(new Set());
  const [progress, setProgress] = useState<PackProgress | null>(null);
  const [doneBytes, setDoneBytes] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const [cloud, setCloud] = useState<"xai" | "openai">("xai");
  const [key, setKey] = useState("");
  const [perms, setPerms] = useState<PermissionStatus | null>(null);
  const [playing, setPlaying] = useState(false);
  const [cellular, setCellular] = useState(false);
  const resumed = useRef(false);
  const id = STEPS[step].id;

  useEffect(() => {
    let alive = true;
    const load = preview || !isTauri() ? Promise.resolve(englishSetupPlan()) : api.offlinePlan();
    void load.then((next) => {
      if (alive) setPlan(next);
    }).catch(() => {
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
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    void listen<PackProgress>("voice://pack", (event) => setProgress(event.payload)).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    if (id !== "preview" || preview || !isTauri()) return;
    void api.permissionsStatus().then(setPerms).catch(() => {});
  }, [id, preview]);

  const screenLocale = lang === "ca" ? "ca-ES-valencia" : lang;
  const pieces = plan?.pieces ?? [];
  const totalBytes = pieces.reduce((sum, piece) => sum + piece.bytes, 0);
  const currentBytes = progress && progress.total > 0 ? progress.received : 0;
  const ratio = totalBytes > 0 ? Math.min(100, Math.round(((doneBytes + currentBytes) / totalBytes) * 100)) : 0;

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
      if (isTauri()) await api.saveSetup(null, "preview");
      setStep(2);
    } catch (err) {
      setError(errorText(err));
      setFailed(true);
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (!autoResume || resumed.current || !plan || step !== 1) return;
    resumed.current = true;
    void download();
    // Resume once, after the plan for this language is known.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoResume, plan, step]);

  const remember = async (nextStep: string, withLang: boolean) => {
    if (!isTauri()) {
      if (withLang) setAppLocale(screenLocale);
      return;
    }
    const saved = await api.saveSetup(withLang ? lang : null, nextStep);
    if (withLang) setAppLocale(saved.locale || screenLocale);
  };

  const finish = async (next: "done" | "skipped") => {
    setBusy(true);
    setError(null);
    try {
      if (!preview) await remember(next, next === "skipped" && step === 0);
      onFinished(next);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const next = async () => {
    setError(null);
    if (id === "language") {
      setBusy(true);
      try {
        await remember("packs", true);
        setStep(1);
      } catch (err) {
        setError(errorText(err));
      } finally {
        setBusy(false);
      }
      return;
    }
    if (id === "packs") {
      if (failed || pieces.length === 0 || pieces.every((piece) => ready.has(piece.id))) {
        try {
          await remember("preview", false);
        } catch (err) {
          setError(errorText(err));
          return;
        }
        setStep(2);
        return;
      }
      await download();
      return;
    }
    if (id === "preview") {
      try {
        await remember("cloud", false);
      } catch (err) {
        setError(errorText(err));
        return;
      }
      setStep(3);
      return;
    }
    await finish("done");
  };

  const saveCloud = async () => {
    setBusy(true);
    setError(null);
    try {
      if (preview || !isTauri()) {
        onAuth({
          ...auth,
          hasKey: true,
          accounts: [...auth.accounts, { id: `demo-${cloud}`, provider: cloud, hint: "demo…key" }],
        });
        setKey("");
        return;
      }
      onAuth(await api.addVoiceAccount(cloud, key.trim()));
      setKey("");
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
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

  const packsReady = pieces.length === 0 || pieces.every((piece) => ready.has(piece.id));

  return (
    <div className="flex h-full flex-col px-5 py-6">
      <header className="mb-6 text-center">
        <p className="text-[11px] tracking-[0.35em] text-primary uppercase">{t("Setup")}</p>
        <h1 className="mt-1 text-lg font-semibold tracking-[0.22em]">GROK//VOICE</h1>
      </header>

      <ol className="mb-6 flex items-start gap-1">
        {STEPS.map((item, index) => {
          const state = index < step ? "done" : index === step ? "now" : "later";
          return (
            <li key={item.id} className="flex min-w-0 flex-1 flex-col items-center gap-1.5">
              <span
                className={cn(
                  "grid size-7 place-items-center rounded-full border text-xs",
                  state === "done" && "border-primary bg-primary text-primary-foreground",
                  state === "now" && "border-primary text-primary",
                  state === "later" && "border-border text-muted-foreground",
                )}
              >
                {state === "done" ? <Check className="size-3.5" /> : index + 1}
              </span>
              <span className={cn("text-center text-xs", state === "now" ? "text-foreground" : "text-muted-foreground")}>{t(item.label)}</span>
            </li>
          );
        })}
      </ol>

      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto">
        {id === "language" && (
          <>
            <p className="text-sm leading-6 text-muted-foreground">{t("This chooses the screen language. The offline voice stays English. Other languages can be added later in Voice and AI.")}</p>
            <div className="grid grid-cols-2 gap-2">
              {LANGS.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => setLang(item.id)}
                  className={cn("min-h-12 border px-2 py-2 text-sm", lang === item.id ? "border-foreground text-foreground" : "border-border text-muted-foreground")}
                >
                  {t(item.label)}
                </button>
              ))}
            </div>
            <p className="text-sm leading-6 text-muted-foreground">{t("The offline pack is English: Moonshine, SmolLM2, and Phonon. No account and no token.")}</p>
          </>
        )}

        {id === "packs" && plan && (
          <>
            <p className="text-sm leading-6 text-muted-foreground">
              {t("Use Wi-Fi if you can. These files stay on the phone. If the download stops, setup continues from here.")}
            </p>
            {cellular ? <p className="text-sm leading-6 text-foreground">{t("You appear to be on mobile data. Wi-Fi is a better way to download this.")}</p> : null}
            <p className="text-base text-foreground">{fill("About {size}", { size: formatBytes(totalBytes) })}</p>
            <p className="text-sm leading-6 text-muted-foreground">{t("The offline pack is English: Moonshine, SmolLM2, and Phonon. No account and no token.")}</p>
            <ul className="divide-y divide-border border-y border-border">
              {pieces.map((piece) => (
                <li key={piece.id} className="flex items-center justify-between gap-3 py-3 text-sm">
                  <span>{t(piece.label)}</span>
                  <span className="text-muted-foreground">{ready.has(piece.id) ? t("Downloaded") : formatBytes(piece.bytes)}</span>
                </li>
              ))}
            </ul>
            {totalBytes > 0 ? (
              <div className="h-2 bg-border" role="progressbar" aria-valuenow={ratio} aria-valuemin={0} aria-valuemax={100}>
                <div className="h-full bg-foreground" style={{ width: `${ratio}%` }} />
              </div>
            ) : null}
            {failed ? (
              <Button type="button" variant="outline" className="w-full" disabled={busy} onClick={() => void download()}>
                {t("Retry download")}
              </Button>
            ) : null}
          </>
        )}

        {id === "preview" && (
          <>
            <p className="text-sm leading-6 text-muted-foreground">{t("The offline pack is English: Moonshine, SmolLM2, and Phonon. No account and no token.")}</p>
            <Button type="button" className="w-full" disabled={playing || busy} onClick={() => void play()}>
              {playing ? t("Playing") : t("Play this voice")}
            </Button>
            <p className="text-sm leading-6 text-muted-foreground">{t("Setup downloads the English voice. Add Spanish, German, Piper, or a token model later in Voice and AI.")}</p>
            <p className="text-sm leading-6 text-muted-foreground">{t("Allow the microphone before the first talk. You can also do this later.")}</p>
            <Button type="button" variant="outline" className="w-full" disabled={busy || perms?.microphone === "granted"} onClick={() => void allowMic()}>
              {perms?.microphone === "granted" ? t("Microphone allowed") : t("Allow microphone")}
            </Button>
            <p className="text-sm leading-6 text-muted-foreground">{t("Hey Grok can be trained later under Wake word.")}</p>
            {packsReady ? <p className="text-sm text-foreground">{t("This phone is ready to talk with no network.")}</p> : null}
          </>
        )}

        {id === "cloud" && (
          <>
            <p className="text-sm leading-6 text-muted-foreground">
              {t("Add a cloud key if you want one. You can skip this and add it later in Voice and AI.")}
            </p>
            <div className="grid grid-cols-2 gap-2">
              {(["xai", "openai"] as const).map((item) => (
                <button key={item} type="button" onClick={() => setCloud(item)} className={cn("min-h-12 border text-sm", cloud === item ? "border-foreground" : "border-border text-muted-foreground")}>
                  {item === "xai" ? "xAI" : "OpenAI"}
                </button>
              ))}
            </div>
            <Input type="password" autoComplete="off" value={key} placeholder={cloud === "xai" ? "xai-…" : "sk-…"} onChange={(event) => setKey(event.target.value)} />
            <Button type="button" variant="outline" className="w-full" disabled={busy || key.trim().length < 12} onClick={() => void saveCloud()}>
              {t("Save this key")}
            </Button>
            <p className="text-sm leading-6 text-muted-foreground">{t("A cloud key is optional. The phone can answer without one after the offline pack is downloaded.")}</p>
          </>
        )}
      </div>

      {error ? (
        <p role="alert" className="mt-3 text-sm leading-6 text-foreground">
          {t(error)}
        </p>
      ) : null}
      {failed && id === "packs" ? <p className="mt-2 text-sm leading-6 text-muted-foreground">{t("Download stopped. What is already on the phone stays. Try again.")}</p> : null}

      <footer className="mt-4 flex flex-col gap-2 pt-2">
        <div className="flex gap-2">
          <Button variant="outline" disabled={step === 0 || busy} onClick={() => setStep((value) => Math.max(0, value - 1))}>
            <ChevronLeft /> {t("Back")}
          </Button>
          <Button className="flex-1" disabled={busy || (id === "packs" && !plan)} onClick={() => void next()}>
            {busy ? <Loader2 className="animate-spin" /> : <ChevronRight />}
            {id === "cloud" ? t("Start talking") : id === "packs" && failed ? t("Continue anyway") : id === "packs" && !packsReady ? t("Download the offline pack") : t("Next")}
          </Button>
        </div>
        <Button type="button" variant="outline" className="w-full" disabled={busy} onClick={() => void finish("skipped")}>
          {t("Finish later")}
        </Button>
        <p className="text-center text-xs leading-5 text-muted-foreground">{t("Setup can wait. Voice and AI in Settings finishes the download.")}</p>
      </footer>
    </div>
  );
}
