export interface ValuePoint {
  accountId: string;
  date: string;
  quantity: string;
  valueMinor: number;
  currency: string;
  nativeMinor?: number;
  nativeCurrency?: string;
  source: string;
  name: string;
  className?: string;
}

export type RangeId = "1m" | "3m" | "1y" | "all";

export function rangeStart(today: string, range: RangeId): string {
  if (range === "all" || today.length < 10) return "";
  const [year, month, day] = today.split("-").map(Number);
  const date = new Date(Date.UTC(year, (month || 1) - 1, day || 1));
  if (range === "1m") date.setUTCDate(date.getUTCDate() - 30);
  else if (range === "3m") date.setUTCDate(date.getUTCDate() - 90);
  else date.setUTCDate(date.getUTCDate() - 365);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getUTCFullYear()}-${pad(date.getUTCMonth() + 1)}-${pad(date.getUTCDate())}`;
}

export interface SeriesPoint {
  date: string;
  valueMinor: number;
}

/** One point per day. Later rows for the same holding and day replace earlier ones. */
export function seriesFor(points: ValuePoint[], ids: string[], from: string): SeriesPoint[] {
  const keep = new Set(ids);
  const byDay = new Map<string, Map<string, number>>();
  const ordered = [...points].sort((a, b) => a.date.localeCompare(b.date));
  for (const point of ordered) {
    if (!keep.has(point.accountId) || point.valueMinor == null) continue;
    if (from && point.date < from) continue;
    const day = byDay.get(point.date) ?? new Map<string, number>();
    day.set(point.accountId, point.valueMinor);
    byDay.set(point.date, day);
  }
  const days = [...byDay.keys()].sort();
  const carried = new Map<string, number>();
  const out: SeriesPoint[] = [];
  for (const date of days) {
    const day = byDay.get(date);
    if (!day) continue;
    for (const [id, value] of day) carried.set(id, value);
    let total = 0;
    for (const id of ids) {
      if (carried.has(id)) total += carried.get(id) || 0;
    }
    out.push({ date, valueMinor: total });
  }
  return out;
}

/**
 * First day on which every holding in `ids` has a saved point.
 * Earlier days are partial snapshots and must not be the baseline of a percent change.
 */
export function firstCompleteDate(points: ValuePoint[], ids: string[], from: string): string {
  const wanted = new Set(ids);
  if (wanted.size === 0) return "";
  const seen = new Set<string>();
  const days = [...new Set(points.filter((point) => wanted.has(point.accountId) && (!from || point.date >= from)).map((point) => point.date))].sort();
  for (const date of days) {
    for (const point of points) {
      if (point.date === date && wanted.has(point.accountId)) seen.add(point.accountId);
    }
    if (seen.size === wanted.size) return date;
  }
  return "";
}

/** The series from the first complete snapshot. Empty when a holding in `ids` never appears. */
export function completeSeries(points: ValuePoint[], ids: string[], from: string): SeriesPoint[] {
  const start = firstCompleteDate(points, ids, from);
  if (!start) return [];
  return seriesFor(points, ids, start > from ? start : from);
}

export function changeOf(points: SeriesPoint[]): { amountMinor: number; percent: number; from: string; to: string } | null {
  if (points.length < 2) return null;
  const start = points[0];
  const end = points[points.length - 1];
  const amountMinor = end.valueMinor - start.valueMinor;
  const percent = start.valueMinor !== 0 ? (amountMinor / Math.abs(start.valueMinor)) * 100 : 0;
  return { amountMinor, percent, from: start.date, to: end.date };
}
