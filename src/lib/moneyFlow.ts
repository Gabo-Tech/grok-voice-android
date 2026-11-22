import { convertMinor, type RateBook } from "./financeMath.ts";
import { compactLabeled, flowPercent, minorExponent } from "./money.ts";
import { inBounds } from "./chartPeriod.ts";
import type { CategoryRecord } from "./financeCategories.ts";

export interface FlowTx {
  amountMinor: number;
  currency: string;
  direction: string;
  category: string;
  subcategory?: string;
  /** Free-text description. It never becomes a sankey node on its own. */
  notes?: string;
  date: string;
  accountKind?: string;
  fromKind?: string;
  /** Kept for callers. Unlinked payments are not drawn as a source named Spending. */
  accountName?: string;
}

export interface FlowLabels {
  income: string;
  totalIncome: string;
  totalSpending: string;
  shortfall: string;
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
  role: "source" | "pool" | "hub" | "sink" | "child";
  kind: string;
  amountMinor: number;
  /** Share of the column this node belongs to. Income sources share the inflow. Expenses share spending. */
  share: number;
  color: string;
  /** Rest, the grouped tail, is set in italic. */
  italic?: boolean;
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

export type FlowGroup = "category" | "entry";

export type MoneyFlow =
  | { state: "empty" }
  | { state: "needs-rate"; missing: string[] }
  | {
      state: "ready";
      currency: string;
      incomeMinor: number;
      spendingMinor: number;
      investmentMinor: number;
      /** Negative when spending and investments exceed income. */
      savingsMinor: number;
      noIncome: boolean;
      nodes: FlowNode[];
      links: FlowLink[];
      uncategorizedMinor: number;
    };

const DEFAULT_LABELS: FlowLabels = {
  income: "Income",
  totalIncome: "Total income",
  totalSpending: "Total spending",
  shortfall: "Shortfall / From savings",
  drawn: "From savings",
  total: "Total",
  surplus: "Surplus",
  other: "Rest",
  uncategorized: "Uncategorized",
  debt: "Debt payments",
  savings: "Savings",
  investments: "Investments",
  card: "Card",
  cash: "Cash",
  bank: "Bank",
  spending: "Spending",
};

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

const INCOME_COLORS = ["var(--flow-in-1)", "var(--flow-in-2)", "var(--flow-in-3)", "var(--flow-in-4)", "var(--flow-in-5)", "var(--flow-in-6)", "var(--flow-in-7)", "var(--flow-in-8)"];
const SPEND_COLORS = ["var(--flow-out-1)", "var(--flow-out-2)", "var(--flow-out-3)", "var(--flow-out-4)", "var(--flow-out-5)", "var(--flow-out-6)", "var(--flow-out-7)", "var(--flow-out-8)"];

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
  members?: Bucket[];
  italic?: boolean;
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

function paint(map: Map<string, Bucket>, id: string, label: string, kind: string, order: number) {
  const found = map.get(id);
  if (found) return found;
  const bucket: Bucket = { id, label, kind, amount: 0, originals: new Map(), order, children: new Map() };
  map.set(id, bucket);
  return bucket;
}

/**
 * Income sources flow into Total income, then out to Total spending, Investments, and Savings.
 * Total income is actual income only. A shortfall joins on the spending side. It is never
 * drawn as income. When income is zero there is no Total income node: shortfall flows
 * straight into Total spending. It is never a fake income source named Spending.
 * A card charge is an expense. Paying that card is spending, not a second expense category named Spending.
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
    /** Category groups by type. Entry gives each transaction its own node. */
    group?: FlowGroup | "source";
    /** Twelve short month names, January first. Entry labels use these. */
    months?: string[];
  },
): MoneyFlow {
  const display = opts.display.trim().toUpperCase();
  const labels = { ...DEFAULT_LABELS, ...opts.labels };
  const categories = opts.categories || [];
  const threshold = opts.threshold ?? 0.03;
  const months = opts.months && opts.months.length === 12 ? opts.months : MONTHS;
  const missing = new Set<string>();
  const sources = new Map<string, Bucket>();
  const spends = new Map<string, Bucket>();
  const invests = new Map<string, Bucket>();
  const group: FlowGroup = opts.group === "entry" || opts.group === "source" ? "entry" : "category";
  let seq = 0;

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
    if (kind === "ignore" || kind === "savings") continue;
    const converted = convertMinor(tx.amountMinor, tx.currency, display, opts.book, opts.nowMs);
    if (!converted) {
      missing.add(tx.currency.toUpperCase());
      continue;
    }
    const placed = placeCategory(tx, categories, labels);
    if (kind === "income") {
      const name = group === "entry" ? entryLabel(placed.source, tx.date, months) : placed.source;
      const id = group === "entry" ? `in:${fold(name)}:${tx.date}:${seq++}` : `in:${fold(placed.source)}`;
      const bucket = paint(sources, id, name, "income", saved?.order ?? 100);
      remember(bucket, converted.amountMinor, tx.amountMinor, tx.currency, display);
      continue;
    }
    const map = kind === "investment" ? invests : spends;
    const fallback = kind === "debt" ? labels.debt : kind === "investment" ? labels.investments : placed.parent;
    const named = group === "entry"
      ? entryLabel(placed.parent || fallback, tx.date, months)
      : kind === "debt" && !tx.category.trim() && !tx.subcategory?.trim()
        ? labels.debt
        : kind === "investment" && !tx.category.trim()
          ? labels.investments
          : placed.parent || fallback;
    const id = group === "entry" ? `out:${kind}:${fold(named)}:${tx.date}:${seq++}` : `out:${kind}:${fold(named)}`;
    const bucket = paint(map, id, named, kind, saved?.order ?? 100);
    remember(bucket, converted.amountMinor, tx.amountMinor, tx.currency, display);
    if (group !== "entry" && placed.child && kind !== "investment") {
      const child = paintChild(bucket, placed.child);
      remember(child, converted.amountMinor, tx.amountMinor, tx.currency, display);
    }
  }

  if (missing.size > 0 && sources.size === 0 && spends.size === 0 && invests.size === 0) {
    return { state: "needs-rate", missing: [...missing].sort() };
  }
  if (missing.size > 0) return { state: "needs-rate", missing: [...missing].sort() };

  const incomeMinor = sumMap(sources);
  const spendingMinor = sumMap(spends);
  const investmentMinor = sumMap(invests);
  if (incomeMinor === 0 && spendingMinor === 0 && investmentMinor === 0) return { state: "empty" };

  const savingsMinor = incomeMinor - spendingMinor - investmentMinor;
  const shortfall = savingsMinor < 0 ? -savingsMinor : 0;
  const inflow = incomeMinor + shortfall;

  const base = incomeMinor || 1;
  let sourceList = groupRest([...sources.values()], base, threshold, Boolean(opts.expandedOther), "in:rest", labels.other, "income");
  if (shortfall > 0) {
    sourceList.push({ id: "in:shortfall", label: labels.shortfall, kind: "drawn", amount: shortfall, originals: new Map(), order: -1, children: new Map() });
  }
  let spendList = groupRest([...spends.values()], spendingMinor || 1, threshold, Boolean(opts.expandedOther), "out:rest", labels.other, "expense");
  const fanInvest = group === "entry" || invests.size > 1;
  let investList = fanInvest ? groupRest([...invests.values()], investmentMinor || 1, threshold, Boolean(opts.expandedOther), "inv:rest", labels.other, "investment") : [];
  spendList = foldUncategorized(spendList, labels, spendingMinor || 1);

  const colorFor = (bucket: Bucket, family: "in" | "out" | "save" | "invest" | "pool" | "spend" | "short", index: number) => {
    const custom = opts.nodeColors?.[bucket.id] || opts.nodeColors?.[bucket.label];
    if (custom && (/^#[0-9A-Fa-f]{6}$/.test(custom) || custom.startsWith("var("))) return custom;
    if (family === "pool") return "var(--flow-in)";
    if (family === "spend") return "var(--flow-out)";
    if (family === "save") return "var(--flow-save)";
    if (family === "invest") return "var(--flow-invest)";
    if (family === "short") return "var(--flow-short)";
    const ramp = family === "in" ? INCOME_COLORS : SPEND_COLORS;
    return ramp[index % ramp.length];
  };

  const toNode = (bucket: Bucket, role: FlowNode["role"], shareBase: number, color: string): FlowNode => ({
    id: bucket.id,
    label: bucket.label,
    role,
    kind: bucket.kind,
    amountMinor: bucket.amount,
    share: shareBase > 0 ? bucket.amount / shareBase : 0,
    color,
    italic: Boolean(bucket.italic),
    originals: originalsOf(bucket),
    members: (bucket.members || []).map((row) => ({ id: row.id, label: row.label, amountMinor: row.amount, originals: originalsOf(row) })),
    children: childRows(bucket),
  });
  const childRows = (bucket: Bucket) => {
    const kids = [...bucket.children.values()].filter((row) => row.amount > 0);
    const covered = kids.reduce((sum, row) => sum + row.amount, 0);
    if (kids.length === 0 || covered !== bucket.amount) return [];
    return kids.map((row) => ({ id: row.id, label: row.label, amountMinor: row.amount, originals: originalsOf(row) }));
  };

  sourceList = sourceList.filter((row) => row.amount > 0).sort((a, b) => a.amount - b.amount || a.label.localeCompare(b.label));
  spendList = spendList.filter((row) => row.amount > 0).sort((a, b) => b.amount - a.amount || a.label.localeCompare(b.label));
  investList = investList.filter((row) => row.amount > 0).sort((a, b) => b.amount - a.amount || a.label.localeCompare(b.label));

  let inPaint = 0;
  let outPaint = 0;
  const sourceNodes = sourceList.map((row) => toNode(row, "source", inflow || 1, row.kind === "drawn" ? colorFor(row, "short", 0) : colorFor(row, "in", inPaint++)));
  const spendNodes = spendList.map((row) => toNode(row, "sink", spendingMinor || 1, colorFor(row, "out", outPaint++)));
  const investNodes = investList.map((row) => toNode(row, "sink", investmentMinor || 1, colorFor(row, "invest", 0)));
  const poolNode = incomeMinor > 0
    ? toNode(
        { id: "pool", label: labels.totalIncome, kind: "pool", amount: incomeMinor, originals: new Map(), order: 0, children: new Map() },
        "pool",
        incomeMinor,
        "var(--flow-in)",
      )
    : null;
  const hubs: FlowNode[] = [];
  if (spendingMinor > 0) {
    hubs.push(toNode(
      { id: "spend", label: labels.totalSpending, kind: "spending", amount: spendingMinor, originals: new Map(), order: 0, children: new Map() },
      "hub",
      inflow || 1,
      "var(--flow-out)",
    ));
  }
  if (investmentMinor > 0) {
    hubs.push(toNode(
      { id: "invest", label: labels.investments, kind: "investment", amount: investmentMinor, originals: new Map(), order: 1, children: new Map() },
      "hub",
      inflow || 1,
      "var(--flow-invest)",
    ));
  }
  if (savingsMinor > 0) {
    hubs.push(toNode(
      { id: "save", label: labels.savings, kind: "savings", amount: savingsMinor, originals: new Map(), order: 2, children: new Map() },
      "hub",
      inflow || 1,
      "var(--flow-save)",
    ));
  }
  const childNodes: FlowNode[] = spendNodes.flatMap((node) =>
    node.children.map((child, index) => ({
      id: child.id,
      label: child.label,
      role: "child" as const,
      kind: "child",
      amountMinor: child.amountMinor,
      share: spendingMinor > 0 ? child.amountMinor / spendingMinor : 0,
      color: SPEND_COLORS[(outPaint + index) % SPEND_COLORS.length],
      originals: child.originals,
      members: [],
      children: [],
    })),
  );
  const nodes = [...sourceNodes, ...(poolNode ? [poolNode] : []), ...hubs, ...spendNodes, ...investNodes, ...childNodes];
  const links: FlowLink[] = [
    ...sourceNodes.filter((row) => row.kind !== "drawn").map((row) => ({ source: row.id, target: "pool", amountMinor: row.amountMinor })),
    ...fundedLinks(incomeMinor, shortfall, spendingMinor, investmentMinor, savingsMinor),
    ...spendNodes.map((row) => ({ source: "spend", target: row.id, amountMinor: row.amountMinor })),
    ...investNodes.map((row) => ({ source: "invest", target: row.id, amountMinor: row.amountMinor })),
    ...spendNodes.flatMap((node) => node.children.map((child) => ({ source: node.id, target: child.id, amountMinor: child.amountMinor }))),
  ].filter((link) => link.amountMinor > 0 && (link.source !== "pool" || incomeMinor > 0));
  const uncategorizedMinor = spendNodes.filter((row) => fold(row.label) === fold(labels.uncategorized)).reduce((sum, row) => sum + row.amountMinor, 0);
  return {
    state: "ready",
    currency: display,
    incomeMinor,
    spendingMinor,
    investmentMinor,
    savingsMinor,
    noIncome: incomeMinor === 0,
    nodes,
    links,
    uncategorizedMinor,
  };
}

function sumMap(map: Map<string, Bucket>): number {
  return [...map.values()].reduce((sum, row) => sum + row.amount, 0);
}

/** Income covers spending, then investments, then savings. Shortfall covers whatever income did not. */
function fundedLinks(income: number, shortfall: number, spending: number, investment: number, savings: number): FlowLink[] {
  let incomeLeft = Math.max(0, income);
  let shortLeft = Math.max(0, shortfall);
  const links: FlowLink[] = [];
  const fund = (id: string, need: number) => {
    const wanted = Math.max(0, need);
    const fromIncome = Math.min(incomeLeft, wanted);
    incomeLeft -= fromIncome;
    const fromShort = Math.min(shortLeft, wanted - fromIncome);
    shortLeft -= fromShort;
    if (fromIncome > 0) links.push({ source: "pool", target: id, amountMinor: fromIncome });
    if (fromShort > 0) links.push({ source: "in:shortfall", target: id, amountMinor: fromShort });
  };
  fund("spend", spending);
  fund("invest", investment);
  fund("save", savings);
  return links;
}

function groupRest(rows: Bucket[], base: number, threshold: number, expanded: boolean, id: string, label: string, kind: string): Bucket[] {
  const live = rows.filter((row) => row.amount > 0);
  if (expanded) return live;
  const small = live.filter((row) => row.kind !== "drawn" && row.amount / (base || 1) < threshold);
  const large = live.filter((row) => !small.includes(row));
  if (small.length === 0 || large.length === 0) return live;
  const other: Bucket = { id, label, kind, amount: 0, originals: new Map(), order: 9_000, children: new Map(), members: small, italic: true };
  for (const row of small) {
    other.amount += row.amount;
    for (const [code, amount] of row.originals) other.originals.set(code, (other.originals.get(code) || 0) + amount);
  }
  return [...large, other];
}

function foldUncategorized(rows: Bucket[], labels: FlowLabels, base: number): Bucket[] {
  const other = rows.find((row) => row.id === "out:rest");
  const unc = rows.find((row) => fold(row.label) === fold(labels.uncategorized));
  if (!other || !unc || unc.amount / (base || 1) >= 0.08) return rows;
  other.amount += unc.amount;
  for (const [code, amount] of unc.originals) other.originals.set(code, (other.originals.get(code) || 0) + amount);
  other.members = [...(other.members || []), unc];
  return rows.filter((row) => row !== unc);
}

function entryLabel(name: string, date: string, months: string[]): string {
  const day = Number(date.slice(8, 10));
  const month = months[Number(date.slice(5, 7)) - 1] || "";
  const when = Number.isFinite(day) && day > 0 ? `${day} ${month}`.trim() : month;
  return [name.trim(), when].filter(Boolean).join(" ");
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
  "investments",
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

export interface SankeyFrame {
  width: number;
  height: number;
  top: number;
  bottom: number;
  left: number;
  right: number;
  plot: number;
  nodeWidth: number;
  nodeGap: number;
  floorShare: number;
  layoutIterations: number;
  linkOpacity: number;
}

/**
 * Plot box for an ECharts sankey. The chart width is the container width.
 * Node bars stay near 10px on a phone and 14px on a tablet. Height grows with
 * the busiest column so one flow is a ribbon, not a wall.
 */
export function sankeyFrame(chartWidth: number, columnCounts: number[], labels = true): SankeyFrame {
  const width = Math.max(280, Math.round(chartWidth || 0));
  const phone = width < 600;
  const nodeWidth = phone ? 10 : 14;
  const nodeGap = 16;
  const counts = columnCounts.filter((count) => count > 0);
  const columns = Math.max(1, counts.length);
  const busiest = Math.max(1, ...counts, 1);
  const minPlot = nodeWidth * columns + 16 * Math.max(0, columns - 1);
  const room = Math.max(8, Math.floor((width - minPlot) / 2));
  const gutter = labels ? Math.max(72, Math.min(phone ? 136 : 160, room)) : 12;
  const slot = 28;
  const plot = Math.round(Math.min(560, Math.max(phone ? 88 : 112, busiest * (nodeGap + slot))));
  const top = labels ? 40 : 12;
  const bottom = labels ? 20 : 12;
  const height = plot + top + bottom;
  const usable = Math.max(2, plot - Math.max(0, busiest - 1) * nodeGap);
  const floorShare = 2 / usable;
  return {
    width,
    height,
    top,
    bottom,
    left: gutter,
    right: gutter,
    plot,
    nodeWidth,
    nodeGap,
    floorShare,
    layoutIterations: 0,
    linkOpacity: 0.42,
  };
}

/** "CHF 523.70 (100%)". The code is the caller's currency. Two decimals for ordinary fiat. */
export function flowAmountLabel(amountMinor: number, currency: string, share: number): string {
  const code = currency.trim().toUpperCase();
  const exp = minorExponent(code);
  const digits = exp <= 0 ? 0 : Math.min(exp, 2);
  const value = amountMinor / 10 ** (exp < 0 ? 0 : exp);
  const body = value.toFixed(digits);
  const money = code ? `${code} ${body}` : body;
  return `${money} (${flowPercent(share)})`;
}

export function flowDepth(role: FlowNode["role"], present: { pool: boolean; hub: boolean; sink: boolean }): number {
  if (role === "source") return 0;
  if (role === "pool") return 1;
  if (role === "hub") return present.pool ? 2 : 1;
  if (role === "child") return 1 + Number(present.pool) + Number(present.hub) + Number(present.sink);
  return 1 + Number(present.pool) + Number(present.hub);
}

export interface SankeyOptionInput {
  nodes: FlowNode[];
  links: FlowLink[];
  currency: string;
  width: number;
  labels: boolean;
  values: boolean;
  colors: Record<string, string>;
  ink: string;
  muted: string;
  brand: string;
  paper: string;
  line: string;
  mono: boolean;
}

/** ECharts option. Width is the container. Labels keep a line break between the name and the amount. */
export function buildSankeyOption(input: SankeyOptionInput): { option: Record<string, unknown>; frame: SankeyFrame } {
  const present = {
    pool: input.nodes.some((node) => node.role === "pool"),
    hub: input.nodes.some((node) => node.role === "hub"),
    sink: input.nodes.some((node) => node.role === "sink" || node.role === "child"),
  };
  const depthOf = (node: FlowNode) => flowDepth(node.role, present);
  const counts = [0, 1, 2, 3, 4].map((depth) => input.nodes.filter((node) => depthOf(node) === depth).length);
  const frame = sankeyFrame(input.width, counts, input.labels);
  const lastDepth = Math.max(0, ...input.nodes.map(depthOf));
  const total = Math.max(1, ...input.nodes.map((node) => node.amountMinor));
  const safe = new Map(input.nodes.map((node, index) => [node.id, `n${index}`]));
  const sideWidth = Math.max(56, Math.min(frame.left, frame.right) - 18);
  const clip = (value: string, width: number) => {
    const max = Math.max(8, Math.floor(width / 6.4));
    const clean = value.replace(/[{}|]/g, " ");
    return clean.length > max ? `${clean.slice(0, max - 1)}…` : clean;
  };
  const data = input.nodes.map((node) => {
    const depth = depthOf(node);
    const outsideRight = depth === lastDepth && frame.right >= 150;
    const end = depth === 0 ? "left" : outsideRight ? "right" : depth === lastDepth ? "left" : node.role === "hub" ? "bottom" : "top";
    const align = end === "right" ? "left" : end === "left" ? "right" : "center";
    const box = end === "top" || end === "bottom" ? Math.max(72, sideWidth) : sideWidth;
    const name = clip(node.label, box);
    const meta = clip(flowAmountLabel(node.amountMinor, input.currency, node.share), box);
    const text = input.values ? `{name|${name}}\n{meta|${meta}}` : `{name|${name}}`;
    return {
      name: safe.get(node.id),
      depth,
      raw: node.amountMinor,
      value: Math.max(1, sankeyValue(node.amountMinor, total, frame.floorShare)),
      label: {
        show: input.labels,
        position: end,
        align,
        distance: 6,
        width: box,
        overflow: "truncate",
        ellipsis: "…",
        fontSize: 13,
        lineHeight: 16,
        formatter: text,
        rich: {
          name: { color: input.ink, fontWeight: 650, fontSize: 13, fontStyle: node.italic ? "italic" : "normal", lineHeight: 16, width: box, overflow: "truncate", align },
          meta: { color: input.muted, fontSize: 13, lineHeight: 16, width: box, overflow: "truncate", align },
        },
      },
      itemStyle: { color: input.mono ? input.brand : input.colors[node.id] || input.brand, borderWidth: 0 },
    };
  });
  return {
    frame,
    option: {
      backgroundColor: input.paper,
      series: [
        {
          type: "sankey",
          orient: "horizontal",
          nodeAlign: "justify",
          layoutIterations: frame.layoutIterations,
          draggable: false,
          nodeWidth: frame.nodeWidth,
          nodeGap: frame.nodeGap,
          left: frame.left,
          right: frame.right,
          top: frame.top,
          bottom: frame.bottom,
          emphasis: { focus: "adjacency" },
          lineStyle: { color: input.mono ? input.brand : "gradient", curveness: 0.5, opacity: frame.linkOpacity },
          label: { show: input.labels },
          data,
          links: input.links.map((link) => ({
            source: safe.get(link.source),
            target: safe.get(link.target),
            raw: link.amountMinor,
            value: Math.max(1, sankeyValue(link.amountMinor, total, frame.floorShare)),
          })),
        },
      ],
    },
  };
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
    if (node.role === "hub" && inn !== node.amountMinor) return false;
    if (node.role === "hub" && out !== 0 && out !== node.amountMinor) return false;
    if (node.role === "sink" && inn !== node.amountMinor) return false;
    if (node.role === "sink" && out !== 0 && out !== node.amountMinor) return false;
    if (node.role === "child" && (inn !== node.amountMinor || out !== 0)) return false;
  }
  return true;
}

/** One sentence for Talk: income, spending, savings or shortfall, and the largest categories. */
export function describeFlow(flow: MoneyFlow): string {
  if (flow.state === "empty") return "Nothing recorded in this period.";
  if (flow.state === "needs-rate") return `Needs a rate for ${flow.missing.join(", ")}.`;
  const money = (amount: number) => compactLabeled(amount, flow.currency);
  const tops = flow.nodes
    .filter((node) => node.role === "sink" && node.kind !== "investment")
    .sort((a, b) => b.amountMinor - a.amountMinor)
    .slice(0, 3)
    .map((node) => `${node.label} ${money(node.amountMinor)} (${flowPercent(node.share)})`);
  const topText = tops.length ? ` Largest categories: ${tops.join(", ")}.` : "";
  if (flow.noIncome) return `No income this month. Spending ${money(flow.spendingMinor)}.${topText}`;
  const saved = flow.savingsMinor < 0 ? `Shortfall ${money(-flow.savingsMinor)}.` : `Savings ${money(flow.savingsMinor)}.`;
  return `Income ${money(flow.incomeMinor)}. Spending ${money(flow.spendingMinor)}. ${saved} Investments ${money(flow.investmentMinor)}.${topText}`;
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
  let grew = true;
  while (grew) {
    grew = false;
    for (const link of flow.links) {
      if (keep.has(link.source) || keep.has(link.target)) {
        if (!keep.has(link.source)) {
          keep.add(link.source);
          grew = true;
        }
        if (!keep.has(link.target)) {
          keep.add(link.target);
          grew = true;
        }
      }
    }
  }
  return keep;
}
