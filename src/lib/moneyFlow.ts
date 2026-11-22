import { convertMinor, type RateBook } from "./financeMath.ts";
import { inBounds } from "./chartPeriod.ts";
import type { CategoryRecord } from "./financeCategories.ts";

export interface FlowTx {
  amountMinor: number;
  currency: string;
  direction: string;
  category: string;
  subcategory?: string;
  /** Free-text description. It never becomes a sankey node. */
  notes?: string;
  date: string;
  accountKind?: string;
  fromKind?: string;
  /** Account the money left. Used when there is no income, and in by-source grouping. */
  accountName?: string;
}

export interface FlowLabels {
  income: string;
  drawn: string;
  total: string;
  surplus: string;
  other: string;
  uncategorized: string;
  debt: string;
  savings: string;
  investments: string;
  card: string;
  cash: string;
  bank: string;
  spending: string;
}

export interface FlowOriginal {
  amountMinor: number;
  currency: string;
}

export interface FlowNode {
  id: string;
  label: string;
  role: "source" | "pool" | "sink" | "child";
  kind: string;
  amountMinor: number;
  /** Share of income. Above 1 when spending was funded from savings. */
  share: number;
  color: string;
  originals: FlowOriginal[];
  members: Array<{ id: string; label: string; amountMinor: number; originals: FlowOriginal[] }>;
  /** Subcategories. Empty when this category has no split. */
  children: Array<{ id: string; label: string; amountMinor: number; originals: FlowOriginal[] }>;
}

export interface FlowLink {
  source: string;
  target: string;
  amountMinor: number;
}

export type FlowGroup = "category" | "source";

export type MoneyFlow =
  | { state: "empty" }
  | { state: "needs-rate"; missing: string[] }
  | { state: "ready"; currency: string; incomeMinor: number; nodes: FlowNode[]; links: FlowLink[]; uncategorizedMinor: number };

const DEFAULT_LABELS: FlowLabels = {
  income: "Income",
  drawn: "From savings",
  total: "Total",
  surplus: "Surplus",
  other: "Other",
  uncategorized: "Uncategorized",
  debt: "Debt payments",
  savings: "Savings",
  investments: "Investing",
  card: "Card",
  cash: "Cash",
  bank: "Bank",
  spending: "Spending",
};

function fold(value: string): string {
  return value.trim().toLowerCase();
}

function categoryOf(categories: CategoryRecord[], name: string): CategoryRecord | undefined {
  return categories.find((row) => fold(row.name) === fold(name));
}

interface Bucket {
  id: string;
  label: string;
  kind: string;
  amount: number;
  originals: Map<string, number>;
  order: number;
  children: Map<string, Bucket>;
}

function remember(bucket: Bucket, converted: number, original: number, currency: string, display: string) {
  bucket.amount += converted;
  const code = currency.toUpperCase();
  if (!code || code === display) return;
  bucket.originals.set(code, (bucket.originals.get(code) || 0) + original);
}

function originalsOf(bucket: Bucket): FlowOriginal[] {
  return [...bucket.originals.entries()]
    .map(([currency, amountMinor]) => ({ currency, amountMinor }))
    .sort((a, b) => a.currency.localeCompare(b.currency));
}

/**
 * Income sources flow into one income pool, then out to expenses, savings, investing, and debt payments.
 * A card charge is an expense. Paying that card is cash leaving, not a second expense.
 * Amounts are converted into `display`. A missing rate stops the chart instead of assuming 1:1.
 */
