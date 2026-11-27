import type { CSSProperties } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { GrokMark } from "@/components/GrokMark";
import type { Level, Phase } from "@/lib/api";
import { useLightScheme, useMotionAllowed } from "@/lib/motion";
import { useT } from "@/lib/i18n";

export type PresencePose = "idle" | "listening" | "thinking" | "speaking";

const POSES: Phase[] = ["idle", "wake", "recording", "connecting", "listening", "thinking", "speaking"];

export function previewPhase(): Phase | null {
  if (typeof window === "undefined" || isTauri()) return null;
  const value = new URLSearchParams(window.location.search).get("presence");
  return POSES.includes(value as Phase) ? (value as Phase) : null;
}

export function presencePose(phase: Phase): PresencePose {
  if (phase === "speaking") return "speaking";
  if (phase === "thinking" || phase === "connecting") return "thinking";
  if (phase === "listening" || phase === "recording") return "listening";
  return "idle";
}

export function presenceLabel(phase: Phase): string {
  if (phase === "speaking") return "Grok is speaking. Open voice status.";
  if (phase === "thinking") return "Grok is thinking. Open voice status.";
  if (phase === "connecting") return "Grok is connecting. Open voice status.";
  if (phase === "listening") return "Grok is listening. Open voice status.";
  if (phase === "recording") return "Grok is recording. Open voice status.";
  if (phase === "wake") return "Grok is waiting. Open voice status.";
  return "Grok is ready. Open voice status.";
}

export function Presence({
  phase,
  level,
  onOpen,
}: {
  phase: Phase;
  level?: Level;
  onOpen: () => void;
}) {
  const t = useT();
  const motion = useMotionAllowed();
  const light = useLightScheme();
  const pose = presencePose(phase);
  const amp = phase === "speaking" && typeof level?.out === "number" ? Math.min(1, Math.max(0, level.out)) : 0;
  const live = pose === "speaking" && amp > 0.04;
  const bars = [0.55, 0.9, 1, 0.75, 0.45].map((weight) => 0.35 + amp * weight * 0.65);
  return (
    <button
      type="button"
      className="presence"
      data-state={pose}
      data-motion={motion ? "on" : "off"}
      data-level={live ? "live" : "loop"}
      style={live ? ({ "--b1": bars[0], "--b2": bars[1], "--b3": bars[2], "--b4": bars[3], "--b5": bars[4] } as CSSProperties) : undefined}
      aria-label={t(presenceLabel(phase))}
      onClick={onOpen}
    >
      <GrokMark scheme={light ? "light" : "dark"} />
    </button>
  );
}
