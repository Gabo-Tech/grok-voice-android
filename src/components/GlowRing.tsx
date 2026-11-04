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
  wakePhrase = "Hey Grok",
}: {
  phase: Phase;
  level: Level;
  onPress: () => void;
  disabled?: boolean;
  wakePhrase?: string;
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

  const left = live ? style.label : `Say “${wakePhrase}”`;
  const right = live ? "Tap to end" : "or tap to talk";

  return (
    <div className="grid shrink-0 grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-2 px-1">
      <p className="text-right text-[10px] leading-4 tracking-[0.08em] uppercase" style={{ color: style.color }}>
        {left}
      </p>
      <button
        type="button"
        onClick={onPress}
        disabled={disabled}
        aria-label={live ? "End voice session" : "Start voice session"}
        className="mark shrink-0 rounded-full disabled:opacity-50"
        style={{ "--ink": style.color } as CSSProperties}
      >
        <span className="mark-fill rounded-full" style={{ opacity: 0.12 + amp * 0.7 }} />
        <Icon className="relative size-5" strokeWidth={1.25} />
      </button>
      <p className="text-left text-[10px] leading-4 tracking-[0.08em] text-muted-foreground uppercase">{right}</p>
      <span className="sr-only" role="status">
        {style.label}
      </span>
    </div>
  );
}