export function buildMoneyFlow(
  txs: FlowTx[],
  opts: {
    categories?: CategoryRecord[];
    display: string;
    book: RateBook | null;
    nowMs: number;
    start?: string;
    end?: string;
    threshold?: number;
    expandedOther?: boolean;
    nodeColors?: Record<string, string>;
    labels?: Partial<FlowLabels>;
    /** Category groups by category and subcategory. Source groups the outflows by account. */
    group?: FlowGroup;
  },
): MoneyFlow {
  const display = opts.display.trim().toUpperCase();
  const labels = { ...DEFAULT_LABELS, ...opts.labels };
  const categories = opts.categories || [];
  const threshold = opts.threshold ?? 0.03;
  const missing = new Set<string>();
  const sources = new Map<string, Bucket>();
  const sinks = new Map<string, Bucket>();
  const funds = new Map<string, Bucket>();
  const group = opts.group === "source" ? "source" : "category";

  const paint = (map: Map<string, Bucket>, id: string, label: string, kind: string, order: number) => {
    const found = map.get(id);
    if (found) return found;
    const bucket: Bucket = { id, label, kind, amount: 0, originals: new Map(), order, children: new Map() };
    map.set(id, bucket);
    return bucket;
  };
  const paintChild = (parent: Bucket, label: string) => {
    const id = `sub:${parent.id}:${fold(label)}`;
    const found = parent.children.get(id);
    if (found) return found;
    const bucket: Bucket = { id, label, kind: "child", amount: 0, originals: new Map(), order: parent.children.size, children: new Map() };
    parent.children.set(id, bucket);
    return bucket;
  };

  for (const tx of txs) {
    if (!tx.currency || !tx.amountMinor || tx.amountMinor <= 0) continue;
    if (!inBounds(tx.date, opts.start || "", opts.end || "")) continue;
    const saved = categoryOf(categories, tx.category);
    const kind = sinkKind(tx, saved?.kind || "");
    if (kind === "ignore") continue;
    const converted = convertMinor(tx.amountMinor, tx.currency, display, opts.book, opts.nowMs);
    if (!converted) {
      missing.add(tx.currency.toUpperCase());
      continue;
    }
    const placed = placeCategory(tx, categories, labels);
    const fundName = fundLabel(tx, labels);
    if (kind === "income") {
      const bucket = paint(sources, `in:${fold(placed.source)}`, placed.source, "income", saved?.order ?? 100);
      remember(bucket, converted.amountMinor, tx.amountMinor, tx.currency, display);
      continue;
    }
    const fund = paint(funds, `in:fund:${fold(fundName)}`, fundName, "fund", funds.size);
    remember(fund, converted.amountMinor, tx.amountMinor, tx.currency, display);
    const label = group === "source"
      ? fundName
      : kind === "debt" && !tx.category.trim() && !tx.subcategory?.trim()
        ? labels.debt
        : kind === "savings" && !tx.category.trim()
          ? labels.savings
          : kind === "investment" && !tx.category.trim()
            ? labels.investments
            : placed.parent;
    const id = `out:${kind}:${fold(label)}`;
    const bucket = paint(sinks, id, label, kind, saved?.order ?? 100);
    remember(bucket, converted.amountMinor, tx.amountMinor, tx.currency, display);
    if (group !== "source" && placed.child) {
      const child = paintChild(bucket, placed.child);
      remember(child, converted.amountMinor, tx.amountMinor, tx.currency, display);
    }
  }

  if (missing.size > 0) return { state: "needs-rate", missing: [...missing].sort() };
  if (sources.size === 0 && sinks.size === 0) return { state: "empty" };

  const incomeMinor = [...sources.values()].reduce((sum, row) => sum + row.amount, 0);
  let sinkTotal = [...sinks.values()].reduce((sum, row) => sum + row.amount, 0);
  if (incomeMinor === 0 && sinkTotal > 0) {
    // No income was recorded. The money left the accounts that paid, not a single savings plug.
    for (const row of funds.values()) {
      if (row.amount > 0) sources.set(row.id, row);
    }
  } else if (sinkTotal > incomeMinor) {
    const drawn = paint(sources, "in:drawn", labels.drawn, "drawn", -1);
    remember(drawn, sinkTotal - incomeMinor, 0, display, display);
  } else if (incomeMinor > sinkTotal) {
    const surplus = paint(sinks, "out:surplus", labels.surplus, "surplus", 10_000);
    remember(surplus, incomeMinor - sinkTotal, 0, display, display);
    sinkTotal = incomeMinor;
  }
  const pool = Math.max(incomeMinor, sinkTotal);
  const shareBase = incomeMinor > 0 ? incomeMinor : pool || 1;

  let sinkList = [...sinks.values()].filter((row) => row.amount > 0);
  if (!opts.expandedOther) {
    const small = sinkList.filter((row) => row.kind !== "surplus" && row.amount / (pool || 1) < threshold);
    const large = sinkList.filter((row) => !small.includes(row));
    if (small.length > 0 && large.length > 0) {
      const other: Bucket = { id: "out:other", label: labels.other, kind: "other", amount: 0, originals: new Map(), order: 9_000, children: new Map() };
      for (const row of small) {
        other.amount += row.amount;
        for (const [code, amount] of row.originals) other.originals.set(code, (other.originals.get(code) || 0) + amount);
      }
      other.order = 9_000;
      sinkList = [...large, other];
      sinks.clear();
      for (const row of sinkList) sinks.set(row.id, row);
      (other as Bucket & { members?: Bucket[] }).amount = other.amount;
      Object.assign(other, { members: small });
    }
  }
  let uncategorizedMinor = [...sinks.values()].filter((row) => fold(row.label) === fold(labels.uncategorized)).reduce((sum, row) => sum + row.amount, 0);
  const otherBucket = sinks.get("out:other");
  if (otherBucket && uncategorizedMinor > 0 && uncategorizedMinor / (pool || 1) < 0.08) {
    const unc = [...sinks.values()].find((row) => fold(row.label) === fold(labels.uncategorized));
    if (unc) {
      otherBucket.amount += unc.amount;
      for (const [code, amount] of unc.originals) otherBucket.originals.set(code, (otherBucket.originals.get(code) || 0) + amount);
      const members = (otherBucket as Bucket & { members?: Bucket[] }).members || [];
      members.push(unc);
      Object.assign(otherBucket, { members });
      sinks.delete(unc.id);
    }
  }

  const colorFor = (bucket: Bucket, accent: boolean) => {
    const custom = opts.nodeColors?.[bucket.id] || opts.nodeColors?.[bucket.label];
    if (custom && (/^#[0-9A-Fa-f]{6}$/.test(custom) || custom.startsWith("var("))) return custom;
    if (bucket.id === "pool" || accent) return "var(--brand)";
    return "var(--text-tertiary)";
  };

  const toNode = (bucket: Bucket, role: FlowNode["role"]): FlowNode => {
    const members = ((bucket as Bucket & { members?: Bucket[] }).members || []).map((row) => ({
      id: row.id,
      label: row.label,
      amountMinor: row.amount,
      originals: originalsOf(row),
    }));
    return {
      id: bucket.id,
      label: bucket.label,
      role,
      kind: bucket.kind,
      amountMinor: bucket.amount,
      share: bucket.amount / shareBase,
      color: colorFor(bucket, bucket.id === accentId),
      originals: originalsOf(bucket),
      members,
      children: childRows(bucket),
    };
  };
  const childRows = (bucket: Bucket) => {
    const kids = [...bucket.children.values()].filter((row) => row.amount > 0);
    const covered = kids.reduce((sum, row) => sum + row.amount, 0);
    // A partial split would invent an Uncategorized slice beside the real subcategory.
    if (kids.length === 0 || covered !== bucket.amount) return [];
    return kids.map((row) => ({ id: row.id, label: row.label, amountMinor: row.amount, originals: originalsOf(row) }));
  };

  const sourceNodes = [...sources.values()].filter((row) => row.amount > 0).sort((a, b) => a.order - b.order || b.amount - a.amount);
  const sinkNodes = [...sinks.values()].filter((row) => row.amount > 0).sort((a, b) => a.order - b.order || b.amount - a.amount);
  const accentId = [...sourceNodes].filter((row) => row.kind !== "drawn").sort((a, b) => b.amount - a.amount)[0]?.id || "";
  const poolNode: FlowNode = {
    id: "pool",
    label: labels.total,
    role: "pool",
    kind: "pool",
    amountMinor: pool,
    share: incomeMinor > 0 ? pool / incomeMinor : 1,
    color: colorFor({ id: "pool", label: labels.total, kind: "pool", amount: pool, originals: new Map(), order: 0, children: new Map() }, true),
    originals: [],
    members: [],
    children: [],
  };
  const categoryNodes = sinkNodes.map((row) => toNode(row, "sink"));
  const childNodes: FlowNode[] = categoryNodes.flatMap((node) =>
    node.children.map((child) => ({
      id: child.id,
      label: child.label,
      role: "child" as const,
      kind: "child",
      amountMinor: child.amountMinor,
      share: child.amountMinor / shareBase,
      color: "var(--text-tertiary)",
      originals: child.originals,
      members: [],
      children: [],
    })),
  );
  const nodes = [...sourceNodes.map((row) => toNode(row, "source")), poolNode, ...categoryNodes, ...childNodes];
  const links: FlowLink[] = [
    ...sourceNodes.map((row) => ({ source: row.id, target: "pool", amountMinor: row.amount })),
    ...sinkNodes.map((row) => ({ source: "pool", target: row.id, amountMinor: row.amount })),
    ...categoryNodes.flatMap((node) => node.children.map((child) => ({ source: node.id, target: child.id, amountMinor: child.amountMinor }))),
  ];
  return { state: "ready", currency: display, incomeMinor, nodes, links, uncategorizedMinor };
}

const SUGGESTED = new Set([
  "salary",
  "freelance",
  "dividends",
  "interest",
  "gifts",
  "refunds",
  "other income",
  "housing",
  "food",
  "groceries",
  "transport",
  "subscriptions",
  "health",
  "savings",
  "investing",
  "debt payments",
  "uncategorized",
]);

const HINTS: Array<{ re: RegExp; name: string }> = [
  { re: /vacuum|cleaner|purifier|\bair\b|rent|alquiler|miete|lloguer|renda|housing|habitatge|vivienda/, name: "Housing" },
  { re: /groc|supermerc|mercad/, name: "Groceries" },
  { re: /food|comida|essen|restaurant|coffee|cafe|café/, name: "Food" },
  { re: /uber|taxi|transport|transit|\bbus\b/, name: "Transport" },
  { re: /salary|sueldo|gehalt|nomina|nòmina/, name: "Salary" },
];

/** A product phrase such as "vacuum cleaner" is a description, not a category. A one-word category such as Coffee stays. */
function looksDescribed(name: string): boolean {
  const folded = fold(name);
  if (!folded || SUGGESTED.has(folded)) return false;
  const words = folded.split(/\s+/).filter(Boolean);
  if (words.length >= 2) return true;
  return /^(vacuum|cleaner|purifier)$/.test(folded);
}

function hintedName(text: string): string {
  const folded = fold(text);
  if (!folded) return "";
  return HINTS.find((row) => row.re.test(folded))?.name || "";
}

function fundLabel(tx: FlowTx, labels: FlowLabels): string {
  const named = (tx.accountName || "").trim();
  if (named) return named;
  if (tx.accountKind === "credit_card") return labels.card;
  if (tx.accountKind === "cash") return labels.cash;
  if (tx.accountKind === "bank") return labels.bank;
  if (tx.accountKind === "investment") return labels.investments;
  return labels.spending;
}

/**
 * By-category nodes are catalog categories and their subcategories.
 * A description stored in the category field is mapped onto a real category, or Uncategorized.
 */
function placeCategory(tx: FlowTx, categories: CategoryRecord[], labels: FlowLabels): { source: string; parent: string; child: string } {
  const named = tx.category.trim();
  const sub = (tx.subcategory || "").trim();
  const notes = (tx.notes || "").trim();
  const record = named ? categoryOf(categories, named) : undefined;
  let parent = "";
  let child = "";
  if (record?.parent) {
    const owner = categoryOf(categories, record.parent);
    parent = owner?.name || record.parent;
    child = record.name;
  } else if (record && !record.parent && !looksDescribed(record.name)) {
    parent = record.name;
  } else if (looksDescribed(named) || (!named && (looksDescribed(sub) || looksDescribed(notes)))) {
    const hint = hintedName(`${named} ${sub} ${notes}`);
    parent = hint ? categoryOf(categories, hint)?.name || hint : labels.uncategorized;
  } else if (named && !looksDescribed(named)) {
    parent = record?.name || named;
  } else {
    parent = labels.uncategorized;
  }
  if (!parent.trim()) parent = labels.uncategorized;
  if (!child && sub && !looksDescribed(sub)) {
    const subRecord = categoryOf(categories, sub);
    if (!subRecord?.parent || fold(subRecord.parent) === fold(parent)) child = subRecord?.name || sub;
  }
  if (child && fold(child) === fold(parent)) child = "";
  const source = child && !record?.parent && fold(named) === fold(parent) ? child : parent;
  return { source: kindIncomeLabel(tx, source, labels), parent, child };
}

function kindIncomeLabel(tx: FlowTx, source: string, labels: FlowLabels): string {
  if (tx.direction !== "income") return source;
  const sub = (tx.subcategory || "").trim();
  return sub || source || labels.uncategorized;
}

function sinkKind(tx: FlowTx, categoryKind: string): "income" | "expense" | "savings" | "investment" | "debt" | "ignore" {
  if (tx.direction === "income") return "income";
  if (tx.direction === "payment") return "debt";
  if (tx.direction === "transfer") {
    if (tx.accountKind === "investment" || categoryKind === "investment") return "investment";
    if (categoryKind === "savings") return "savings";
    return "ignore";
  }
  if (tx.direction !== "expense") return "ignore";
  if (categoryKind === "savings") return "savings";
  if (categoryKind === "investment") return "investment";
  return "expense";
}

/**
 * Plot box for an ECharts sankey. The diagram is capped and centered so one flow
 * does not become a slab the full height of the chart. `floor` is the value share
 * that should still paint about 2px.
 */
export function sankeyFrame(chartHeight: number, columnCounts: number[]): { top: number; bottom: number; plot: number; nodeWidth: number; nodeGap: number; floorShare: number } {
  const height = Math.max(160, chartHeight);
  const nodeWidth = 12;
  const nodeGap = 14;
  const cap = Math.round(height * 0.62);
  const plot = Math.max(120, Math.min(cap, height - 36));
  const spare = Math.max(0, height - plot);
  const top = Math.round(spare / 2);
  const bottom = height - plot - top;
  const busiest = Math.max(1, ...columnCounts, 1);
  const usable = Math.max(2, plot - Math.max(0, busiest - 1) * nodeGap);
  const floorShare = 2 / usable;
  return { top, bottom, plot, nodeWidth, nodeGap, floorShare };
}

/** Keep a tiny flow visible without letting it dominate the column. */
export function sankeyValue(amount: number, total: number, floorShare: number): number {
  if (amount <= 0 || total <= 0) return 0;
  const floor = total * floorShare;
  return amount < floor ? floor : amount;
}

/** Inflow equals outflow on every node that both receives and sends. Sources only send. Terminal nodes only receive. */
export function flowConserved(flow: MoneyFlow): boolean {
  if (flow.state !== "ready") return false;
  for (const node of flow.nodes) {
    const inn = flow.links.filter((link) => link.target === node.id).reduce((sum, link) => sum + link.amountMinor, 0);
    const out = flow.links.filter((link) => link.source === node.id).reduce((sum, link) => sum + link.amountMinor, 0);
    if (node.role === "source" && (inn !== 0 || out !== node.amountMinor)) return false;
    if (node.role === "pool" && (inn !== node.amountMinor || out !== node.amountMinor)) return false;
    if (node.role === "sink" && inn !== node.amountMinor) return false;
    if (node.role === "sink" && out !== 0 && out !== node.amountMinor) return false;
    if (node.role === "child" && (inn !== node.amountMinor || out !== 0)) return false;
  }
  return true;
}

export interface SankeyBox {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
  barY: number;
  barH: number;
}

export interface SankeyRibbon {
  source: string;
  target: string;
  amountMinor: number;
  d: string;
}

/**
 * Vertical bands. Bar width is the value. A ribbon's two ends are the same width and sit on the bar edges.
 * `columns` lists node ids from top to bottom.
 */
export function layoutSankey(columns: string[][], amounts: Record<string, number>, links: FlowLink[], width: number): { height: number; nodes: SankeyBox[]; ribbons: SankeyRibbon[] } {
  const max = Math.max(...Object.values(amounts), 1);
  const scale = (Math.max(width, 160) * 0.46) / max;
  const nodeH = 44;
  const barH = 14;
  const rowGap = 4;
  const bandGap = 28;
  const nodes: SankeyBox[] = [];
  let y = 0;
  columns.forEach((column, index) => {
    for (const id of column) {
      const amount = amounts[id] || 0;
      const w = amount <= 0 ? 0 : Math.max(8, amount * scale);
      nodes.push({ id, x: 0, y, w, h: nodeH, barY: y + (nodeH - barH) / 2, barH });
      y += nodeH + rowGap;
    }
    if (index < columns.length - 1) y += bandGap;
  });
  const box = new Map(nodes.map((node) => [node.id, node]));
  const ribbons: SankeyRibbon[] = [];
  for (let index = 0; index < columns.length - 1; index += 1) {
    const from = new Set(columns[index]);
    const to = new Set(columns[index + 1]);
    const band = links.filter((link) => from.has(link.source) && to.has(link.target) && link.amountMinor > 0);
    const outCursor = new Map<string, number>();
    const inCursor = new Map<string, number>();
    const ordered = [...band].sort((a, b) => (box.get(a.source)?.y || 0) - (box.get(b.source)?.y || 0) || (box.get(a.target)?.y || 0) - (box.get(b.target)?.y || 0));
    for (const link of ordered) {
      const source = box.get(link.source);
      const target = box.get(link.target);
      if (!source || !target) continue;
      const w = Math.max(1, link.amountMinor * scale);
      const x0 = source.x + (outCursor.get(link.source) || 0);
      const x1 = target.x + (inCursor.get(link.target) || 0);
      outCursor.set(link.source, (outCursor.get(link.source) || 0) + w);
      inCursor.set(link.target, (inCursor.get(link.target) || 0) + w);
      const y0 = source.barY + source.barH;
      const y1 = target.barY;
      ribbons.push({ source: link.source, target: link.target, amountMinor: link.amountMinor, d: ribbonPath(x0, y0, w, x1, y1, w) });
    }
  }
  return { height: Math.max(y, nodeH), nodes, ribbons };
}

function ribbonPath(x0: number, y0: number, w0: number, x1: number, y1: number, w1: number): string {
  const mid = y0 + (y1 - y0) / 2;
  return `M ${x0} ${y0} L ${x0 + w0} ${y0} C ${x0 + w0} ${mid}, ${x1 + w1} ${mid}, ${x1 + w1} ${y1} L ${x1} ${y1} C ${x1} ${mid}, ${x0} ${mid}, ${x0} ${y0} Z`;
}

export function linkedIds(flow: Extract<MoneyFlow, { state: "ready" }>, id: string): Set<string> {
  const keep = new Set<string>([id]);
  if (id === "pool") {
    for (const link of flow.links) {
      keep.add(link.source);
      keep.add(link.target);
    }
    return keep;
  }
  for (const link of flow.links) {
    if (link.source === id || link.target === id) {
      keep.add(link.source);
      keep.add(link.target);
      keep.add("pool");
    }
  }
  return keep;
}
