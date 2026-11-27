import { AudioLines, Mic, Square } from "lucide-react";
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
  const raw = phase === "speaking" ? level?.out : phase === "listening" ? level?.mic : 0;
  const amp = typeof raw === "number" && Number.isFinite(raw) ? raw : 0;
  const userTalking = phase === "listening" && amp >= 0.08;
  const shown = userTalking ? "user" : phase;
  const Icon = phase === "speaking" ? AudioLines : live ? Square : Mic;

  return (
    <div className="flex shrink-0 flex-col items-center">
      <p className="sr-only" role="status">{copy.label}</p>
      <button
        type="button"
        onClick={onPress}
        disabled={disabled}
        aria-pressed={live}
        data-phase={shown}
        aria-label={live ? t("End voice session") : t("Start voice session")}
        className="mark shrink-0 rounded-full disabled:opacity-50"
        style={{ transform: userTalking || phase === "speaking" ? `scale(${(1 + amp * 0.12).toFixed(3)})` : undefined }}
      >
        <span className="mark-fill rounded-full" style={{ opacity: phase === "idle" ? 0 : 0.12 + amp * 0.7 }} />
        <Icon className="relative size-6" strokeWidth={1.5} />
      </button>
    </div>
  );
}
