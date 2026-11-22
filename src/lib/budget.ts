import { convertMinor, type PlannedHit, type RateBook } from "./financeMath.ts";
import type { FlowTx } from "./moneyFlow.ts";

export type BudgetRole = "income" | "expense" | "savings" | "investment";

export interface BudgetLine {
  id: string;
  role: BudgetRole | string;
  category: string;
  subcategory: string;
  label: string;
  amountMinor: number;
  currency: string;
}

export interface BudgetRecord {
  id: string;
  /** YYYY-MM, or empty for the recurring template. */
  period: string;
  currency: string;
  lines: BudgetLine[];
  copyForward: boolean;
  notify: boolean;
  warned: string[];
  updatedMs: number;
}

export interface BudgetActualTx {
  amountMinor: number;
  currency: string;
  direction: string;
  category: string;
  subcategory?: string;
  notes?: string;
  date: string;
  accountKind?: string;
}

export interface BudgetRow {
  id: string;
  role: string;
  category: string;
  subcategory: string;
  label: string;
  name: string;
  plannedMinor: number;
  actualMinor: number;
  /** Planned minus actual. Negative means over the line. */
  remainingMinor: number;
  ratio: number;
  over: boolean;
  warn: boolean;
}

export interface BudgetSummary {
  currency: string;
  month: string;
  inherited: boolean;
  incomePlanned: number;
  incomeActual: number;
  spendingPlanned: number;
  spendingActual: number;
  savingsPlanned: number;
  savingsActual: number;
  investmentsPlanned: number;
  investmentsActual: number;
  /** Income still expected plus what already arrived, minus spending and investments still ahead. */
  projectedNet: number;
  rows: BudgetRow[];
  warnings: Array<{ name: string; ratio: number; over: boolean; amountMinor: number }>;
}

export function emptyBudget(period: string, currency: string): BudgetRecord {
  return { id: "", period, currency, lines: [], copyForward: true, notify: false, warned: [], updatedMs: 0 };
}

/** The month's own budget, otherwise the template, otherwise the latest earlier month that copies forward. */
export function resolveBudget(budgets: BudgetRecord[], month: string): { budget: BudgetRecord | null; inherited: boolean } {
  const own = budgets.find((row) => row.period === month);
  if (own) return { budget: own, inherited: false };
  const template = budgets.find((row) => row.period === "" && (row.copyForward || row.lines.length > 0));
  if (template) return { budget: { ...template, id: "", period: month }, inherited: true };
  const earlier = budgets
    .filter((row) => row.period && row.period < month && row.copyForward)
    .sort((a, b) => b.period.localeCompare(a.period))[0];
  if (earlier) return { budget: { ...earlier, id: "", period: month, warned: [] }, inherited: true };
  return { budget: null, inherited: false };
}

function roleOf(direction: string, categoryKind: string, accountKind: string): BudgetRole | "ignore" {
  if (direction === "income") return "income";
  if (direction === "payment") return "expense";
  if (direction === "transfer") {
    if (accountKind === "investment" || categoryKind === "investment") return "investment";
    if (categoryKind === "savings") return "ignore";
    return "ignore";
  }
  if (direction !== "expense") return "ignore";
  if (categoryKind === "savings") return "ignore";
  if (categoryKind === "investment") return "investment";
  return "expense";
}

function lineName(line: BudgetLine): string {
  return line.label.trim() || line.subcategory.trim() || line.category.trim() || line.role;
}

/**
 * Planned lines against money already recorded, plus the end-of-month net if scheduled items still ahead land.
 * Savings actual is income minus spending minus investments, the same residual the chart uses.
 */
