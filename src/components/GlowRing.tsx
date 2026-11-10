import type { CSSProperties } from "react";
import { AudioLines, Loader2, Mic, Square } from "lucide-react";
import type { Level, Phase } from "@/lib/api";
import { useT } from "@/lib/i18n";

const LIVE: Phase[] = ["connecting", "listening", "thinking", "speaking"];

export function GlowRing({
  phase,
  level,
  onPress,
  disabled,
}: {
  phase: Phase;
  level: Level;
  onPress: () => void;
  disabled?: boolean;
  wakePhrase?: string;
}) {
  const t = useT();
  const copy = {
    idle: { label: t("Ready"), hint: t("Tap the circle to talk") },
    wake: { label: t("Waiting for Hey Grok"), hint: t("Or tap the circle to talk now") },
    recording: { label: t("Recording"), hint: t("Keep speaking") },
    connecting: { label: t("Connecting"), hint: t("Trying your voices in order") },
    listening: { label: t("Listening"), hint: t("Speak now. Tap the circle to hang up.") },
    thinking: { label: t("Thinking"), hint: t("Tap the circle to hang up.") },
    speaking: { label: t("Speaking"), hint: t("Tap the circle to hang up.") },
  }[phase] ?? { label: t("Ready"), hint: t("Tap the circle to talk") };
  const live = LIVE.includes(phase);
  const raw = phase === "speaking" ? level?.out : phase === "idle" ? 0 : level?.mic;
  const amp = typeof raw === "number" && Number.isFinite(raw) ? raw : 0;
  const Icon =
    phase === "connecting" || phase === "thinking" ? Loader2 : phase === "speaking" ? AudioLines : live ? Square : Mic;

  return (
    <div className="flex shrink-0 flex-col items-center gap-3 px-1 pb-1">
      <div className="text-center">
        <p className="text-lg leading-6 font-medium text-foreground" role="status">
          {copy.label}
        </p>
        <p className="mt-1 text-sm leading-5 text-muted-foreground">{copy.hint}</p>
      </div>
      <button
        type="button"
        onClick={onPress}
        disabled={disabled}
        aria-pressed={live}
        aria-label={live ? t("End voice session") : t("Start voice session")}
        className="mark shrink-0 rounded-full disabled:opacity-50"
        style={{ "--ink": live ? "#f2f2f2" : "#b5b5b5", width: "5.75rem", height: "5.75rem" } as CSSProperties}
      >
        <span className="mark-fill rounded-full" style={{ opacity: 0.12 + amp * 0.7 }} />
        <Icon className={`relative size-6 ${phase === "connecting" || phase === "thinking" ? "animate-spin" : ""}`} strokeWidth={1.25} />
      </button>
    </div>
  );
}
