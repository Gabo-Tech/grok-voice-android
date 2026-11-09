import { useRef } from "react";
import { useT } from "@/lib/i18n";
import { voiceLabel, xaiSampleUrl, type VoiceChoice } from "@/lib/voices";
import { cn } from "@/lib/utils";

export function VoicePicker({
  voices,
  value,
  onChange,
  preview,
}: {
  voices: VoiceChoice[];
  value: string;
  onChange: (id: string) => void;
  preview?: boolean;
}) {
  const t = useT();
  const audio = useRef<HTMLAudioElement | null>(null);
  const selected = value.trim().toLowerCase();

  const play = (id: string) => {
    const el = audio.current ?? new Audio();
    audio.current = el;
    el.src = xaiSampleUrl(id);
    void el.play().catch(() => {});
  };

  return (
    <div className="max-h-[50vh] overflow-y-auto border-y border-border">
      {voices.map((voice) => {
        const on = voice.id === selected;
        const detail = [voice.tone ? t(voice.tone) : "", voice.accent ? t(voice.accent) : ""].filter(Boolean).join(" · ");
        return (
          <div key={voice.id} className={cn("flex min-h-12 items-stretch gap-2 border-b border-border px-1", on && "bg-white/5")}>
            <button type="button" className="min-h-12 min-w-0 flex-1 py-2 text-left" onClick={() => onChange(voice.id)} aria-pressed={on}>
              <span className="block text-base text-foreground">{voiceLabel(voice.id)}</span>
              {detail ? <span className="mt-0.5 block whitespace-normal break-words text-sm leading-5 text-muted-foreground">{detail}</span> : null}
            </button>
            {preview ? (
              <button type="button" className="min-h-12 shrink-0 px-3 text-sm text-foreground" onClick={() => play(voice.id)}>
                {t("Preview")}
              </button>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
