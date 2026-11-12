import { useEffect, useState } from "react";
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
import { cn } from "@/lib/utils";

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
}: {
  settings: Settings | null;
  onChange: (next: Settings) => void;
}) {
  const t = useT();
  const selected = normalizeAppearance(settings?.appearance);
  const [colors, setColors] = useState<CustomColors>(() => customFromString(settings?.customTheme) ?? colorsFromPreset("graphite"));
  const [seed, setSeed] = useState<PresetId>("graphite");
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

  return (
    <div className="min-w-0 max-w-full">
      <div className="grid min-w-0 grid-cols-2 gap-3">
        {PRESET_CHOICES.map((choice) => (
          <Swatch
            key={choice.id}
            title={t(choice.title)}
            body={t(choice.body)}
            pressed={selected === choice.id}
            canvas={PRESETS[choice.id].canvas}
            panel={PRESETS[choice.id].panel}
            ink={PRESETS[choice.id].ink}
            gold={PRESETS[choice.id].gold}
            onClick={() => commit(choice.id)}
          />
        ))}
        <Swatch
          title={t("System")}
          body={t("Dark uses Graphite. Light uses Stone.")}
          pressed={selected === "system"}
          canvas="#0E0F11"
          panel="#F4F2EE"
          ink="#EEECE8"
          gold="#C4B5A0"
          split
          onClick={() => commit("system")}
        />
        <Swatch
          title={t("Custom")}
          body={t("Pick every color.")}
          pressed={selected === "custom"}
          canvas={preview.canvas}
          panel={preview.panel}
          ink={preview.ink}
          gold={preview.gold}
          wide
          onClick={() => commit("custom", colors)}
        />
      </div>
      {selected === "custom" ? (
        <div className="mt-4 min-w-0 max-w-full border-[2.5px] border-border p-3">
          <div className="min-w-0 border-[2.5px] p-3" style={{ background: preview.canvas, color: preview.ink, borderColor: preview.frameColor }}>
            <p className="text-xs font-extrabold tracking-[0.14em] uppercase" style={{ color: preview.gold }}>
              {t("Live preview")}
            </p>
            <p className="mt-2 text-2xl leading-none font-extrabold tracking-wide uppercase">{t("Net worth")}</p>
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
                <span className="text-xs font-extrabold tracking-[0.12em] uppercase">{t(field.label)}</span>
                <span className="mt-1 flex min-w-0 items-center gap-2">
                  <input
                    type="color"
                    aria-label={t(field.label)}
                    value={parseHex(colors[field.key]) || "#000000"}
                    className="h-12 w-14 shrink-0 border-[2.5px] border-current bg-transparent p-1"
                    onChange={(event) => setField(field.key, event.target.value)}
                  />
                  <input
                    value={colors[field.key]}
                    maxLength={7}
                    autoCapitalize="characters"
                    spellCheck={false}
                    aria-label={`${t(field.label)} #`}
                    className="h-12 min-w-0 flex-1 border-[2.5px] border-border bg-transparent px-3 font-mono text-base uppercase"
                    onChange={(event) => setField(field.key, event.target.value.toUpperCase())}
                  />
                </span>
              </label>
            ))}
          </div>
          <label className="mt-4 block min-w-0">
            <span className="text-xs font-extrabold tracking-[0.12em] uppercase">{t("Start from")}</span>
            <select
              value={seed}
              aria-label={t("Start from")}
              className="mt-1 h-12 w-full max-w-full border-[2.5px] border-border bg-transparent px-3 text-base"
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
    </div>
  );
}

function Swatch({
  title,
  body,
  pressed,
  canvas,
  panel,
  ink,
  gold,
  split,
  wide,
  onClick,
}: {
  title: string;
  body: string;
  pressed: boolean;
  canvas: string;
  panel: string;
  ink: string;
  gold: string;
  split?: boolean;
  wide?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      onClick={onClick}
      className={cn("min-h-24 min-w-0 px-3 py-3 text-left", wide && "col-span-2", pressed && "outline outline-2 outline-offset-2")}
      style={{
        background: split ? "linear-gradient(135deg, #0E0F11 50%, #F4F2EE 50%)" : canvas,
        color: split ? "#EEECE8" : ink,
        border: `2.5px solid ${pressed ? gold : ink}`,
        outlineColor: "var(--gold)",
      }}
    >
      <span className="mb-2 flex min-w-0 items-center gap-2">
        <span className="inline-block h-3 w-8 shrink-0" style={{ background: panel, border: `2px solid ${gold}` }} />
        <span className="truncate text-xs font-extrabold tracking-[0.14em] uppercase">{title}</span>
      </span>
      <span className="block text-xs leading-4 break-words opacity-80">{body}</span>
    </button>
  );
}
