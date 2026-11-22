import { convertMinor, type RateBook } from "./financeMath.ts";

export const DASH_IDS = ["hero", "result", "change", "next", "debt", "categories", "flow", "pace", "attention"] as const;
export type DashId = (typeof DASH_IDS)[number];
export type DashPreset = "this" | "last" | "3m" | "ytd" | "1y" | "custom";

export interface DateRange {
  start: string;
  end: string;
}

export interface MoneyTx {
  amountMinor: number;
  currency: string;
  direction: string;
  date: string;
  category?: string;
}

const PRESETS: DashPreset[] = ["this", "last", "3m", "ytd", "1y", "custom"];

export function isDashPreset(value: string): value is DashPreset {
  return (PRESETS as string[]).includes(value);
}

function iso(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function parse(isoDate: string): Date {
  const [year, month, day] = isoDate.split("-").map(Number);
  return new Date(year, (month || 1) - 1, day || 1);
}

/** Inclusive bounds for the dashboard period. This month is the calendar month. */
export function dashboardRange(preset: DashPreset, today: Date, custom: DateRange = { start: "", end: "" }): DateRange {
  if (preset === "custom") return { start: custom.start || "", end: custom.end || "" };
  if (preset === "last") {
    return { start: iso(new Date(today.getFullYear(), today.getMonth() - 1, 1)), end: iso(new Date(today.getFullYear(), today.getMonth(), 0)) };
  }
  if (preset === "3m") {
    return { start: iso(new Date(today.getFullYear(), today.getMonth() - 2, 1)), end: iso(new Date(today.getFullYear(), today.getMonth() + 1, 0)) };
  }
  if (preset === "ytd") return { start: `${today.getFullYear()}-01-01`, end: iso(today) };
  if (preset === "1y") return { start: iso(new Date(today.getFullYear() - 1, today.getMonth(), today.getDate())), end: iso(today) };
  return { start: iso(new Date(today.getFullYear(), today.getMonth(), 1)), end: iso(new Date(today.getFullYear(), today.getMonth() + 1, 0)) };
}

/** The window of the same length immediately before the current one. */
export function previousRange(preset: DashPreset, today: Date, current: DateRange): DateRange | null {
  if (!current.start || !current.end) return null;
  if (preset === "this") return dashboardRange("last", today);
  if (preset === "last") {
    return { start: iso(new Date(today.getFullYear(), today.getMonth() - 2, 1)), end: iso(new Date(today.getFullYear(), today.getMonth() - 1, 0)) };
  }
  if (preset === "3m") {
    return { start: iso(new Date(today.getFullYear(), today.getMonth() - 5, 1)), end: iso(new Date(today.getFullYear(), today.getMonth() - 2, 0)) };
  }
  if (preset === "ytd") {
    const end = new Date(today.getFullYear() - 1, today.getMonth(), today.getDate());
    return { start: `${today.getFullYear() - 1}-01-01`, end: iso(end) };
  }
  const start = parse(current.start);
  const end = parse(current.end);
  const days = Math.round((end.getTime() - start.getTime()) / 86_400_000) + 1;
  if (!Number.isFinite(days) || days <= 0) return null;
  const prevEnd = new Date(start);
  prevEnd.setDate(prevEnd.getDate() - 1);
  const prevStart = new Date(prevEnd);
  prevStart.setDate(prevStart.getDate() - (days - 1));
  return { start: iso(prevStart), end: iso(prevEnd) };
}

export function comparisonKey(preset: DashPreset): string {
  if (preset === "this") return "vs last month";
  if (preset === "last") return "vs the month before";
  if (preset === "3m") return "vs the previous 3 months";
  if (preset === "ytd" || preset === "1y") return "vs a year earlier";
  return "vs the previous period";
}

/** Null when the earlier figure is zero. A percent is not invented. */
export function changeRatio(current: number, previous: number): number | null {
  if (previous === 0) return null;
  return (current - previous) / previous;
}

/** Income minus expenses, divided by income. Hidden when income is zero. */
export function savingsRate(income: number, expenses: number): number | null {
  if (income <= 0) return null;
  return (income - expenses) / income;
}

export interface Total {
  amountMinor: number | null;
  missing: string[];
}

export function totalInRange(rows: MoneyTx[], direction: string, range: DateRange, display: string, book: RateBook | null, nowMs: number): Total {
  const target = display.trim().toUpperCase();
  let amount = 0;
  const missing = new Set<string>();
  let seen = false;
  for (const row of rows) {
    if (row.direction !== direction || !row.currency || row.amountMinor <= 0) continue;
    if (range.start && row.date < range.start) continue;
    if (range.end && row.date > range.end) continue;
    seen = true;
    const converted = convertMinor(row.amountMinor, row.currency, target, book, nowMs);
    if (!converted) missing.add(row.currency.toUpperCase());
    else amount += converted.amountMinor;
  }
  if (missing.size) return { amountMinor: null, missing: [...missing].sort() };
  return { amountMinor: seen ? amount : 0, missing: [] };
}

export interface CategoryBar {
  name: string;
  amountMinor: number;
}

export function topCategories(rows: MoneyTx[], range: DateRange, display: string, book: RateBook | null, nowMs: number, limit = 5): { rows: CategoryBar[]; missing: string[] } {
  const target = display.trim().toUpperCase();
  const by = new Map<string, number>();
  const missing = new Set<string>();
  for (const row of rows) {
    if (row.direction !== "expense" || !row.currency || row.amountMinor <= 0) continue;
    if (range.start && row.date < range.start) continue;
    if (range.end && row.date > range.end) continue;
    const converted = convertMinor(row.amountMinor, row.currency, target, book, nowMs);
    if (!converted) {
      missing.add(row.currency.toUpperCase());
      continue;
    }
    const name = row.category?.trim() || "Uncategorized";
    by.set(name, (by.get(name) || 0) + converted.amountMinor);
  }
  const ranked = [...by.entries()]
    .map(([name, amountMinor]) => ({ name, amountMinor }))
    .sort((a, b) => b.amountMinor - a.amountMinor || a.name.localeCompare(b.name))
    .slice(0, limit);
  return { rows: ranked, missing: [...missing].sort() };
}

export interface Pace {
  spentMinor: number;
  averageMinor: number;
  months: number;
  through: string;
}

function monthHasExpense(rows: MoneyTx[], start: string, end: string): boolean {
  return rows.some((row) => row.direction === "expense" && row.amountMinor > 0 && row.date >= start && row.date <= end);
}

/**
 * Spend from the first of the anchor month through the anchor day,
 * against the average of up to three earlier months on that same day-of-month.
 * Null when none of those earlier months has an expense.
 */
export function spendingPace(rows: MoneyTx[], anchor: Date, display: string, book: RateBook | null, nowMs: number): Pace | null {
  const day = anchor.getDate();
  const slices: number[] = [];
  for (let back = 1; back <= 3; back += 1) {
    const month = new Date(anchor.getFullYear(), anchor.getMonth() - back, 1);
    const dim = new Date(month.getFullYear(), month.getMonth() + 1, 0).getDate();
    const full = { start: iso(month), end: iso(new Date(month.getFullYear(), month.getMonth(), dim)) };
    if (!monthHasExpense(rows, full.start, full.end)) continue;
    const slice = { start: full.start, end: iso(new Date(month.getFullYear(), month.getMonth(), Math.min(day, dim))) };
    const total = totalInRange(rows, "expense", slice, display, book, nowMs);
    if (total.amountMinor == null) continue;
    slices.push(total.amountMinor);
  }
  if (!slices.length) return null;
  const currentDim = new Date(anchor.getFullYear(), anchor.getMonth() + 1, 0).getDate();
  const current = {
    start: iso(new Date(anchor.getFullYear(), anchor.getMonth(), 1)),
    end: iso(new Date(anchor.getFullYear(), anchor.getMonth(), Math.min(day, currentDim))),
  };
  const spent = totalInRange(rows, "expense", current, display, book, nowMs);
  if (spent.amountMinor == null) return null;
  const average = Math.round(slices.reduce((sum, value) => sum + value, 0) / slices.length);
  return { spentMinor: spent.amountMinor, averageMinor: average, months: slices.length, through: current.end };
}

export function normalizeLayout(order: string[], hidden: string[]): { order: DashId[]; hidden: DashId[] } {
  const next: DashId[] = [];
  for (const id of order) {
    if ((DASH_IDS as readonly string[]).includes(id) && !next.includes(id as DashId)) next.push(id as DashId);
  }
  for (const id of DASH_IDS) if (!next.includes(id)) next.push(id);
  const hide: DashId[] = [];
  for (const id of hidden) {
    if (next.includes(id as DashId) && !hide.includes(id as DashId)) hide.push(id as DashId);
  }
  return { order: next, hidden: hide };
}

export function moveSection(order: DashId[], id: DashId, direction: -1 | 1): DashId[] {
  const index = order.indexOf(id);
  const target = index + direction;
  if (index < 0 || target < 0 || target >= order.length) return order;
  const next = order.slice();
  const [item] = next.splice(index, 1);
  next.splice(target, 0, item);
  return next;
}

export function sectionFromVoice(name: string): DashId | null {
  const text = name.trim().toLowerCase();
  const table: Record<string, DashId> = {
    hero: "hero",
    "net worth": "hero",
    result: "result",
    "savings rate": "result",
    "net result": "result",
    change: "change",
    next: "next",
    "next 30 days": "next",
    debt: "debt",
    categories: "categories",
    "top categories": "categories",
    category: "categories",
    flow: "flow",
    sankey: "flow",
    "where my money went": "flow",
    pace: "pace",
    "spending pace": "pace",
    attention: "attention",
  };
  return table[text] || null;
}
