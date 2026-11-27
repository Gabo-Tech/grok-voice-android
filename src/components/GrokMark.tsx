import { cn } from "@/lib/utils";

/** Layered robot so Talk and the loader can move the eyes, ears, and bars. */
export function GrokMark({ className, scheme }: { className?: string; scheme: "light" | "dark" }) {
  return (
    <svg className={cn("grok-mark", className)} data-scheme={scheme} viewBox="0 0 80 96" aria-hidden="true">
      <g className="mark-ears">
        <ellipse className="paper" cx="13" cy="38" rx="9" ry="11" />
        <ellipse className="paper" cx="67" cy="38" rx="9" ry="11" />
        <rect className="ink" x="9" y="34" width="4" height="8" rx="1" />
        <rect className="ink" x="67" y="34" width="4" height="8" rx="1" />
      </g>
      <g className="mark-pips">
        <rect className="paper" x="57" y="6" width="5" height="13" rx="2.5" transform="rotate(18 59.5 12.5)" />
        <rect className="paper" x="66" y="10" width="5" height="11" rx="2.5" transform="rotate(34 68.5 15.5)" />
      </g>
      <circle className="paper" cx="40" cy="38" r="20" />
      <ellipse className="ink" cx="40" cy="37" rx="14.5" ry="12" />
      <g className="presence-eyes">
        <ellipse className="eye" cx="34" cy="36" rx="2.5" ry="3.4" />
        <ellipse className="eye" cx="46" cy="36" rx="2.5" ry="3.4" />
      </g>
      <path className="smile" d="M34 42.2 Q40 46 46 42.2" />
      <g className="mark-bubble">
        <ellipse className="paper" cx="40" cy="74" rx="22" ry="13" />
        <path className="paper" d="M48 84 L42 93 L54 85 Z" />
        <g className="presence-bars">
          <rect className="ink bar" x="22" y="70" width="3.2" height="8" rx="1.4" />
          <rect className="ink bar" x="28.5" y="66" width="3.2" height="16" rx="1.4" />
          <rect className="ink bar" x="35" y="64" width="3.2" height="20" rx="1.4" />
          <rect className="ink bar" x="41.5" y="67" width="3.2" height="14" rx="1.4" />
          <rect className="ink bar" x="48" y="70" width="3.2" height="8" rx="1.4" />
        </g>
      </g>
    </svg>
  );
}
