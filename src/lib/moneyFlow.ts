import { convertMinor, type RateBook } from "./financeMath.ts";
import { inBounds } from "./chartPeriod.ts";
import type { CategoryRecord } from "./financeCategories.ts";

export interface FlowTx {
  amountMinor: number;
  currency: string;
  direction: string;
  category: string;
  subcategory?: string;
  date: string;
  accountKind?: string;
  fromKind?: string;
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

export type MoneyFlow =
  | { state: "empty" }
  | { state: "needs-rate"; missing: string[] }
  | { state: "ready"; currency: string; incomeMinor: number; nodes: FlowNode[]; links: FlowLink[] };

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
  },
): MoneyFlow {
  const display = opts.display.trim().toUpperCase();
  const labels = { ...DEFAULT_LABELS, ...opts.labels };
  const categories = opts.categories || [];
  const threshold = opts.threshold ?? 0.03;
  const missing = new Set<string>();
  const sources = new Map<string, Bucket>();
  const sinks = new Map<string, Bucket>();

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
    if (kind === "income") {
      const bucket = paint(sources, `in:${fold(placed.source)}`, placed.source, "income", saved?.order ?? 100);
      remember(bucket, converted.amountMinor, tx.amountMinor, tx.currency, display);
      continue;
    }
    const label = kind === "debt" && !tx.category.trim() && !tx.subcategory?.trim() ? labels.debt : kind === "savings" && !tx.category.trim() ? labels.savings : kind === "investment" && !tx.category.trim() ? labels.investments : placed.parent;
    const id = `out:${kind}:${fold(label)}`;
    const bucket = paint(sinks, id, label, kind, saved?.order ?? 100);
    remember(bucket, converted.amountMinor, tx.amountMinor, tx.currency, display);
    if (placed.child) {
      const child = paintChild(bucket, placed.child);
      remember(child, converted.amountMinor, tx.amountMinor, tx.currency, display);
    }
  }

  if (missing.size > 0) return { state: "needs-rate", missing: [...missing].sort() };
  if (sources.size === 0 && sinks.size === 0) return { state: "empty" };

  const incomeMinor = [...sources.values()].reduce((sum, row) => sum + row.amount, 0);
  let sinkTotal = [...sinks.values()].reduce((sum, row) => sum + row.amount, 0);
  if (sinkTotal > incomeMinor) {
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
    if (kids.length > 0 && covered < bucket.amount) {
      kids.push({
        id: `sub:${bucket.id}:${fold(labels.uncategorized)}`,
        label: labels.uncategorized,
        kind: "child",
        amount: bucket.amount - covered,
        originals: new Map(),
        order: kids.length,
        children: new Map(),
      });
    }
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
  return { state: "ready", currency: display, incomeMinor, nodes, links };
}

function placeCategory(tx: FlowTx, categories: CategoryRecord[], labels: FlowLabels): { source: string; parent: string; child: string } {
  const named = tx.category.trim();
  const sub = (tx.subcategory || "").trim();
  const record = named ? categoryOf(categories, named) : undefined;
  let parent = named || labels.uncategorized;
  let child = sub;
  if (record?.parent) {
    const owner = categoryOf(categories, record.parent);
    parent = owner?.name || record.parent;
    if (!child) child = record.name;
  }
  if (!parent.trim()) parent = labels.uncategorized;
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
