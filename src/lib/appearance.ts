export type PresetId = "graphite" | "ink" | "stone" | "midnight" | "carbon" | "ivory" | "sand" | "forest" | "plum";
export type AppearanceChoice = PresetId | "system" | "custom";
export type ResolvedAppearance = PresetId | "custom";

const STORAGE_KEY = "grok.appearance";
const CUSTOM_KEY = "grok.customTheme";

/** WidgetTheme.kt mirrors these hex values. */
export interface Palette {
  canvas: string;
  panel: string;
  panel2: string;
  ink: string;
  muted: string;
  gold: string;
  goldDeep: string;
  sage: string;
  rose: string;
  frame: string;
  frameColor: string;
  rule: string;
  kicker: string;
  grain: string;
  onGold: string;
  scheme: "light" | "dark";
}

export interface CustomColors {
  canvas: string;
  panel: string;
  border: string;
  text: string;
  accent: string;
  income: string;
  expense: string;
}

export const PRESETS: Record<PresetId, Palette> = {
  graphite: {
    canvas: "#0E0F11",
    panel: "#17181B",
    panel2: "#1C1D21",
    ink: "#EEECE8",
    muted: "#8B8A86",
    gold: "#C4B5A0",
    goldDeep: "#B8A48A",
    sage: "#7D9A86",
    rose: "#C48984",
    frame: "2.5px",
    frameColor: "#9A9690",
    rule: "rgba(232, 230, 227, 0.28)",
    kicker: "#C4B5A0",
    grain: "rgba(238, 236, 232, 0.55)",
    onGold: "#1A1B1E",
    scheme: "dark",
  },
  ink: {
    canvas: "#070708",
    panel: "#101114",
    panel2: "#16171B",
    ink: "#F7F4EE",
    muted: "#B7B3AA",
    gold: "#C4B5A0",
    goldDeep: "#B8A48A",
    sage: "#8EAE98",
    rose: "#D09A96",
    frame: "3px",
    frameColor: "#E8E4DC",
    rule: "rgba(247, 244, 238, 0.35)",
    kicker: "#F7F4EE",
    grain: "rgba(247, 244, 238, 0.7)",
    onGold: "#070708",
    scheme: "dark",
  },
  stone: {
    canvas: "#F4F2EE",
    panel: "#FBFAF7",
    panel2: "#FFFFFF",
    ink: "#1A1B1E",
    muted: "#5E5A54",
    gold: "#6F5A42",
    goldDeep: "#8C7358",
    sage: "#2F6A48",
    rose: "#8E3F3B",
    frame: "2.5px",
    frameColor: "#2A2826",
    rule: "rgba(26, 27, 30, 0.18)",
    kicker: "#5C4632",
    grain: "rgba(26, 27, 30, 0.45)",
    onGold: "#F4F2EE",
    scheme: "light",
  },
  midnight: {
    canvas: "#0B1220",
    panel: "#121A2B",
    panel2: "#182236",
    ink: "#E7E4DC",
    muted: "#8E97A8",
    gold: "#C6B59A",
    goldDeep: "#A89478",
    sage: "#7F9E8C",
    rose: "#C48B8A",
    frame: "2.5px",
    frameColor: "#8E97A8",
    rule: "rgba(231, 228, 220, 0.28)",
    kicker: "#C6B59A",
    grain: "rgba(231, 228, 220, 0.5)",
    onGold: "#0B1220",
    scheme: "dark",
  },
  carbon: {
    canvas: "#000000",
    panel: "#0A0A0A",
    panel2: "#141414",
    ink: "#F4F1EA",
    muted: "#9A958C",
    gold: "#C4B5A0",
    goldDeep: "#B8A48A",
    sage: "#86A892",
    rose: "#C98984",
    frame: "3px",
    frameColor: "#E8E4DC",
    rule: "rgba(244, 241, 234, 0.32)",
    kicker: "#F4F1EA",
    grain: "rgba(244, 241, 234, 0.65)",
    onGold: "#000000",
    scheme: "dark",
  },
  ivory: {
    canvas: "#FBF7F0",
    panel: "#FFFFFF",
    panel2: "#F6F1E8",
    ink: "#1C1916",
    muted: "#6B645C",
    gold: "#8A6E49",
    goldDeep: "#6F5838",
    sage: "#2E6B46",
    rose: "#8C3E3A",
    frame: "2.5px",
    frameColor: "#2A241E",
    rule: "rgba(28, 25, 22, 0.16)",
    kicker: "#6F5838",
    grain: "rgba(28, 25, 22, 0.4)",
    onGold: "#FBF7F0",
    scheme: "light",
  },
  sand: {
    canvas: "#E7DCC8",
    panel: "#F3E9D6",
    panel2: "#EFE4CE",
    ink: "#3A2E22",
    muted: "#7A6A58",
    gold: "#8C6239",
    goldDeep: "#6E4C2A",
    sage: "#3E6B4F",
    rose: "#8E4A42",
    frame: "2.5px",
    frameColor: "#4A3B2C",
    rule: "rgba(58, 46, 34, 0.2)",
    kicker: "#6E4C2A",
    grain: "rgba(58, 46, 34, 0.4)",
    onGold: "#F3E9D6",
    scheme: "light",
  },
  forest: {
    canvas: "#101612",
    panel: "#18201B",
    panel2: "#1E2922",
    ink: "#E8E4D8",
    muted: "#8E9A8E",
    gold: "#C6B48A",
    goldDeep: "#A89468",
    sage: "#8FB59A",
    rose: "#C4928C",
    frame: "2.5px",
    frameColor: "#9AA898",
    rule: "rgba(232, 228, 216, 0.28)",
    kicker: "#C6B48A",
    grain: "rgba(232, 228, 216, 0.5)",
    onGold: "#101612",
    scheme: "dark",
  },
  plum: {
    canvas: "#161218",
    panel: "#211C24",
    panel2: "#2A242E",
    ink: "#F0EBE4",
    muted: "#A398A4",
    gold: "#C4B09A",
    goldDeep: "#A89078",
    sage: "#86A494",
    rose: "#C48A96",
    frame: "2.5px",
    frameColor: "#B4A4B0",
    rule: "rgba(240, 235, 228, 0.28)",
    kicker: "#C4B09A",
    grain: "rgba(240, 235, 228, 0.5)",
    onGold: "#161218",
    scheme: "dark",
  },
};