export function summarizeBudget(
  budget: BudgetRecord | null,
  txs: BudgetActualTx[],
  scheduled: PlannedHit[],
  opts: {
    month: string;
    display: string;
    book: RateBook | null;
    nowMs: number;
    today: string;
    inherited?: boolean;
    categoryKinds?: Record<string, string>;
  },
): BudgetSummary {
  const display = opts.display.trim().toUpperCase();
  const lines = budget?.lines || [];
  const start = `${opts.month}-01`;
  const end = `${opts.month}-31`;
  const actuals = new Map<string, number>();
  const loose: Array<{ role: string; category: string; subcategory: string; amount: number }> = [];

  const add = (key: string, amount: number) => actuals.set(key, (actuals.get(key) || 0) + amount);

  for (const tx of txs) {
    if (!tx.date.startsWith(opts.month) || tx.amountMinor <= 0) continue;
    const kind = opts.categoryKinds?.[tx.category.trim().toLowerCase()] || "";
    const role = roleOf(tx.direction, kind, tx.accountKind || "");
    if (role === "ignore") continue;
    const converted = convertMinor(tx.amountMinor, tx.currency, display, opts.book, opts.nowMs);
    if (!converted) continue;
    loose.push({ role, category: tx.category.trim(), subcategory: (tx.subcategory || "").trim(), amount: converted.amountMinor });
  }

  const groups = new Map<string, BudgetLine[]>();
  for (const line of lines) {
    const key = `${normalize(line.role)}:${fold(line.category)}:${fold(line.subcategory)}`;
    const list = groups.get(key) || [];
    list.push(line);
    groups.set(key, list);
  }

  const claimed = new Map<string, number>();
  for (const [key, group] of groups) {
    const [role, category, subcategory] = key.split(":");
    const matching = loose.filter((row) => row.role === role && fold(row.category) === category && (!subcategory || fold(row.subcategory) === subcategory));
    const total = matching.reduce((sum, row) => sum + row.amount, 0);
    if (group.length === 1) {
      claimed.set(group[0].id, total);
      continue;
    }
    let left = total;
    const ordered = [...group].sort((a, b) => a.label.localeCompare(b.label) || a.id.localeCompare(b.id));
    for (const line of ordered) {
      const planned = convertedLine(line, display, opts.book, opts.nowMs);
      const take = Math.min(left, Math.max(planned, 0));
      claimed.set(line.id, line === ordered[ordered.length - 1] ? left : take);
      left -= take;
    }
  }

  for (const row of loose) add(`role:${row.role}`, row.amount);

  const rows: BudgetRow[] = lines.map((line) => {
    const planned = convertedLine(line, display, opts.book, opts.nowMs);
    const actual = claimed.get(line.id) || 0;
    const remaining = planned - actual;
    const ratio = planned > 0 ? actual / planned : actual > 0 ? Infinity : 0;
    const over = line.role === "expense" && remaining < 0;
    const warn = line.role === "expense" && planned > 0 && ratio >= 0.8;
    return {
      id: line.id,
      role: line.role,
      category: line.category,
      subcategory: line.subcategory,
      label: line.label,
      name: lineName(line),
      plannedMinor: planned,
      actualMinor: actual,
      remainingMinor: remaining,
      ratio,
      over,
      warn,
    };
  });

  const plannedOf = (role: string) => rows.filter((row) => row.role === role).reduce((sum, row) => sum + row.plannedMinor, 0);
  const incomeActual = actuals.get("role:income") || 0;
  const spendingActual = actuals.get("role:expense") || 0;
  const investmentsActual = actuals.get("role:investment") || 0;
  const savingsActual = incomeActual - spendingActual - investmentsActual;
  const ahead = scheduled
    .filter((hit) => hit.date >= opts.today && hit.date >= start && hit.date <= end && hit.amountMinor)
    .reduce(
      (sum, hit) => {
        const role = hit.purpose === "investment_payout" ? "income" : hit.direction === "income" ? "income" : hit.direction === "expense" || hit.direction === "payment" ? "expense" : "ignore";
        const converted = convertMinor(hit.amountMinor || 0, hit.currency, display, opts.book, opts.nowMs);
        if (!converted || role === "ignore") return sum;
        if (role === "income") sum.income += converted.amountMinor;
        else sum.spending += converted.amountMinor;
        return sum;
      },
      { income: 0, spending: 0 },
    );
  const projectedNet = incomeActual + ahead.income - (spendingActual + ahead.spending) - investmentsActual;
  const savingRows = rows.filter((row) => row.role === "savings");
  if (savingRows.length === 1) {
    const row = savingRows[0];
    row.actualMinor = Math.max(0, savingsActual);
    row.remainingMinor = row.plannedMinor - row.actualMinor;
    row.ratio = row.plannedMinor > 0 ? row.actualMinor / row.plannedMinor : 0;
    row.over = false;
    row.warn = false;
  }
  const warnings = rows
    .filter((row) => row.warn)
    .map((row) => ({ name: row.name, ratio: row.ratio, over: row.over, amountMinor: row.actualMinor }));
  return {
    currency: display,
    month: opts.month,
    inherited: Boolean(opts.inherited),
    incomePlanned: plannedOf("income"),
    incomeActual,
    spendingPlanned: plannedOf("expense"),
    spendingActual,
    savingsPlanned: plannedOf("savings"),
    savingsActual,
    investmentsPlanned: plannedOf("investment"),
    investmentsActual,
    projectedNet,
    rows,
    warnings,
  };
}

function convertedLine(line: BudgetLine, display: string, book: RateBook | null, nowMs: number): number {
  const converted = convertMinor(line.amountMinor, line.currency || display, display, book, nowMs);
  return converted ? converted.amountMinor : 0;
}

export function lineRemaining(summary: BudgetSummary, name: string): BudgetRow | undefined {
  const folded = fold(name);
  return summary.rows.find((row) => fold(row.name) === folded || fold(row.category) === folded || fold(row.label) === folded);
}

export function overBudget(summary: BudgetSummary): boolean {
  if (summary.spendingPlanned > 0 && summary.spendingActual > summary.spendingPlanned) return true;
  return summary.rows.some((row) => row.over);
}

/** Budget lines as transactions so the Sankey can draw the plan with the same columns. */
export function budgetAsFlow(lines: BudgetLine[], month: string): FlowTx[] {
  return lines
    .filter((line) => line.amountMinor > 0 && line.role !== "savings")
    .map((line) => ({
      amountMinor: line.amountMinor,
      currency: line.currency,
      direction: line.role === "income" ? "income" : line.role === "investment" ? "transfer" : "expense",
      category: line.category || line.label,
      subcategory: line.subcategory,
      notes: line.label,
      date: `${month}-15`,
      accountKind: line.role === "investment" ? "investment" : "",
    }));
}

export function shiftMonth(month: string, by: number): string {
  const [year, mon] = month.split("-").map(Number);
  if (!year || !mon) return month;
  const index = year * 12 + (mon - 1) + by;
  const nextYear = Math.floor(index / 12);
  const nextMonth = (index % 12) + 1;
  return `${nextYear}-${String(nextMonth).padStart(2, "0")}`;
}

function fold(value: string): string {
  return value.trim().toLowerCase();
}

function normalize(role: string): string {
  const folded = fold(role);
  if (folded === "income" || folded === "expense" || folded === "savings" || folded === "investment") return folded;
  return "expense";
}
