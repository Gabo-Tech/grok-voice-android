import type { Settings } from "@/lib/api";
import { applyAppearance, normalizeAppearance, prefersLight, rememberAppearance, type AppearanceChoice } from "@/lib/appearance";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

const CHOICES: Array<{
  id: AppearanceChoice;
  title: string;
  body: string;
  canvas: string;
  panel: string;
  ink: string;
  gold: string;
}> = [
  {
    id: "graphite",
    title: "Graphite",
    body: "Warm dark, and the default.",
    canvas: "#0E0F11",
    panel: "#17181B",
    ink: "#EEECE8",
    gold: "#C4B5A0",
  },
  {
    id: "ink",
    title: "Ink",
    body: "Higher contrast. Ivory on black.",
    canvas: "#070708",
    panel: "#101114",
    ink: "#F7F4EE",
    gold: "#C4B5A0",
  },
  {
    id: "stone",
    title: "Stone",
    body: "Warm light, charcoal ink.",
    canvas: "#F4F2EE",
    panel: "#FFFFFF",
    ink: "#1A1B1E",
    gold: "#8C7358",
  },
  {
    id: "system",
    title: "System",
    body: "Dark uses Graphite. Light uses Stone.",
    canvas: "#0E0F11",
    panel: "#F4F2EE",
    ink: "#EEECE8",
    gold: "#C4B5A0",
  },
];

export function ThemePicker({
  settings,
  onChange,
}: {
  settings: Settings | null;
  onChange: (next: Settings) => void;
}) {
  const t = useT();
  const selected = normalizeAppearance(settings?.appearance);
  const pick = (choice: AppearanceChoice) => {
    rememberAppearance(choice);
    applyAppearance(choice, prefersLight());
    if (!settings) return;
    onChange({ ...settings, appearance: choice });
  };
  return (
    <div className="grid grid-cols-2 gap-3">
      {CHOICES.map((choice) => {
        const on = selected === choice.id;
        return (
          <button
            key={choice.id}
            type="button"
            aria-pressed={on}
            onClick={() => pick(choice.id)}
            className={cn("min-h-24 px-3 py-3 text-left", on && "outline outline-2 outline-offset-2")}
            style={{
              background: choice.id === "system" ? "linear-gradient(135deg, #0E0F11 50%, #F4F2EE 50%)" : choice.canvas,
              color: choice.ink,
              border: `2.5px solid ${on ? choice.gold : choice.ink}`,
              outlineColor: "var(--gold)",
            }}
          >
            <span className="mb-2 flex items-center gap-2">
              <span className="inline-block h-3 w-8" style={{ background: choice.panel, border: `2px solid ${choice.gold}` }} />
              <span className="text-xs font-extrabold tracking-[0.14em] uppercase">{t(choice.title)}</span>
            </span>
            <span className="block text-xs leading-4 opacity-80">{t(choice.body)}</span>
          </button>
        );
      })}
    </div>
  );
}