const CHOICES: AppearanceChoice[] = [...Object.keys(PRESETS), "system", "custom"] as AppearanceChoice[];

export function isPresetId(value: string): value is PresetId {
  return Object.prototype.hasOwnProperty.call(PRESETS, value);
}

export function isAppearanceChoice(value: string): value is AppearanceChoice {
  return (CHOICES as string[]).includes(value);
}

/** A missing or unknown value stays on Graphite. System and Custom are only explicit picks. */
export function normalizeAppearance(value: string | undefined | null): AppearanceChoice {
  const cleaned = (value ?? "").trim().toLowerCase();
  return isAppearanceChoice(cleaned) ? cleaned : "graphite";
}

export function hasStoredAppearance(): boolean {
  try {
    return window.localStorage.getItem(STORAGE_KEY) !== null;
  } catch {
    return false;
  }
}

export function readStoredAppearance(): AppearanceChoice {
  try {
    return normalizeAppearance(window.localStorage.getItem(STORAGE_KEY));
  } catch {
    return "graphite";
  }
}

export function rememberAppearance(choice: AppearanceChoice) {
  try {
    window.localStorage.setItem(STORAGE_KEY, choice);
  } catch {
    /* private mode */
  }
}

export function colorsFromPreset(id: PresetId): CustomColors {
  const palette = PRESETS[id];
  return {
    canvas: palette.canvas,
    panel: palette.panel,
    border: palette.frameColor,
    text: palette.ink,
    accent: palette.gold,
    income: palette.sage,
    expense: palette.rose,
  };
}

export function customToString(colors: CustomColors): string {
  return [colors.canvas, colors.panel, colors.border, colors.text, colors.accent, colors.income, colors.expense].join(",");
}

export function parseHex(value: string): string | null {
  const match = /^#?([0-9A-Fa-f]{6})$/.exec(value.trim());
  return match ? `#${match[1].toUpperCase()}` : null;
}

export function customFromString(value: string | undefined | null): CustomColors | null {
  const parts = (value ?? "").split(",").map((part) => parseHex(part));
  if (parts.length !== 7 || parts.some((part) => !part)) return null;
  const [canvas, panel, border, text, accent, income, expense] = parts as string[];
  return { canvas, panel, border, text, accent, income, expense };
}

export function rememberCustomTheme(value: string) {
  const colors = customFromString(value);
  if (!colors) return;
  try {
    window.localStorage.setItem(CUSTOM_KEY, customToString(colors));
  } catch {
    /* private mode */
  }
}

export function readStoredCustom(): CustomColors | null {
  try {
    return customFromString(window.localStorage.getItem(CUSTOM_KEY));
  } catch {
    return null;
  }
}

