/** One saved color per account, holding, category, income type, class, chart series, or Sankey node. */

export const SERIES_HEX = [
  "#0072B2",
  "#9A5C00",
  "#0B7A5A",
  "#A84B86",
  "#C44E00",
  "#2E6A8A",
  "#B85A78",
  "#1F7A6E",
  "#9A5AAA",
  "#267A46",
  "#7A68C0",
  "#A3522A",
] as const;

export const SERIES_TOKEN = SERIES_HEX.map((_, index) => `var(--series-${index + 1})`);

/** Auto-assigned by older builds. Treated as "no choice" so each item gets a distinct series color. */
export const LEGACY_HEX = [
  "#4C78D8",
  "#6E8B74",
  "#A56B66",
  "#C4B5A0",
  "#7A6A58",
  "#3E6B73",
  "#8C6A3F",
  "#B8A48A",
  "#5C6B58",
  "#9A7B6A",
] as const;

export const THEME_CANVAS = {
  graphite: "#0E0F11",
  ink: "#121214",
  midnight: "#0B1220",
  carbon: "#141416",
  forest: "#101612",
  plum: "#161218",
  stone: "#F4F2EE",
  ivory: "#FBF7F0",
  sand: "#E7DCC8",
} as const;

const HUE = ["blue", "amber", "green", "pink", "red", "steel", "rose", "teal", "purple", "forest", "indigo", "rust"] as const;

const NAMED: Array<{ word: string; hex: string }> = [
  { word: "green", hex: "#009E73" },
  { word: "verde", hex: "#009E73" },
  { word: "grun", hex: "#009E73" },
  { word: "blue", hex: "#0072B2" },
  { word: "azul", hex: "#0072B2" },
  { word: "blau", hex: "#0072B2" },
  { word: "red", hex: "#C44E00" },
  { word: "rojo", hex: "#C44E00" },
  { word: "rot", hex: "#C44E00" },
  { word: "vermelho", hex: "#C44E00" },
  { word: "roig", hex: "#C44E00" },
  { word: "vermell", hex: "#C44E00" },
  { word: "orange", hex: "#E69F00" },
  { word: "naranja", hex: "#E69F00" },
  { word: "laranja", hex: "#E69F00" },
  { word: "taronja", hex: "#E69F00" },
  { word: "gray", hex: "#8A8A8A" },
  { word: "grey", hex: "#8A8A8A" },
  { word: "gris", hex: "#8A8A8A" },
  { word: "grau", hex: "#8A8A8A" },
  { word: "cinza", hex: "#8A8A8A" },
  { word: "cinzent", hex: "#8A8A8A" },
  { word: "pink", hex: "#A84B86" },
  { word: "rosa", hex: "#A84B86" },
  { word: "purple", hex: "#9A5AAA" },
  { word: "morado", hex: "#9A5AAA" },
  { word: "lila", hex: "#9A5AAA" },
  { word: "roxo", hex: "#9A5AAA" },
  { word: "morat", hex: "#9A5AAA" },
  { word: "teal", hex: "#1F7A6E" },
  { word: "turquesa", hex: "#1F7A6E" },
  { word: "petrol", hex: "#1F7A6E" },
  { word: "amber", hex: "#9A5C00" },
  { word: "ambar", hex: "#9A5C00" },
];

export interface EntityColor {
  kind: string;
  id: string;
  color: string;
}

export function paletteIndex(id: string): number {
  let hash = 0;
  for (const ch of id) {
    hash = Math.imul(hash, 33) + (ch.codePointAt(0) || 0);
    hash >>>= 0;
  }
  return hash % SERIES_HEX.length;
}

export function defaultToken(id: string): string {
  return SERIES_TOKEN[paletteIndex(id)] || SERIES_TOKEN[0];
}

export function normalizeHex(color: string): string {
  const trimmed = color.trim();
  const match = /^#?([0-9A-Fa-f]{6})$/.exec(trimmed);
  return match ? `#${match[1].toUpperCase()}` : "";
}

export function isLegacyColor(color: string): boolean {
  const hex = normalizeHex(color);
  return Boolean(hex) && (LEGACY_HEX as readonly string[]).includes(hex);
}

export function isKeptColor(color: string): boolean {
  const trimmed = color.trim();
  if (!trimmed) return false;
  if (trimmed.startsWith("var(--series-") || trimmed.startsWith("var(--flow-") || trimmed.startsWith("var(--brand") || trimmed.startsWith("var(--positive") || trimmed.startsWith("var(--negative")) return true;
  const hex = normalizeHex(trimmed);
  return Boolean(hex) && !isLegacyColor(hex);
}

