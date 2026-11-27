export interface ShareRow {
  id: string;
  amount: number | null;
  /** The portfolio total. It reads 100% and is not part of the denominator. */
  whole?: boolean;
  omit?: boolean;
  hidden?: boolean;
  unpriced?: boolean;
}

/**
 * One-decimal shares of the visible parts.
 * A whole row is 100. Parts are shares of their own sum, so they add to 100.
 */
export function legendShares(rows: ShareRow[]): Map<string, number | null> {
  const parts = rows.filter((row) => !row.hidden && !row.omit && !row.whole && !row.unpriced && row.amount != null && row.amount > 0);
  const total = parts.reduce((sum, row) => sum + (row.amount || 0), 0);
  const rounded = roundShares(
    parts.map((row) => ({ id: row.id, exact: total > 0 ? ((row.amount || 0) / total) * 100 : 0 })),
    100,
  );
  const out = new Map<string, number | null>();
  for (const row of rows) {
    if (row.whole && !row.hidden && !row.unpriced && row.amount != null && (row.amount > 0 || total > 0)) {
      out.set(row.id, 100);
      continue;
    }
    if (row.hidden || row.omit || row.unpriced || row.amount == null || total <= 0 || row.amount <= 0) {
      out.set(row.id, null);
      continue;
    }
    out.set(row.id, rounded.get(row.id) ?? 0);
  }
  return out;
}

/** Largest remainder so the rounded figures add up to `target`. */
function roundShares(rows: Array<{ id: string; exact: number }>, target: number): Map<string, number> {
  const factor = 10;
  const scaled = rows.map((row) => {
    const exact = row.exact * factor;
    const floor = Math.floor(exact + 1e-9);
    return { id: row.id, floor, rest: exact - floor };
  });
  let left = Math.round(target * factor) - scaled.reduce((sum, row) => sum + row.floor, 0);
  const order = [...scaled].sort((a, b) => b.rest - a.rest || a.id.localeCompare(b.id));
  const extra = new Set<string>();
  for (const row of order) {
    if (left <= 0) break;
    extra.add(row.id);
    left -= 1;
  }
  const map = new Map<string, number>();
  for (const row of scaled) map.set(row.id, (row.floor + (extra.has(row.id) ? 1 : 0)) / factor);
  return map;
}
