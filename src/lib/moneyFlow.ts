import { convertMinor, type RateBook } from "./financeMath.ts";
import { PALETTE } from "./financeView.ts";
import { inBounds } from "./chartPeriod.ts";
import type { CategoryRecord } from "./financeCategories.ts";

export interface FlowTx {
  amountMinor: number;
  currency: string;
  direction: string;
  category: string;
  date: string;
  accountKind?: string;
  fromKind?: string;
}

export interface FlowLabels {
  income: string;
  drawn: string;
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
  role: "source" | "pool" | "sink";
  kind: string;
  amountMinor: number;
  /** Share of income. Above 1 when spending was funded from savings. */
  share: number;
  color: string;
  originals: FlowOriginal[];
  members: Array<{ id: string; label: string; amountMinor: number; originals: FlowOriginal[] }>;
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
  drawn: "Drawn from savings",
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
    const bucket: Bucket = { id, label, kind, amount: 0, originals: new Map(), order };
    map.set(id, bucket);
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
    const name = tx.category.trim() || labels.uncategorized;
    if (kind === "income") {
      const bucket = paint(sources, `in:${fold(name)}`, name, "income", saved?.order ?? 100);
      remember(bucket, converted.amountMinor, tx.amountMinor, tx.currency, display);
      continue;
    }
    const label = kind === "debt" && !tx.category.trim() ? labels.debt : kind === "savings" && !tx.category.trim() ? labels.savings : kind === "investment" && !tx.category.trim() ? labels.investments : name;
    const id = `out:${kind}:${fold(label)}`;
    const bucket = paint(sinks, id, label, kind, saved?.order ?? 100);
    remember(bucket, converted.amountMinor, tx.amountMinor, tx.currency, display);
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
      const other: Bucket = { id: "out:other", label: labels.other, kind: "other", amount: 0, originals: new Map(), order: 9_000 };
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

  const colorFor = (bucket: Bucket, index: number) => {
    const custom = opts.nodeColors?.[bucket.id] || opts.nodeColors?.[bucket.label];
    if (custom && /^#[0-9A-Fa-f]{6}$/.test(custom)) return custom;
    const saved = categoryOf(categories, bucket.label);
    if (saved?.color && /^#[0-9A-Fa-f]{6}$/.test(saved.color)) return saved.color;
    if (bucket.kind === "drawn") return "#8C6A3F";
    if (bucket.kind === "surplus") return "#6E8B74";
    if (bucket.kind === "other") return "#7A6A58";
    if (bucket.id === "pool") return "#C4B5A0";
    return PALETTE[index % PALETTE.length];
  };

  const toNode = (bucket: Bucket, role: FlowNode["role"], index: number): FlowNode => {
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
      color: colorFor(bucket, index),
      originals: originalsOf(bucket),
      members,
    };
  };

  const sourceNodes = [...sources.values()].filter((row) => row.amount > 0).sort((a, b) => a.order - b.order || b.amount - a.amount);
  const sinkNodes = [...sinks.values()].filter((row) => row.amount > 0).sort((a, b) => a.order - b.order || b.amount - a.amount);
  const poolNode: FlowNode = {
    id: "pool",
    label: labels.income,
    role: "pool",
    kind: "pool",
    amountMinor: pool,
    share: incomeMinor > 0 ? pool / incomeMinor : 1,
    color: colorFor({ id: "pool", label: labels.income, kind: "pool", amount: pool, originals: new Map(), order: 0 }, 0),
    originals: [],
    members: [],
  };
  const nodes = [...sourceNodes.map((row, index) => toNode(row, "source", index)), poolNode, ...sinkNodes.map((row, index) => toNode(row, "sink", index))];
  const links: FlowLink[] = [
    ...sourceNodes.map((row) => ({ source: row.id, target: "pool", amountMinor: row.amount })),
    ...sinkNodes.map((row) => ({ source: "pool", target: row.id, amountMinor: row.amount })),
  ];
  return { state: "ready", currency: display, incomeMinor, nodes, links };
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