/** Saved choice, or a distinct series color when the row was never chosen. */
export function shownColor(id: string, saved = ""): string {
  if (isKeptColor(saved)) return saved.trim().startsWith("var(") ? saved.trim() : normalizeHex(saved);
  return defaultToken(id || "item");
}

export function tokenToHex(color: string): string {
  const token = /^var\(--series-(\d+)\)$/.exec(color.trim());
  if (token) {
    const index = Number(token[1]) - 1;
    if (SERIES_HEX[index]) return SERIES_HEX[index];
  }
  return normalizeHex(color) || SERIES_HEX[0];
}

export function classToken(name: string): string {
  switch (name.trim().toLowerCase()) {
    case "crypto":
      return "var(--series-5)";
    case "metals":
    case "metal":
      return "var(--series-2)";
    case "stocks":
    case "stock":
    case "funds":
    case "fund":
      return "var(--series-1)";
    case "cash":
      return "var(--series-6)";
    case "manual":
    case "collectible":
    case "property":
      return "var(--series-4)";
    default:
      return "var(--series-12)";
  }
}

const SERIES_DEFAULT: Record<string, string> = {
  "trend:income": "var(--positive)",
  "trend:expense": "var(--negative)",
  "trend:net": "var(--brand)",
  "allocation:money": "var(--brand)",
  "allocation:debt": "var(--negative)",
  "allocation:investment": "var(--series-3)",
  "allocation:credit": "var(--series-6)",
  "portfolio:total": "var(--brand)",
  "activity:income": "var(--positive)",
  "activity:expenses": "var(--negative)",
  "activity:result": "var(--brand)",
  "activity:cash": "var(--series-6)",
  "activity:cards": "var(--series-7)",
  "activity:debts": "var(--negative)",
};

function fold(value: string): string {
  return value.trim().toLowerCase();
}

export function findColor(rows: EntityColor[] | undefined, kind: string, id: string): EntityColor | undefined {
  const key = fold(id);
  return (rows || []).find((row) => fold(row.kind) === fold(kind) && fold(row.id) === key);
}

export function resolveColor(rows: EntityColor[] | undefined, kind: string, id: string, legacy = ""): string {
  const saved = findColor(rows, kind, id)?.color || "";
  if (kind === "income") {
    const income = saved || findColor(rows, "category", id)?.color || "";
    if (isKeptColor(income)) return shownColor(id, income);
    if (isKeptColor(legacy)) return shownColor(id, legacy);
    return shownColor(id, "");
  }
  if (kind === "category") {
    const category = saved || findColor(rows, "income", id)?.color || "";
    if (isKeptColor(category)) return shownColor(id, category);
    return shownColor(id, legacy);
  }
  if (isKeptColor(saved)) return shownColor(id, saved);
  if (kind === "class") return classToken(id);
  if (kind === "series") {
    if (isKeptColor(legacy)) return shownColor(id, legacy);
    return SERIES_DEFAULT[id] || defaultToken(id);
  }
  return shownColor(id, legacy);
}

export function paintColors(rows: EntityColor[] | undefined, kind: string, id: string, color: string): EntityColor[] {
  const next = (rows || []).filter((row) => !(fold(row.kind) === fold(kind) && fold(row.id) === fold(id)));
  const trimmed = color.trim();
  if (!trimmed) return next;
  next.push({ kind, id, color: trimmed.startsWith("var(") ? trimmed : normalizeHex(trimmed) || trimmed });
  return next;
}

export function namedColor(word: string): string {
  const token = fold(word).replace(/[^a-z]/g, "");
  return NAMED.find((row) => row.word === token)?.hex || (/^#[0-9A-Fa-f]{6}$/.test(word.trim()) ? normalizeHex(word) : "");
}

export function colorWord(color: string): string {
  const hex = tokenToHex(color);
  const series = SERIES_HEX.indexOf(hex as (typeof SERIES_HEX)[number]);
  if (series >= 0) return HUE[series];
  if (hex === "#009E73") return "green";
  if (hex === "#0072B2") return "blue";
  if (hex === "#8A8A8A") return "gray";
  if (hex === "#E69F00") return "orange";
  if (hex === "#C44E00") return "red";
  return hex;
}

export function contrastRatio(hex: string, canvas: string): number {
  const lum = (value: string) => {
    const raw = normalizeHex(value).slice(1);
    const channel = (pair: string) => {
      const unit = parseInt(pair, 16) / 255;
      return unit <= 0.04045 ? unit / 12.92 : ((unit + 0.055) / 1.055) ** 2.4;
    };
    const r = channel(raw.slice(0, 2));
    const g = channel(raw.slice(2, 4));
    const b = channel(raw.slice(4, 6));
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  };
  const left = lum(hex);
  const right = lum(canvas);
  const hi = Math.max(left, right);
  const lo = Math.min(left, right);
  return (hi + 0.05) / (lo + 0.05);
}
