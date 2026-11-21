export interface ChartItem {
  id: string;
  label: string;
  className?: string;
}

const KEY = "grok-chart-hidden";

function readAll(): Record<string, string[]> {
  try {
    const raw = localStorage.getItem(KEY);
    const parsed = raw ? (JSON.parse(raw) as Record<string, string[]>) : {};
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

export function readHidden(chartId: string): string[] {
  const rows = readAll()[chartId];
  return Array.isArray(rows) ? rows.filter((id) => typeof id === "string") : [];
}

export function writeHidden(chartId: string, hidden: string[]) {
  try {
    const all = readAll();
    all[chartId] = hidden;
    localStorage.setItem(KEY, JSON.stringify(all));
  } catch {
    /* The choice still applies for this visit. */
  }
}

function matches(item: ChartItem, name: string): boolean {
  const needle = name.trim().toLowerCase();
  if (!needle) return false;
  const label = item.label.trim().toLowerCase();
  const kind = (item.className || "").trim().toLowerCase();
  if (label === needle || kind === needle) return true;
  if (needle === "crypto" && kind === "crypto") return true;
  if (needle === "metals" || needle === "metal") return kind === "metals" || kind === "metal";
  if (needle === "stocks" || needle === "stock" || needle === "etf") return kind === "stocks" || kind === "stock";
  return label.includes(needle) || needle.includes(label);
}

/** Hidden ids after a voice or tap command. Unknown names leave the set unchanged. */
export function nextHidden(items: ChartItem[], hidden: string[], action: string, name = ""): { hidden: string[]; matched: boolean } {
  const ids = items.map((item) => item.id);
  const verb = action.trim().toLowerCase();
  if (verb === "reset" || verb === "all" || verb === "show all") return { hidden: [], matched: true };
  const hits = items.filter((item) => matches(item, name));
  if (hits.length === 0) return { hidden, matched: false };
  if (verb === "only" || verb === "show only") {
    const keep = new Set(hits.map((item) => item.id));
    return { hidden: ids.filter((id) => !keep.has(id)), matched: true };
  }
  if (verb === "show") {
    const drop = new Set(hits.map((item) => item.id));
    return { hidden: hidden.filter((id) => !drop.has(id)), matched: true };
  }
  const add = new Set(hidden);
  for (const hit of hits) add.add(hit.id);
  return { hidden: [...add], matched: true };
}
