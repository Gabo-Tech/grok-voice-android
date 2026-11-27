import type { CSSProperties } from "react";
import { GrokMark } from "@/components/GrokMark";
import { useLightScheme, useMotionAllowed } from "@/lib/motion";
import { cn } from "@/lib/utils";

export function Loader({
  size = "block",
  value,
  label,
  quiet = false,
}: {
  size?: "inline" | "block" | "full";
  /** 0–100. Omit it when the wait has no known progress. */
  value?: number;
  label: string;
  /** Keeps the label for assistive tech without repeating it next to a button. */
  quiet?: boolean;
}) {
  const motion = useMotionAllowed();
  const light = useLightScheme();
  const determinate = typeof value === "number" && Number.isFinite(value);
  const progress = determinate ? Math.min(100, Math.max(0, value)) : undefined;
  return (
    <div
      className={cn("loader", `loader-${size}`, (!motion || determinate) && "loader-still")}
      role={determinate ? "progressbar" : "status"}
      aria-busy="true"
      aria-live="polite"
      aria-valuemin={determinate ? 0 : undefined}
      aria-valuemax={determinate ? 100 : undefined}
      aria-valuenow={determinate ? Math.round(progress ?? 0) : undefined}
      aria-label={quiet ? label : undefined}
      style={determinate ? ({ "--progress": `${progress}%` } as CSSProperties) : undefined}
    >
      <GrokMark className="loader-mark" scheme={light ? "light" : "dark"} />
      {determinate ? (
        <span className="loader-track" aria-hidden="true">
          <span className="loader-fill" />
        </span>
      ) : null}
      <span className={cn("loader-label", quiet && "sr-only")}>{label}</span>
    </div>
  );
}
