import { useEffect, useState, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import { Sheet } from "@/components/Sheet";
import type { Settings } from "@/lib/api";
import {
  PRESETS,
  applyAppearance,
  colorsFromPreset,
  customFromString,
  customToString,
  normalizeAppearance,
  paletteFromCustom,
  parseHex,
  prefersLight,
  rememberAppearance,
  rememberCustomTheme,
  type AppearanceChoice,
  type CustomColors,
  type PresetId,
} from "@/lib/appearance";
import { useT } from "@/lib/i18n";

const PRESET_CHOICES: Array<{ id: PresetId; title: string; body: string }> = [
  { id: "graphite", title: "Graphite", body: "Warm dark, and the default." },
  { id: "ink", title: "Ink", body: "Higher contrast. Ivory on black." },
  { id: "stone", title: "Stone", body: "Warm light, charcoal ink." },
  { id: "midnight", title: "Midnight", body: "Cool navy. Champagne on ink." },
  { id: "carbon", title: "Carbon", body: "True black. Ivory type." },
  { id: "ivory", title: "Ivory", body: "Bright warm paper." },
  { id: "sand", title: "Sand", body: "Soft sepia, charcoal ink." },
  { id: "forest", title: "Forest", body: "Deep green. Brass type." },
  { id: "plum", title: "Plum", body: "Muted plum. Warm type." },
];

const FIELDS: Array<{ key: keyof CustomColors; label: string }> = [
  { key: "canvas", label: "Canvas" },
  { key: "panel", label: "Panel" },
  { key: "border", label: "Border" },
  { key: "text", label: "Text color" },
  { key: "accent", label: "Accent" },
  { key: "income", label: "Income" },
  { key: "expense", label: "Expenses" },
];

export function ThemePicker({
  settings,
  onChange,
  info,
}: {
  settings: Settings | null;
  onChange: (next: Settings) => void;
  info?: ReactNode;
}) {
  const t = useT();
  const selected = normalizeAppearance(settings?.appearance);
  const [colors, setColors] = useState<CustomColors>(() => customFromString(settings?.customTheme) ?? colorsFromPreset("graphite"));
  const [seed, setSeed] = useState<PresetId>("graphite");
  const [pickerOpen, setPickerOpen] = useState(false);
  const preview = paletteFromCustom(parseHex(colors.canvas) ? colors : colorsFromPreset("graphite"));
  useEffect(() => {
    const parsed = customFromString(settings?.customTheme);
    if (parsed) setColors(parsed);
  }, [settings?.customTheme]);

  const commit = (choice: AppearanceChoice, custom = colors) => {
    rememberAppearance(choice);
    if (choice === "custom") rememberCustomTheme(customToString(custom));
    applyAppearance(choice, prefersLight());
    if (!settings) return;
    onChange({ ...settings, appearance: choice, customTheme: choice === "custom" ? customToString(custom) : settings.customTheme });
  };

  const paintCustom = (next: CustomColors) => {
    setColors(next);
    rememberCustomTheme(customToString(next));
    rememberAppearance("custom");
    applyAppearance("custom", prefersLight());
    if (!settings) return;
    onChange({ ...settings, appearance: "custom", customTheme: customToString(next) });
  };

  const setField = (key: keyof CustomColors, value: string) => {
    const hex = parseHex(value);
    if (!hex) {
      setColors((current) => ({ ...current, [key]: value }));
      return;
    }
    paintCustom({ ...colors, [key]: hex });
  };

  const currentTitle =
    selected === "system" ? t("System") : selected === "custom" ? t("Custom") : t(PRESET_CHOICES.find((item) => item.id === selected)?.title || "Graphite");
  const currentCanvas = selected === "custom" ? preview.canvas : selected === "system" ? PRESETS.graphite.canvas : PRESETS[selected].canvas;

  return (
    <div className="min-w-0 max-w-full">
      <div className="title-line" data-widget-note="">
        <span className="text-base text-foreground">{t("Theme")}</span>
        {info}
        <button type="button" className="ml-auto flex min-h-12 items-center gap-2 text-sm text-muted-foreground" onClick={() => setPickerOpen(true)}>
          <span className="theme-dot" style={{ background: currentCanvas }} aria-hidden />
          <span>{currentTitle}</span>
          <ChevronRight className="size-4" aria-hidden />
        </button>
      </div>
      <Sheet open={pickerOpen} title={t("Theme")} onClose={() => setPickerOpen(false)}>
      <div className="theme-grid" role="radiogroup" aria-label={t("Theme")}>
        {PRESET_CHOICES.map((choice) => (
          <Swatch
            key={choice.id}
            title={t(choice.title)}
            pressed={selected === choice.id}
            canvas={PRESETS[choice.id].canvas}
            ink={PRESETS[choice.id].ink}
            onClick={() => commit(choice.id)}
          />
        ))}
        <Swatch
          title={t("System")}
          pressed={selected === "system"}
          canvas={PRESETS.graphite.canvas}
          ink={PRESETS.graphite.ink}
          split
          onClick={() => commit("system")}
        />
        <Swatch
          title={t("Custom")}
          pressed={selected === "custom"}
          canvas={preview.canvas}
          ink={preview.ink}
          onClick={() => commit("custom", colors)}
        />
      </div>
      {selected === "custom" ? (
        <div className="mt-6 min-w-0 max-w-full border-t border-border pt-5">
          <div className="min-w-0 px-4 py-5" style={{ background: preview.canvas, color: preview.ink }}>
            <p className="text-xs tracking-[0.16em] uppercase" style={{ color: preview.muted }}>
              {t("Live preview")}
            </p>
            <p className="mt-3 text-2xl leading-none font-medium tracking-tight">{t("Net worth")}</p>
            <p className="mt-3 text-sm" style={{ color: preview.sage }}>
              {t("Income")}
            </p>
            <p className="text-sm" style={{ color: preview.rose }}>
              {t("Expenses")}
            </p>
          </div>
          <div className="mt-4 space-y-3">
            {FIELDS.map((field) => (
              <label key={field.key} className="block min-w-0">
                <span className="text-xs tracking-[0.14em] uppercase text-muted-foreground">{t(field.label)}</span>
                <span className="mt-1 flex min-w-0 items-center gap-3">
                  <input
                    type="color"
                    aria-label={t(field.label)}
                    value={parseHex(colors[field.key]) || "#1A1B1E"}
                    className="h-12 w-12 shrink-0 border-0 bg-transparent p-0"
                    onChange={(event) => setField(field.key, event.target.value)}
                  />
                  <input
                    value={colors[field.key]}
                    maxLength={7}
                    autoCapitalize="characters"
                    spellCheck={false}
                    aria-label={`${t(field.label)} #`}
                    className="h-12 min-w-0 flex-1 border-0 border-b border-border bg-transparent px-0 font-mono text-base uppercase"
                    onChange={(event) => setField(field.key, event.target.value.toUpperCase())}
                  />
                </span>
              </label>
            ))}
          </div>
          <label className="mt-4 block min-w-0">
            <span className="text-xs tracking-[0.14em] uppercase text-muted-foreground">{t("Start from")}</span>
            <select
              value={seed}
              aria-label={t("Start from")}
              className="mt-1 h-12 w-full max-w-full border-0 border-b border-border bg-transparent px-0 text-base"
              onChange={(event) => setSeed(event.target.value as PresetId)}
            >
              {PRESET_CHOICES.map((choice) => (
                <option key={choice.id} value={choice.id}>
                  {t(choice.title)}
                </option>
              ))}
            </select>
          </label>
          <button type="button" className="stamp mt-3 min-h-12 w-full" onClick={() => paintCustom(colorsFromPreset(seed))}>
            {t("Reset custom")}
          </button>
        </div>
      ) : null}
      </Sheet>
    </div>
  );
}

function Swatch({
  title,
  pressed,
  canvas,
  ink,
  split,
  onClick,
}: {
  title: string;
  pressed: boolean;
  canvas: string;
  ink: string;
  split?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={pressed}
      onClick={onClick}
      className="theme-swatch"
      style={{ background: canvas, color: ink }}
    >
      {split ? (
        <span className="flex h-7 w-7 overflow-hidden rounded-full" aria-hidden>
          <span className="h-full w-1/2" style={{ background: PRESETS.graphite.canvas }} />
          <span className="h-full w-1/2" style={{ background: PRESETS.stone.canvas }} />
        </span>
      ) : (
        <span className="theme-dot" style={{ background: ink }} aria-hidden />
      )}
      <span className="text-sm">{pressed ? `${title} ✓` : title}</span>
    </button>
  );
}
