export type PeriodPreset = "this" | "last" | "3m" | "6m" | "ytd" | "1y" | "all" | "custom";

export interface ChartPeriod {
  preset: PeriodPreset;
  start: string;
  end: string;
}

const KEY = "grok-chart-period";

export const PERIOD_PRESETS: PeriodPreset[] = ["this", "last", "3m", "6m", "ytd", "1y", "all", "custom"];

export function emptyPeriod(preset: PeriodPreset = "this"): ChartPeriod {
  return { preset, start: "", end: "" };
}

function readAll(): Record<string, ChartPeriod> {
  try {
    const raw = localStorage.getItem(KEY);
    const parsed = raw ? (JSON.parse(raw) as Record<string, ChartPeriod>) : {};
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

export function readPeriod(chartId: string, fallback: PeriodPreset = "this"): ChartPeriod {
  const saved = readAll()[chartId];
  if (!saved || !PERIOD_PRESETS.includes(saved.preset)) return emptyPeriod(fallback);
  return { preset: saved.preset, start: saved.start || "", end: saved.end || "" };
}

export function writePeriod(chartId: string, period: ChartPeriod) {
  try {
    const all = readAll();
    all[chartId] = period;
    localStorage.setItem(KEY, JSON.stringify(all));
  } catch {
    /* The choice still applies for this visit. */
  }
}

function iso(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** Inclusive calendar bounds. Empty strings mean no limit. */
export function boundsOf(period: ChartPeriod, today = new Date()): { start: string; end: string } {
  if (period.preset === "custom") return { start: period.start || "", end: period.end || "" };
  if (period.preset === "all") return { start: "", end: "" };
  if (period.preset === "last") {
    const start = new Date(today.getFullYear(), today.getMonth() - 1, 1);
    const end = new Date(today.getFullYear(), today.getMonth(), 0);
    return { start: iso(start), end: iso(end) };
  }
  if (period.preset === "ytd") return { start: `${today.getFullYear()}-01-01`, end: iso(today) };
  if (period.preset === "1y") {
    const start = new Date(today.getFullYear() - 1, today.getMonth(), today.getDate());
    return { start: iso(start), end: iso(today) };
  }
  if (period.preset === "3m" || period.preset === "6m") {
    const months = period.preset === "3m" ? 2 : 5;
    const start = new Date(today.getFullYear(), today.getMonth() - months, 1);
    const end = new Date(today.getFullYear(), today.getMonth() + 1, 0);
    return { start: iso(start), end: iso(end) };
  }
  const start = new Date(today.getFullYear(), today.getMonth(), 1);
  const end = new Date(today.getFullYear(), today.getMonth() + 1, 0);
  return { start: iso(start), end: iso(end) };
}

export function inBounds(date: string, start: string, end: string): boolean {
  if (start && date < start) return false;
  if (end && date > end) return false;
  return true;
}

export function monthsIn(start: string, end: string, today = new Date()): string[] {
  const bounds = { start, end };
  const cursor = new Date(today.getFullYear(), today.getMonth(), 1);
  const keys: string[] = [];
  for (let i = 0; i < 24; i += 1) {
    const key = `${cursor.getFullYear()}-${String(cursor.getMonth() + 1).padStart(2, "0")}`;
    const monthStart = `${key}-01`;
    const monthEnd = iso(new Date(cursor.getFullYear(), cursor.getMonth() + 1, 0));
    const overlaps = inBounds(monthEnd, bounds.start, bounds.end) && inBounds(monthStart, "", bounds.end || "9999-12-31") && (!bounds.start || monthEnd >= bounds.start);
    if (overlaps) keys.push(key);
    cursor.setMonth(cursor.getMonth() - 1);
  }
  return keys.reverse().slice(-12);
}

const PERIOD_WORDS: Record<string, PeriodPreset> = {
  this: "this",
  "this month": "this",
  current: "this",
  last: "last",
  "last month": "last",
  "3m": "3m",
  "3 months": "3m",
  "6m": "6m",
  "6 months": "6m",
  ytd: "ytd",
  "year to date": "ytd",
  "1y": "1y",
  "1 year": "1y",
  year: "1y",
  all: "all",
  custom: "custom",
};

export function periodFromVoice(raw: string): PeriodPreset | null {
  const text = raw.trim().toLowerCase();
  return PERIOD_WORDS[text] || null;
}
