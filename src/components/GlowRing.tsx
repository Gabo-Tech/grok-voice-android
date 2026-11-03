import type { CSSProperties } from "react";
import { AudioLines, Loader2, Mic, Sparkles, Square } from "lucide-react";
import type { Level, Phase } from "@/lib/api";

const PHASE_STYLE: Record<Phase, { color: string; label: string }> = {
  idle: { color: "#8a8a8a", label: "Tap to talk" },
  wake: { color: "#f5f5f5", label: "Say “Hey Grok”" },
  recording: { color: "#d4d4d4", label: "Recording sample…" },
  connecting: { color: "#c2c2c2", label: "Connecting…" },
  listening: { color: "#ffffff", label: "Listening…" },
  thinking: { color: "#bdbdbd", label: "Thinking…" },
  speaking: { color: "#ffffff", label: "Speaking…" },
};

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
}) {
  const style = PHASE_STYLE[phase] ?? PHASE_STYLE.idle;
  const live = LIVE.includes(phase);
  const raw = phase === "speaking" ? level?.out : phase === "idle" ? 0 : level?.mic;
  const amp = typeof raw === "number" && Number.isFinite(raw) ? raw : 0;
  const Icon =
    phase === "connecting" || phase === "thinking"
      ? Loader2
      : phase === "speaking"
        ? AudioLines
        : phase === "listening"
          ? Sparkles
          : live
            ? Square
            : Mic;

  return (
    <div className="flex shrink-0 items-center justify-between gap-4 px-1">
      <div className="min-w-0">
        <p className="text-sm tracking-[0.16em] uppercase" style={{ color: style.color }}>
          {style.label}
        </p>
        <p className="mt-1 text-xs text-muted-foreground">
          {live ? "Tap to end" : phase === "wake" ? "or tap to talk" : "Ready when you are"}
        </p>
      </div>
      <button
        type="button"
        onClick={onPress}
        disabled={disabled}
        aria-label={live ? "End voice session" : "Start voice session"}
        className="mark shrink-0 disabled:opacity-50"
        style={{ "--ink": style.color } as CSSProperties}
      >
        <span className="mark-fill" style={{ opacity: 0.12 + amp * 0.7 }} />
        <Icon className="relative size-5" strokeWidth={1.25} />
      </button>
      <span className="sr-only" role="status">
        {style.label}
      </span>
    </div>
  );
}
