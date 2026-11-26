import { useEffect, useState, type ReactNode } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { ChevronDown } from "lucide-react";
import { LocalPacks, previewModelPacks } from "@/components/LocalPacks";
import { OnDeviceVoice } from "@/components/OnDeviceVoice";
import { VoiceOrder } from "@/components/VoiceOrder";
import { VoicePicker } from "@/components/VoicePicker";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api, errorText, type AuthStatus, type LocalPack, type OneMinModel, type Settings, type VoiceSetupStatus } from "@/lib/api";
import { attemptOrder, packsReadyForTalk, providerLabel } from "@/lib/flow";
import { fitLabel } from "@/lib/fit";
import { useT } from "@/lib/i18n";
import { OPENAI_VOICES, XAI_VOICES, voiceLabel } from "@/lib/voices";
import { cn } from "@/lib/utils";

const HEARING = [
  { id: "en", label: "English", how: "Moonshine" },
  { id: "es", label: "Spanish", how: "NeMo" },
  { id: "de", label: "German", how: "NeMo" },
] as const;

function pairValue(map: string | undefined, lang: string, fallback = "") {
  for (const part of (map ?? "").split(",")) {
    const [key, value] = part.split("=");
    if (key === lang && value) return value;
  }
  return fallback;
}

export function VoiceHome({
  settings,
  auth,
  onAuth,
  onSettings,
  onOpenTour,
}: {
  settings: Settings;
  auth: AuthStatus;
  onAuth: (auth: AuthStatus) => void;
  onSettings: (next: Settings) => void;
  onOpenTour: () => void;
}) {
  const t = useT();
  const [open, setOpen] = useState<"hearing" | "brain" | "voice" | "order" | "advanced" | null>(askedSection());
  const [packs, setPacks] = useState<LocalPack[]>(() => (!isTauri() && import.meta.env.DEV ? previewModelPacks() : []));
  const [voiceStatus, setVoiceStatus] = useState<VoiceSetupStatus | null>(null);
  const [provider, setProvider] = useState<"xai" | "openai" | "local" | "onemin">(askedProvider);
  const [secret, setSecret] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [modelsOpen, setModelsOpen] = useState(askedSection() === "brain");
  const [oneminModels, setOneminModels] = useState<OneMinModel[]>(askedSection() ? previewOneminModels() : []);
  const [modelWarn, setModelWarn] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;
    void api.localPacks().then(setPacks).catch(() => {});
    void api.onDeviceVoice().then(setVoiceStatus).catch(() => {});
    void api.oneminModels(false).then(setOneminModels).catch(() => setOneminModels([]));
  }, [settings.speechEngines, settings.phononModels, settings.localSpeech, settings.locale]);

  const accounts = auth.accounts ?? [];
  const packsReady = packsReadyForTalk(packs);
  const order = attemptOrder(
    accounts.map((account) => account.provider),
    Boolean(settings.preferLocalOnWake),
    packsReady,
  );
  const first = order.length > 0 ? accounts[order[0]] : undefined;
  const primary = first?.provider ?? "local";
  const speech = settings.localSpeech || "en";
  const engine = pairValue(settings.speechEngines, speech, "auto");
  const hearingHow = speech === "es" || speech === "de" ? "NeMo" : "Moonshine";
  const brainName = packs.find((pack) => pack.selected)?.label || "SmolLM2";
  const voiceName =
    primary === "openai"
      ? voiceLabel(settings.openaiVoice || "marin")
      : primary === "xai"
        ? voiceLabel(settings.voice || "eve")
        : engine === "piper"
          ? "Piper"
          : engine === "android"
            ? "Android voice"
            : engine === "phonon"
              ? "Phonon"
              : "Phonon";
  const sttId = speech === "es" ? "stt-es" : speech === "de" ? "stt-de" : "stt";
  const sttReady = packs.some((pack) => pack.id === sttId && pack.ready);
  const llmReady = packs.some((pack) => (pack.kind === "llm" || pack.kind === "custom") && pack.selected && pack.ready);
  const langStatus = voiceStatus?.langs.find((item) => item.id === speech);
  const voiceReady = Boolean(langStatus?.phononReady || langStatus?.piperReady || engine === "android");
  const offlineReady = primary === "local" ? sttReady && llmReady && voiceReady && engine !== "android" : sttReady && llmReady && voiceReady;
  const downloaded = sttReady && llmReady && (engine === "android" || voiceReady);
  const cloudKey = accounts.some((account) => account.provider === "xai" || account.provider === "openai" || account.provider === "onemin");
  const textBrain = primary === "local" || primary === "onemin";
  const unfinished = settings.setupStep === "skipped" || settings.setupStep === "language" || settings.setupStep === "packs" || settings.setupStep === "packs-run";

  const toggle = (id: typeof open) => setOpen((current) => (current === id ? null : id));

  const save = (patch: Partial<Settings>) => {
    onSettings({ ...settings, ...patch });
  };

  const ensureLocal = async () => {
    if (accounts.some((account) => account.provider === "local")) return auth;
    try {
      const status = await api.addVoiceAccount("local", "on-device");
      onAuth(status);
      return status;
    } catch (err) {
      const text = errorText(err);
      if (text.toLowerCase().includes("already")) {
        const status = await api.authStatus();
        onAuth(status);
        return status;
      }
      throw err;
    }
  };

  const focusLocal = async () => {
    setBusy(true);
    setError(null);
    try {
      if (!isTauri()) {
        save({ preferLocalOnWake: true });
        return;
      }
      const status = await ensureLocal();
      const list = status.accounts ?? [];
      const index = list.findIndex((account) => account.provider === "local");
      if (index > 0) {
        const ids = list.map((account) => account.id);
        const [id] = ids.splice(index, 1);
        ids.unshift(id);
        onAuth(await api.reorderVoiceAccounts(ids));
      }
      save({ preferLocalOnWake: true });
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const focusCloud = async (id: "xai" | "openai" | "onemin") => {
    setProvider(id);
    setOpen("order");
    if (!isTauri()) {
      save({ preferLocalOnWake: false });
      return;
    }
    const index = accounts.findIndex((account) => account.provider === id);
    if (index < 0) {
      setEditing(null);
      setSecret("");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      if (index > 0) {
        const ids = accounts.map((account) => account.id);
        const [kept] = ids.splice(index, 1);
        ids.unshift(kept);
        onAuth(await api.reorderVoiceAccounts(ids));
      }
      save({ preferLocalOnWake: false });
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const saveKey = async () => {
    setBusy(true);
    setError(null);
    try {
      const value = provider === "local" ? "on-device" : secret;
      if (!isTauri()) {
        onAuth({
          ...auth,
          hasKey: true,
          accounts: [
            ...accounts,
            { id: `demo-${provider}-${accounts.length}`, provider, hint: provider === "local" ? "on this phone" : "demo…key" },
          ],
        });
        setSecret("");
        return;
      }
      const status = editing
        ? await api.updateVoiceAccount(editing, provider, value)
        : await api.addVoiceAccount(provider, value);
      setSecret("");
      setEditing(null);
      onAuth(status);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  const hearingValue =
    textBrain ? `${t(hearingHow)} · ${t(speech === "de" ? "German" : speech === "es" ? "Spanish" : "English")}` : t(providerLabel(primary));
  const brainValue = primary === "onemin"
    ? settings.oneminModel || "gpt-4.1-mini"
    : primary === "openai"
      ? settings.openaiModel || "gpt-realtime"
      : primary === "xai"
        ? settings.model || "grok-voice-latest"
        : t(brainName === "SmolLM2 135M Instruct" ? "SmolLM2" : brainName);
  const voiceValue = t(voiceName);
  const shot = typeof window === "undefined" ? "" : new URLSearchParams(window.location.search).get("view") || "";
  if (import.meta.env.DEV && shot === "packs") {
    return (
      <div className="bg-background px-4 py-4 text-foreground">
        <LocalPacks showHeading={false} part="models" />
      </div>
    );
  }
  if (import.meta.env.DEV && shot === "onemin") {
    return (
      <div className="bg-background px-4 py-4 text-foreground">
        <h2 className="text-2xl font-medium tracking-tight">{t("1min.ai")}</h2>
        <p className="mt-1 text-sm leading-6 text-muted-foreground">{t("Paste the 1min.ai key")}</p>
        <Input className="mt-3" type="password" autoComplete="off" value="saved-on-this-phone" readOnly />
        <div className="mt-4">
          <OneminModels hint models={previewOneminModels()} value={settings.oneminModel || "gpt-4.1-mini"} onPick={(id) => save({ oneminModel: id })} onRefresh={() => undefined} />
        </div>
      </div>
    );
  }
  if (import.meta.env.DEV && shot === "picker") {
    return (
      <div className="bg-background px-4 py-4 text-foreground">
        <h2 className="text-2xl font-medium tracking-tight">{t("Model")}</h2>
        <ModelChoices
          settings={settings}
          packs={packs}
          primary={primary}
          onSettings={save}
          onFocus={(id) => {
            if (id === "local") void focusLocal();
            else void focusCloud(id);
          }}
          onUsePack={(id) => {
            setPacks((current) => current.map((pack) => ({ ...pack, selected: pack.id === id })));
            if (isTauri()) void api.selectLocalModel(id).then(setPacks).catch((err) => setError(errorText(err)));
          }}
          oneminModels={oneminModels.length > 0 ? oneminModels : previewOneminModels()}
          onRefreshModels={() => undefined}
        />
      </div>
    );
  }

  return (
    <div className="space-y-4 pb-8">
      <div>
        <h2 className="px-1 text-2xl font-medium tracking-tight">{t("Voice and AI")}</h2>
        <p className="mt-1 px-1 text-sm leading-6 text-muted-foreground">
          {t("Hearing, the model, the voice, and what is tried next.")}
        </p>
      </div>

      <ul className="flex flex-wrap gap-x-4 gap-y-1 px-1 text-sm text-muted-foreground">
        <li>{offlineReady ? t("Offline-ready") : t("Not offline yet")}</li>
        <li>{downloaded ? t("Downloaded") : t("Not downloaded")}</li>
        <li>{cloudKey ? t("Cloud key saved") : t("No cloud key")}</li>
        {voiceStatus?.hfHasToken ? <li>{t("Token saved")}</li> : null}
      </ul>

      {unfinished ? (
        <Button type="button" variant="outline" className="w-full whitespace-normal" onClick={onOpenTour}>
          {t("Finish offline setup")}
        </Button>
      ) : null}

      <Section
        title={t("Hearing")}
        value={hearingValue}
        open={open === "hearing"}
        onToggle={() => toggle("hearing")}
      >
        <div className="grid grid-cols-1 gap-2">
          <button type="button" className={cn("min-h-12 border px-3 text-left text-sm", primary === "local" && "border-foreground")} onClick={() => void focusLocal()}>
            {t("Hear on this phone")}
          </button>
          <button type="button" className={cn("min-h-12 border px-3 text-left text-sm", primary === "onemin" && "border-foreground")} onClick={() => void focusCloud("onemin")}>
            {t("Use 1min.ai")}
          </button>
          <button type="button" className={cn("min-h-12 border px-3 text-left text-sm", primary === "xai" && "border-foreground")} onClick={() => void focusCloud("xai")}>
            {t("Use xAI")}
          </button>
          <button type="button" className={cn("min-h-12 border px-3 text-left text-sm", primary === "openai" && "border-foreground")} onClick={() => void focusCloud("openai")}>
            {t("Use OpenAI")}
          </button>
        </div>
        {textBrain ? (
          <>
            <div className="mt-3 grid grid-cols-3 gap-2">
              {HEARING.map((choice) => (
                <button
                  key={choice.id}
                  type="button"
                  onClick={() => save({ localSpeech: choice.id })}
                  className={cn("min-h-12 border px-2 text-sm", speech === choice.id ? "border-foreground text-foreground" : "border-border text-muted-foreground")}
                >
                  {t(choice.label)}
                </button>
              ))}
            </div>
            <p className="mt-2 text-sm leading-6 text-muted-foreground">
              {t("English uses Moonshine Base. Spanish and German each use a separate download. Moonshine does not understand Spanish or German.")}
            </p>
            <p className="mt-2 text-sm leading-6 text-muted-foreground">
              {t("Setup downloads the English voice. Add Spanish, German, Piper, or a token model later in Voice and AI.")}
            </p>
            <div className="mt-3">
              <LocalPacks showHeading={false} part="stt" onChanged={() => void api.localPacks().then(setPacks).catch(() => {})} />
            </div>
          </>
        ) : (
          <p className="mt-3 text-sm leading-6 text-muted-foreground">
            {t("Cloud hears you, thinks, and speaks. On this phone is still in the list.")}
          </p>
        )}
      </Section>

      <Section title={t("Brain")} value={brainValue} open={open === "brain"} onToggle={() => toggle("brain")}>
        <ModelChoices
          settings={settings}
          packs={packs}
          primary={primary}
          onSettings={save}
          onFocus={(id) => {
            if (id === "local") void focusLocal();
            else void focusCloud(id);
          }}
          onUsePack={(id) => {
            if (!isTauri()) {
              setPacks((current) => current.map((pack) => ({ ...pack, selected: pack.id === id })));
              return;
            }
            void api.selectLocalModel(id).then(setPacks).catch((err) => setError(errorText(err)));
          }}
          oneminModels={oneminModels}
          onRefreshModels={() => {
            void api.oneminModels(true).then(setOneminModels).catch((err) => setError(errorText(err)));
          }}
        />
        <Button type="button" variant="outline" className="mt-3 w-full" onClick={() => setModelsOpen((value) => !value)}>
          {modelsOpen ? t("Hide the other models") : t("Show the other models")}
        </Button>
        {modelsOpen ? <LocalPacks showHeading={false} part="models" onChanged={() => void api.localPacks().then(setPacks).catch(() => {})} /> : null}
        <Button
          type="button"
          variant="outline"
          className="mt-3 w-full"
          onClick={() => {
            if (!modelWarn) {
              setModelWarn(true);
              return;
            }
            setModelWarn(false);
            save({ model: "grok-voice-latest", openaiModel: "gpt-realtime", oneminModel: "gpt-4.1-mini" });
            void api.selectLocalModel("llm").then(setPacks).catch(() => {});
          }}
        >
          {modelWarn ? t("Tap again to put the models back to the defaults.") : t("Use the defaults")}
        </Button>
      </Section>

      <Section title={t("Voice")} value={voiceValue} open={open === "voice"} onToggle={() => toggle("voice")}>
        {primary === "xai" ? (
          <VoicePicker voices={XAI_VOICES} value={settings.voice || "eve"} preview onChange={(id) => save({ voice: id })} />
        ) : primary === "openai" ? (
          <VoicePicker voices={OPENAI_VOICES} value={settings.openaiVoice || "marin"} onChange={(id) => save({ openaiVoice: id })} />
        ) : (
          <OnDeviceVoice embedded settings={settings} onSettings={onSettings} />
        )}
      </Section>

      <Section
        title={t("If this fails")}
        value={accounts.length > 1 ? accounts.map((account) => t(providerLabel(account.provider))).join(" · ") : t(providerLabel(primary))}
        open={open === "order"}
        onToggle={() => toggle("order")}
      >
        <p className="text-sm leading-6 text-muted-foreground">
          {t("Talk tries the top row first. On this phone hears you, thinks, and speaks without a network. A cloud row does all three with its key.")}
        </p>
        <div className="mt-3 grid grid-cols-2 gap-2">
          {(["local", "onemin", "xai", "openai"] as const).map((id) => (
            <button
              key={id}
              type="button"
              onClick={() => {
                setProvider(id);
                setEditing(null);
                setSecret("");
              }}
              className={cn("min-h-12 border px-2 text-sm", provider === id ? "border-foreground text-foreground" : "border-border text-muted-foreground")}
            >
              {t(providerLabel(id))}
            </button>
          ))}
        </div>
        {provider === "local" ? (
          <p className="mt-3 text-sm leading-6 text-muted-foreground">{t("No key. Talk uses this phone when the packs below are ready. If a pack is missing or the model cannot load, the next key is used.")}</p>
        ) : (
          <label className="mt-3 block">
            <span className="mb-2 block text-sm text-muted-foreground">{provider === "xai" ? t("Paste the xAI key") : provider === "onemin" ? t("Paste the 1min.ai key") : t("Paste the OpenAI key")}</span>
            <Input type="password" autoComplete="off" value={secret} placeholder={editing ? t("Leave blank to keep the saved key") : provider === "xai" ? "xai-…" : "sk-…"} onChange={(event) => setSecret(event.target.value)} />
          </label>
        )}
        <Button type="button" className="mt-3 w-full" disabled={busy || (provider !== "local" && secret.trim().length < 12 && !editing)} onClick={() => void saveKey()}>
          {provider === "local" ? t("Use on device") : t("Save this key")}
        </Button>
        {provider === "onemin" ? (
          <div className="mt-4">
            <p className="mb-2 text-sm text-muted-foreground">{t("1min.ai model")}</p>
            <OneminModels
              hint
              models={oneminModels}
              value={settings.oneminModel || "gpt-4.1-mini"}
              onPick={(id) => save({ oneminModel: id })}
              onRefresh={() => {
                void api.oneminModels(true).then(setOneminModels).catch((err) => setError(errorText(err)));
              }}
            />
          </div>
        ) : null}
        <div className="mt-4">
          <VoiceOrder
            accounts={accounts}
            onEdit={(id, current) => {
              setEditing(id);
              setSecret("");
              if (current === "xai" || current === "openai" || current === "local") setProvider(current);
            }}
            onRemove={(id) => {
              void api.removeVoiceAccount(id).then(onAuth).catch((err) => setError(errorText(err)));
            }}
            onReorder={async (ids) => {
              onAuth(await api.reorderVoiceAccounts(ids));
            }}
          />
        </div>
        <button
          type="button"
          disabled={busy}
          onClick={() => save({ preferLocalOnWake: !settings.preferLocalOnWake })}
          className={cn("mt-3 flex min-h-12 w-full items-center justify-between border px-3 text-left text-sm", settings.preferLocalOnWake ? "border-foreground" : "border-border text-muted-foreground")}
        >
          <span>{t("Prefer On device when packs are ready")}</span>
          <span>{settings.preferLocalOnWake ? t("On") : t("Off")}</span>
        </button>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          {t("Off follows the list. On uses On device first for Talk when its packs are on this phone and On device is in this list.")}
        </p>
      </Section>

      <Section title={t("Advanced")} value={t("Set up offline voice")} open={open === "advanced"} onToggle={() => toggle("advanced")}>
        <Button type="button" variant="outline" className="w-full whitespace-normal" onClick={onOpenTour}>
          {t("Set up offline voice")}
        </Button>
        <p className="mt-3 text-sm leading-6 text-muted-foreground">
          {t("A cloud key is optional. The phone can answer without one after the offline pack is downloaded.")}
        </p>
      </Section>

      {error ? (
        <p role="alert" className="px-1 text-sm leading-6 text-foreground">
          {t(error)}
        </p>
      ) : null}
    </div>
  );
}

function Section({
  title,
  value,
  open,
  onToggle,
  children,
}: {
  title: string;
  value: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <section className="border border-border">
      <button type="button" className="flex min-h-12 w-full items-center gap-3 px-3 py-3 text-left" aria-expanded={open} onClick={onToggle}>
        <span className="min-w-0 flex-1">
          <span className="block text-sm text-muted-foreground">{title}</span>
          <span className="mt-0.5 block text-base text-foreground">{value}</span>
        </span>
        <ChevronDown className={cn("size-4 shrink-0", open && "rotate-180")} />
      </button>
      {open ? <div className="border-t border-border px-3 py-3">{children}</div> : null}
    </section>
  );
}

function ModelChoices({
  settings,
  packs,
  primary,
  onSettings,
  onFocus,
  onUsePack,
  oneminModels,
  onRefreshModels,
}: {
  settings: Settings;
  packs: LocalPack[];
  primary: string;
  onSettings: (patch: Partial<Settings>) => void;
  onFocus: (id: "local" | "onemin" | "xai" | "openai") => void;
  onUsePack: (id: string) => void;
  oneminModels: OneMinModel[];
  onRefreshModels: () => void;
}) {
  const t = useT();
  const downloaded = packs.filter((pack) => (pack.kind === "llm" || pack.kind === "custom") && pack.ready);
  const brains = [
    ["local", "On this phone"],
    ["onemin", "1min.ai"],
    ["xai", "xAI"],
    ["openai", "OpenAI"],
  ] as const;
  return (
    <div>
      <p className="text-sm leading-6 text-muted-foreground">{t("A text model still hears you and speaks on this phone.")}</p>
      <p className="mb-2 mt-3 text-sm text-muted-foreground">{t("Talk answers with")}</p>
      <div className="grid grid-cols-2 gap-2 min-[700px]:grid-cols-4">
        {brains.map(([id, label]) => (
          <button
            key={id}
            type="button"
            onClick={() => onFocus(id)}
            className={cn("min-h-11 border px-3 text-left text-sm", primary === id ? "border-foreground text-foreground" : "border-border text-muted-foreground")}
          >
            {t(label)}
          </button>
        ))}
      </div>
      <p className="mb-2 mt-4 text-sm text-muted-foreground">{t("On this phone")}</p>
      <div className="flex flex-col gap-2">
        {downloaded.length === 0 ? <p className="text-sm leading-6 text-muted-foreground">{t("No language model is on this phone yet.")}</p> : null}
        {downloaded.map((pack) => (
          <button
            key={pack.id}
            type="button"
            onClick={() => onUsePack(pack.id)}
            className={cn("flex min-h-11 w-full flex-col items-start justify-center border px-3 py-2 text-left text-sm", pack.selected ? "border-foreground text-foreground" : "border-border text-muted-foreground")}
          >
            <span>{t(pack.label)}</span>
            <span>
              {[pack.selected ? t("In use") : "", pack.recommended ? t("Recommended for your phone") : "", pack.fit ? t(fitLabel(pack.fit)) : ""].filter(Boolean).join(" · ")}
            </span>
          </button>
        ))}
      </div>
      <label className="mt-4 block">
        <span className="mb-2 block text-sm text-muted-foreground">{t("xAI model")}</span>
        <Input value={settings.model} onChange={(event) => onSettings({ model: event.target.value })} />
      </label>
      <label className="mt-3 block">
        <span className="mb-2 block text-sm text-muted-foreground">{t("OpenAI model")}</span>
        <Input value={settings.openaiModel || "gpt-realtime"} onChange={(event) => onSettings({ openaiModel: event.target.value })} />
      </label>
      <div className="mt-3">
        <p className="mb-2 text-sm text-muted-foreground">{t("1min.ai model")}</p>
        <OneminModels hint models={oneminModels} value={settings.oneminModel || "gpt-4.1-mini"} onPick={(id) => onSettings({ oneminModel: id })} onRefresh={onRefreshModels} />
      </div>
    </div>
  );
}

function OneminModels({
  models,
  value,
  onPick,
  onRefresh,
  hint = false,
}: {
  models: OneMinModel[];
  value: string;
  onPick: (id: string) => void;
  onRefresh: () => void;
  hint?: boolean;
}) {
  const t = useT();
  const shown = models.length > 0 ? models : previewOneminModels();
  return (
    <div>
      {hint || models.length === 0 ? <p className="mb-2 text-sm leading-6 text-muted-foreground">{t("The model list loads after a key is saved.")}</p> : null}
      <div className="flex flex-col gap-2">
        {shown.map((model) => (
          <button
            key={model.id}
            type="button"
            onClick={() => onPick(model.id)}
            className={cn("flex min-h-12 items-center justify-between gap-3 border px-3 text-left text-sm", value === model.id ? "border-foreground text-foreground" : "border-border text-muted-foreground")}
          >
            <span>{model.id}</span>
            {model.tools ? null : <span>{t("No tools")}</span>}
          </button>
        ))}
      </div>
      <button type="button" className="mt-3 min-h-11 text-sm text-foreground underline" onClick={onRefresh}>
        {t("Refresh models")}
      </button>
    </div>
  );
}

function askedSection(): "hearing" | "brain" | "voice" | "order" | "advanced" | null {
  if (typeof window === "undefined") return null;
  const value = new URLSearchParams(window.location.search).get("open");
  if (value === "hearing" || value === "brain" || value === "voice" || value === "order" || value === "advanced") return value;
  return null;
}

function askedProvider(): "xai" | "openai" | "local" | "onemin" {
  if (typeof window === "undefined") return "xai";
  const value = new URLSearchParams(window.location.search).get("provider");
  if (value === "openai" || value === "local" || value === "onemin" || value === "xai") return value;
  return "xai";
}

function previewOneminModels(): OneMinModel[] {
  return [
    { id: "gpt-4.1-mini", ownedBy: "openai", tools: true },
    { id: "gpt-4.1", ownedBy: "openai", tools: true },
    { id: "gpt-6.1-sol", ownedBy: "openai", tools: false },
    { id: "gpt-6.2-astra", ownedBy: "openai", tools: false },
    { id: "gpt-6.2-sol", ownedBy: "openai", tools: false },
  ];
}