export function relativeLuminance(hex: string): number {
  const parsed = parseHex(hex);
  if (!parsed) return 0;
  const channels = [1, 3, 5].map((index) => {
    const channel = parseInt(parsed.slice(index, index + 2), 16) / 255;
    return channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function mix(a: string, b: string, toward: number): string {
  const left = parseHex(a) || "#000000";
  const right = parseHex(b) || "#000000";
  const channel = (index: number) => {
    const from = parseInt(left.slice(index, index + 2), 16);
    const to = parseInt(right.slice(index, index + 2), 16);
    return Math.round(from + (to - from) * toward);
  };
  const hex = (value: number) => value.toString(16).padStart(2, "0");
  return `#${hex(channel(1))}${hex(channel(3))}${hex(channel(5))}`.toUpperCase();
}

function rgba(hex: string, alpha: number): string {
  const parsed = parseHex(hex) || "#000000";
  const r = parseInt(parsed.slice(1, 3), 16);
  const g = parseInt(parsed.slice(3, 5), 16);
  const b = parseInt(parsed.slice(5, 7), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

export function paletteFromCustom(colors: CustomColors): Palette {
  const light = relativeLuminance(colors.canvas) > 0.45;
  const accentLight = relativeLuminance(colors.accent) > 0.55;
  return {
    canvas: colors.canvas,
    panel: colors.panel,
    panel2: mix(colors.panel, colors.text, 0.08),
    ink: colors.text,
    muted: mix(colors.text, colors.canvas, 0.45),
    gold: colors.accent,
    goldDeep: mix(colors.accent, colors.canvas, 0.28),
    sage: colors.income,
    rose: colors.expense,
    frame: "2.5px",
    frameColor: colors.border,
    rule: rgba(colors.text, light ? 0.18 : 0.28),
    kicker: colors.accent,
    grain: rgba(colors.text, light ? 0.4 : 0.55),
    onGold: accentLight ? "#1A1B1E" : "#F7F4EE",
    scheme: light ? "light" : "dark",
  };
}

export function resolvedAppearance(choice: AppearanceChoice, prefersLight: boolean): ResolvedAppearance {
  if (choice === "system") return prefersLight ? "stone" : "graphite";
  return choice;
}

function paletteFor(choice: AppearanceChoice, prefersLight: boolean): Palette {
  const resolved = resolvedAppearance(choice, prefersLight);
  if (resolved === "custom") return paletteFromCustom(readStoredCustom() ?? colorsFromPreset("graphite"));
  return PRESETS[resolved];
}

function paintPalette(root: HTMLElement, palette: Palette) {
  const tokens: Record<string, string> = {
    "--canvas": palette.canvas,
    "--panel": palette.panel,
    "--panel-2": palette.panel2,
    "--ink": palette.ink,
    "--muted-ink": palette.muted,
    "--gold": palette.gold,
    "--gold-deep": palette.goldDeep,
    "--sage": palette.sage,
    "--rose": palette.rose,
    "--frame": palette.frame,
    "--frame-color": palette.frameColor,
    "--rule": palette.rule,
    "--kicker": palette.kicker,
    "--grain": palette.grain,
    "--on-gold": palette.onGold,
    "--scheme": palette.scheme,
  };
  for (const [name, value] of Object.entries(tokens)) root.style.setProperty(name, value);
  root.style.colorScheme = palette.scheme;
}

export function applyAppearance(choice: AppearanceChoice, prefersLight: boolean) {
  if (typeof document === "undefined") return;
  const resolved = resolvedAppearance(choice, prefersLight);
  const palette = paletteFor(choice, prefersLight);
  const root = document.documentElement;
  root.dataset.appearance = resolved;
  root.dataset.appearanceChoice = choice;
  root.classList.toggle("dark", palette.scheme === "dark");
  paintPalette(root, palette);
  const meta = document.querySelector('meta[name="theme-color"]');
  if (meta) meta.setAttribute("content", palette.canvas);
  const bridge = (window as unknown as { GrokTheme?: { setCanvas?: (hex: string) => void } }).GrokTheme;
  try {
    bridge?.setCanvas?.(palette.canvas);
  } catch {
    /* the page still paints from CSS */
  }
}

export function bootAppearance() {
  applyAppearance(readStoredAppearance(), prefersLight());
}

export function prefersLight(): boolean {
  return typeof window !== "undefined" && window.matchMedia("(prefers-color-scheme: light)").matches;
}

/** Server value wins when it is a real choice. Empty keeps whatever already painted. */
export function choiceFromSettings(value: string | undefined | null): AppearanceChoice | null {
  const cleaned = (value ?? "").trim().toLowerCase();
  return isAppearanceChoice(cleaned) ? cleaned : null;
}
