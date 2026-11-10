import { Field } from "@/components/Field";
import { Input } from "@/components/ui/input";
import { api, errorText } from "@/lib/api";
import { useT } from "@/lib/i18n";

export type SoundMode = "default" | "file" | "youtube";

const MODES: SoundMode[] = ["default", "file", "youtube"];

export function AlarmSoundFields({
  mode,
  fileName,
  fileRef,
  url,
  error,
  busy,
  onMode,
  onFile,
  onUrl,
  onError,
}: {
  mode: SoundMode;
  fileName: string;
  fileRef: string;
  url: string;
  error: string | null;
  busy: boolean;
  onMode: (mode: SoundMode) => void;
  onFile: (relativePath: string, name: string) => void;
  onUrl: (url: string) => void;
  onError: (error: string | null) => void;
}) {
  const t = useT();
  const label = (item: SoundMode) => (item === "default" ? t("Default") : item === "file" ? t("Audio file") : t("YouTube"));

  const pick = async () => {
    onError(null);
    try {
      const picked = await api.pickAlarmSound();
      onFile(picked.relativePath, picked.name);
      onMode("file");
    } catch (e) {
      onError(errorText(e));
    }
  };

  const preview = async () => {
    onError(null);
    try {
      await api.previewAlarmSound(fileRef);
    } catch (e) {
      onError(errorText(e));
    }
  };

  return (
    <div>
      <p className="mb-2 text-sm text-muted-foreground">{t("Sound")}</p>
      <div className="grid grid-cols-3 gap-2">
        {MODES.map((item) => (
          <button
            key={item}
            type="button"
            aria-pressed={mode === item}
            onClick={() => {
              onError(null);
              onMode(item);
            }}
            className={
              mode === item
                ? "h-12 border border-foreground text-sm text-foreground"
                : "h-12 border border-border text-sm text-muted-foreground"
            }
          >
            {label(item)}
          </button>
        ))}
      </div>
      {mode === "default" && <p className="mt-2 text-sm leading-5 text-muted-foreground">{t("Uses the phone's alarm sound.")}</p>}
      {mode === "file" && (
        <div className="mt-3">
          <button type="button" className="h-12 w-full border border-border text-sm" disabled={busy} onClick={() => void pick()}>
            {t("Choose audio file")}
          </button>
          {fileName && (
            <div className="mt-2 flex items-center gap-2">
              <p className="min-w-0 flex-1 truncate text-sm text-foreground">{fileName}</p>
              <button type="button" className="h-12 shrink-0 border border-border px-3 text-sm" disabled={busy} onClick={() => void preview()}>
                {t("Play")}
              </button>
            </div>
          )}
          <p className="mt-2 text-sm leading-5 text-muted-foreground">{t("Plays this file when the alarm rings.")}</p>
        </div>
      )}
      {mode === "youtube" && (
        <div className="mt-3">
          <Field label={t("YouTube link")} hint={t("Paste a YouTube link. The video plays in this app when the alarm rings. It is not downloaded.")}>
            <Input
              type="url"
              inputMode="url"
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck={false}
              value={url}
              placeholder="https://www.youtube.com/watch?v=…"
              aria-label={t("YouTube link")}
              onChange={(e) => onUrl(e.target.value)}
            />
          </Field>
        </div>
      )}
      {error && (
        <p role="alert" className="mt-2 text-sm leading-5 text-foreground">
          {t(error)}
        </p>
      )}
    </div>
  );
}
