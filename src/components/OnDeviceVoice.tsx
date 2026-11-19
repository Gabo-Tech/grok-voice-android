import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import { LocalPacks } from "@/components/LocalPacks";
import { api, errorText, type PackProgress, type Settings, type VoiceSetupStatus } from "@/lib/api";
import { useT } from "@/lib/i18n";
import voices from "@/data/phonon-voices.json";

const LANGS = [
  { id: "en", label: "English" },
  { id: "es", label: "Spanish" },
  { id: "de", label: "German" },
  { id: "pt", label: "Portuguese" },
  { id: "ca", label: "Valencian" },
] as const;

const ENGINES = ["auto", "phonon", "piper", "android"] as const;

function formatBytes(bytes: number) {
  const mb = bytes / (1024 * 1024);
  if (mb >= 10) return `${Math.round(mb)} MB`;
  if (mb >= 1) return `${mb.toFixed(1)} MB`;
  if (bytes <= 0) return "";
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

function setPair(map: string | undefined, lang: string, value: string) {
  const order = LANGS.map((item) => item.id);
  const pairs = new Map<string, string>();
  for (const part of (map ?? "").split(",")) {
    const [key, stored] = part.split("=");
    if (key && stored) pairs.set(key, stored);
  }
  if (value) pairs.set(lang, value);
  else pairs.delete(lang);
  return order
    .filter((id) => pairs.has(id))
    .map((id) => `${id}=${pairs.get(id)}`)
    .join(",");
}

function pairValue(map: string | undefined, lang: string, fallback = "") {
  for (const part of (map ?? "").split(",")) {
    const [key, value] = part.split("=");
    if (key === lang && value) return value;
  }
  return fallback;
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

export function OnDeviceVoice({
  settings,
  onSettings,
  embedded = false,
}: {
  settings: Settings;
  onSettings: (next: Settings) => void;
  /** Inside Voice and AI. Hearing and the language model live on that page. */
  embedded?: boolean;
}) {
  const t = useT();
  const [status, setStatus] = useState<VoiceSetupStatus | null>(null);
  const [lang, setLang] = useState(settings.localSpeech || "en");
  const [progress, setProgress] = useState<PackProgress | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [token, setToken] = useState("");
  const [playing, setPlaying] = useState(false);

  const refresh = () => {
    if (!isTauri()) return;
    void api
      .onDeviceVoice()
      .then((next) => {
        setStatus(next);
        setError(null);
      })
      .catch((err) => setError(t(errorText(err))));
  };

  useEffect(() => {
    if (!isTauri()) return;
    refresh();
    let unlisten: (() => void) | undefined;
    void listen<PackProgress>("voice://pack", (event) => setProgress(event.payload)).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    refresh();
  }, [settings.speechEngines, settings.phononModels, settings.phononVoices, settings.localSpeech]);

  const current = status?.langs.find((item) => item.id === lang);
  const engineChoice = pairValue(settings.speechEngines, lang, "auto");
  const modelChoice = pairValue(settings.phononModels, lang, current?.modelId || "");
  const voiceChoice = pairValue(settings.phononVoices, lang, "");
  const models = (status?.models ?? []).filter((model) => model.langs.includes(lang));
  const chosenModel = models.find((model) => model.id === (modelChoice || current?.modelId)) ?? models[0];
  const phonon = (engineChoice === "phonon" || engineChoice === "auto") && lang !== "ca";
  const catalog = voices.filter((voice) => voice.lang === lang || (lang === "en" && voice.locale === "characters"));
  const ratio =
    progress && chosenModel && progress.id === chosenModel.id && progress.total > 0
      ? Math.min(100, Math.round((progress.received / progress.total) * 100))
      : null;

  const save = (patch: Partial<Settings>) => {
    const next = { ...settings, ...patch };
    onSettings(next);
  };

  const chooseLang = (id: string) => {
    setLang(id);
    if (id === "en" || id === "es" || id === "de") save({ localSpeech: id });
  };

  const chooseEngine = (choice: string) => {
    save({ speechEngines: setPair(settings.speechEngines, lang, choice) });
  };

  const chooseModel = (id: string) => {
    save({ phononModels: setPair(settings.phononModels, lang, id) });
  };

  const chooseVoice = async (id: string) => {
    save({ phononVoices: setPair(settings.phononVoices, lang, id) });
    if (!id || status?.voicesReady.includes(id)) return;
    setBusy(`voice-${id}`);
    setError(null);
    try {
      setStatus(await api.downloadPhononVoice(id));
    } catch (err) {
      setError(t(errorText(err)));
    } finally {
      setBusy(null);
    }
  };

  const downloadModel = async (id: string) => {
    setBusy(id);
    setError(null);
    setProgress(null);
    try {
      setStatus(await api.downloadPhononModel(id));
    } catch (err) {
      setError(t(errorText(err)));
    } finally {
      setBusy(null);
      setProgress(null);
    }
  };

  const removeModel = async (id: string) => {
    setBusy(id);
    setError(null);
    try {
      setStatus(await api.deletePhononModel(id));
    } catch (err) {
      setError(t(errorText(err)));
    } finally {
      setBusy(null);
    }
  };

  const downloadPiper = async () => {
    const id = lang === "de" ? "tts-de" : lang === "es" ? "tts-es" : "tts";
    if (lang !== "en" && lang !== "es" && lang !== "de") return;
    setBusy(id);
    setError(null);
    setProgress(null);
    try {
      await api.downloadLocalPack(id);
      refresh();
    } catch (err) {
      setError(t(errorText(err)));
    } finally {
      setBusy(null);
      setProgress(null);
    }
  };

  const saveToken = async () => {
    setBusy("token");
    setError(null);
    try {
      setStatus(await api.saveHfToken(token.trim()));
      setToken("");
    } catch (err) {
      setError(t(errorText(err)));
    } finally {
      setBusy(null);
    }
  };

  const clearToken = async () => {
    setBusy("token");
    setError(null);
    try {
      setStatus(await api.clearHfToken());
    } catch (err) {
      setError(t(errorText(err)));
    } finally {
      setBusy(null);
    }
  };

  const testVoice = async () => {
    setPlaying(true);
    setError(null);
    try {
      const audio = await api.previewOnDeviceVoice(lang);
      if (audio.wavBase64) await playWav(audio.wavBase64);
    } catch (err) {
      setError(t(errorText(err)));
    } finally {
      setPlaying(false);
    }
  };

  return (
    <div className="space-y-6 pb-8">
      {embedded ? null : (
      <div>
        <h2 className="px-1 text-2xl font-medium tracking-tight">{t("On-device voice")}</h2>
        <p className="mt-1 px-1 text-sm leading-6 text-muted-foreground">
          {t("Pick a language, then an engine, a voice, and a download. Nothing is fetched until you tap Download.")}
        </p>
      </div>
      )}

      <section className="border border-border px-3 py-3">
        <p className="text-sm text-muted-foreground">{t("At a glance")}</p>
        <p className="mt-1 text-base leading-6 text-foreground">{current ? t(current.note) : t("Will use the Android voice.")}</p>
        {current?.resolved === "phonon" && chosenModel ? (
          <p className="mt-1 text-sm leading-5 text-muted-foreground">
            {t(chosenModel.label)} · {chosenModel.license}
            {current.voiceId ? "" : ` · ${t("Default voice")}`}
          </p>
        ) : null}
        {current?.resolved === "piper" && current.piperName ? (
          <p className="mt-1 text-sm leading-5 text-muted-foreground">{t(current.piperName)}</p>
        ) : null}
      </section>

      <section>
        <p className="mb-2 px-1 text-sm text-muted-foreground">{t("1. Language")}</p>
        <div className="grid grid-cols-2 gap-2">
          {LANGS.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => chooseLang(item.id)}
              className={`min-h-12 border px-2 py-2 text-sm ${lang === item.id ? "border-foreground text-foreground" : "border-border text-muted-foreground"}`}
            >
              {t(item.label)}
            </button>
          ))}
        </div>
        <p className="mt-2 px-1 text-sm leading-6 text-muted-foreground">
          {lang === "pt" || lang === "ca"
            ? t("Portuguese and Valencian have no listening pack. Talk still listens with the language set under Local speech.")
            : t("Listening for Talk uses this language when it is English, Spanish, or German.")}
        </p>
      </section>

      <section>
        <p className="mb-2 px-1 text-sm text-muted-foreground">{t("2. Engine")}</p>
        <div className="grid grid-cols-2 gap-2">
          {ENGINES.map((choice) => (
            <button
              key={choice}
              type="button"
              disabled={(choice === "phonon" && lang === "ca") || (choice === "piper" && (lang === "pt" || lang === "ca"))}
              onClick={() => chooseEngine(choice)}
              className={`min-h-12 border px-2 py-2 text-sm disabled:opacity-40 ${engineChoice === choice ? "border-foreground text-foreground" : "border-border text-muted-foreground"}`}
            >
              {t(choice === "auto" ? "Auto" : choice === "piper" ? "Piper" : choice === "android" ? "Android voice" : "Phonon")}
            </button>
          ))}
        </div>
        <p className="mt-2 px-1 text-sm leading-6 text-muted-foreground">
          {t("Auto uses Phonon when that language has a model on this phone. Otherwise it uses Piper, then the Android voice.")}
        </p>
      </section>

      {phonon ? (
        <section>
          <p className="mb-2 px-1 text-sm text-muted-foreground">{t("3. Voice")}</p>
          <div className="grid grid-cols-2 gap-2">
            <button
              type="button"
              onClick={() => void chooseVoice("")}
              className={`min-h-12 border px-2 py-2 text-left text-sm ${voiceChoice === "" ? "border-foreground text-foreground" : "border-border text-muted-foreground"}`}
            >
              <span className="block">{t("Default voice")}</span>
              <span className="block text-xs leading-4">{t("Comes with the model.")}</span>
            </button>
          </div>
          <div className="mt-2 max-h-64 space-y-2 overflow-y-auto">
            {catalog.map((voice) => {
              const ready = status?.voicesReady.includes(voice.id) ?? false;
              const selected = voiceChoice === voice.id;
              return (
                <button
                  key={voice.id}
                  type="button"
                  disabled={busy !== null}
                  onClick={() => void chooseVoice(voice.id)}
                  className={`flex min-h-12 w-full items-center justify-between gap-3 border px-3 py-2 text-left text-sm ${selected ? "border-foreground text-foreground" : "border-border text-muted-foreground"}`}
                >
                  <span>
                    <span className="block">{voice.name}</span>
                    <span className="block text-xs leading-4">
                      {t(voice.gender === "male" ? "Male" : "Female")}
                      {voice.locale ? ` · ${voice.locale}` : ""}
                    </span>
                  </span>
                  <span className="shrink-0 text-xs">{ready ? t("On this phone") : t("Not downloaded")}</span>
                </button>
              );
            })}
          </div>
        </section>
      ) : (
        <section>
          <p className="mb-2 px-1 text-sm text-muted-foreground">{t("3. Voice")}</p>
          <p className="px-1 text-sm leading-6 text-foreground">{current?.piperName ? t(current.piperName) : t("Will use the Android voice.")}</p>
        </section>
      )}

      <section className="space-y-3">
        <p className="px-1 text-sm text-muted-foreground">{t("4. Download")}</p>
        {phonon
          ? models.map((model) => (
              <div key={model.id} className="border border-border px-3 py-3">
                <button type="button" onClick={() => chooseModel(model.id)} className="w-full text-left">
                  <p className={`text-base ${(modelChoice || chosenModel?.id) === model.id ? "text-foreground" : "text-muted-foreground"}`}>{t(model.label)}</p>
                  <p className="mt-1 text-sm leading-5 text-muted-foreground">{t(model.detail)}</p>
                  <p className="mt-1 text-sm text-muted-foreground">
                    {model.license} · {model.gated ? t("Gated model") : t("Public model")} ·{" "}
                    {formatBytes(model.ready && model.installedBytes > 0 ? model.installedBytes : model.downloadBytes)} ·{" "}
                    {model.ready ? t("On this phone") : t("Not downloaded")}
                  </p>
                </button>
                {ratio !== null && progress?.id === model.id ? <Progress value={ratio} className="mt-3" /> : null}
                <div className="mt-3 flex flex-col gap-2">
                  <Button type="button" className="w-full" disabled={busy !== null} onClick={() => void downloadModel(model.id)}>
                    {busy === model.id ? t("Downloading") : model.ready ? t("Download again") : t("Download")}
                  </Button>
                  {model.ready ? (
                    <Button type="button" variant="outline" className="w-full" disabled={busy !== null} onClick={() => void removeModel(model.id)}>
                      {t("Delete")}
                    </Button>
                  ) : null}
                </div>
              </div>
            ))
          : null}
        {engineChoice === "android" && (lang === "en" || lang === "es" || lang === "de") ? (
          <p className="px-1 text-sm leading-6 text-muted-foreground">
            {t("The Android voice speaks alerts. Talk on this phone needs Phonon or Piper to answer.")}
          </p>
        ) : null}
        {(lang === "en" || lang === "es" || lang === "de") && engineChoice !== "android" ? (
          <Button type="button" variant="outline" className="w-full" disabled={busy !== null || current?.piperReady} onClick={() => void downloadPiper()}>
            {current?.piperReady ? t("On this phone") : t("Download Piper")}
          </Button>
        ) : null}
        {lang !== "ca" ? (
          <div className="space-y-2 pt-2">
            <p className="px-1 text-sm text-muted-foreground">{t("Hugging Face token")}</p>
            <p className="px-1 text-sm leading-6 text-muted-foreground">
              {t(
                "Optional. Used only to download a gated Phonon model. It is stored on this phone like the other keys. English Phonon 31m does not need a token.",
              )}
            </p>
            <p className="px-1 text-sm text-foreground">{status?.hfHasToken ? `${t("Token saved")} · ${status.hfHint}` : t("No token saved")}</p>
            <Input value={token} type="password" autoComplete="off" onChange={(event) => setToken(event.target.value)} />
            <div className="flex flex-col gap-2">
              <Button type="button" className="w-full" disabled={busy !== null || token.trim().length < 8} onClick={() => void saveToken()}>
                {t("Save token")}
              </Button>
              {status?.hfHasToken ? (
                <Button type="button" variant="outline" className="w-full" disabled={busy !== null} onClick={() => void clearToken()}>
                  {t("Remove token")}
                </Button>
              ) : null}
            </div>
          </div>
        ) : null}
      </section>

      <section>
        <p className="mb-2 px-1 text-sm text-muted-foreground">{t("5. Test")}</p>
        <Button type="button" className="w-full" disabled={playing || busy !== null} onClick={() => void testVoice()}>
          {playing ? t("Playing") : t("Test this voice")}
        </Button>
      </section>

      {error ? (
        <p role="alert" className="px-1 text-sm leading-6 text-foreground">
          {error}
        </p>
      ) : null}

      <p className="px-1 text-sm leading-6 text-muted-foreground">
        {t(
          "Phonon is by Gradium. It builds on Pocket TTS and the Mimi codec by Kyutai. The runtime is MIT or Apache 2.0. Each model keeps the license shown here.",
        )}
      </p>

      {embedded ? null : (
      <div className="space-y-3 border-t border-border pt-4">
        <p className="px-1 text-sm leading-6 text-muted-foreground">
          {t("Speech to text and the language model are below. Moonshine, SmolLM2, and the Piper packs still download the same way.")}
        </p>
        <LocalPacks showHeading />
      </div>
      )}
    </div>
  );
}
