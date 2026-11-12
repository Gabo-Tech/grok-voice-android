import type { FinanceAccount, FinanceTransaction } from "@/lib/api";

export const PALETTE = ["#4C78D8", "#6E8B74", "#A56B66", "#C4B5A0", "#7A6A58", "#3E6B73", "#8C6A3F", "#B8A48A", "#5C6B58", "#9A7B6A"];

export type PeriodId = "this" | "last" | "six" | "year" | "all";

export function displayColor(color: string, index: number): string {
  return /^#[0-9A-Fa-f]{6}$/.test(color || "") ? color : PALETTE[index % PALETTE.length];
}

export function isDebt(kind: string): boolean {
  return kind === "credit_card" || kind === "loan" || kind === "other_debt";
}

export function groupOf(kind: string): "money" | "debt" | "investment" {
  if (isDebt(kind)) return "debt";
  if (kind === "investment") return "investment";
  return "money";
}

export interface NetRow {
  currency: string;
  money: number;
  debt: number;
  investment: number;
  net: number;
}

export function netByCurrency(accounts: FinanceAccount[]): NetRow[] {
  const by = new Map<string, NetRow>();
  for (const account of accounts) {
    if (account.shownBalanceMinor == null || !account.currency) continue;
    const row = by.get(account.currency) ?? { currency: account.currency, money: 0, debt: 0, investment: 0, net: 0 };
    const amount = account.shownBalanceMinor;
    if (isDebt(account.kind)) row.debt += amount;
    else if (account.kind === "investment") row.investment += amount;
    else row.money += amount;
    row.net = row.money + row.investment - row.debt;
    by.set(account.currency, row);
  }
  return [...by.values()].sort((a, b) => a.currency.localeCompare(b.currency));
}

export function periodBounds(period: PeriodId, today = new Date(), focusMonth = ""): { start: string; end: string } {
  if (focusMonth.length === 7) {
    const [year, month] = focusMonth.split("-").map(Number);
    const start = new Date(year, (month || 1) - 1, 1);
    const end = new Date(year, month || 1, 0);
    return { start: iso(start), end: iso(end) };
  }
  if (period === "all") return { start: "", end: "" };
  if (period === "year") return { start: `${today.getFullYear()}-01-01`, end: `${today.getFullYear()}-12-31` };
  if (period === "last") {
    const start = new Date(today.getFullYear(), today.getMonth() - 1, 1);
    const end = new Date(today.getFullYear(), today.getMonth(), 0);
    return { start: iso(start), end: iso(end) };
  }
  if (period === "six") {
    const start = new Date(today.getFullYear(), today.getMonth() - 5, 1);
    const end = new Date(today.getFullYear(), today.getMonth() + 1, 0);
    return { start: iso(start), end: iso(end) };
  }
  const start = new Date(today.getFullYear(), today.getMonth(), 1);
  const end = new Date(today.getFullYear(), today.getMonth() + 1, 0);
  return { start: iso(start), end: iso(end) };
}

export function inPeriod(date: string, start: string, end: string): boolean {
  if (start && date < start) return false;
  if (end && date > end) return false;
  return true;
}

export function filterTransactions(
  rows: FinanceTransaction[],
  opts: { start: string; end: string; accountId: string; category: string; currency: string; direction: string; recurrence: string; group: string; accounts: FinanceAccount[] },
): FinanceTransaction[] {
  const groupIds = new Set(opts.accounts.filter((account) => opts.group === "all" || groupOf(account.kind) === opts.group).map((account) => account.id));
  return rows.filter((tx) => {
    if (!inPeriod(tx.date, opts.start, opts.end)) return false;
    if (opts.accountId !== "all" && tx.accountId !== opts.accountId && tx.fromAccountId !== opts.accountId) return false;
    if (opts.category !== "all" && tx.category !== opts.category) return false;
    if (opts.currency !== "all" && tx.currency !== opts.currency) return false;
    if (opts.direction !== "all" && tx.direction !== opts.direction) return false;
    if (opts.recurrence !== "all" && tx.recurrence !== opts.recurrence) return false;
    if (opts.group !== "all" && tx.accountId && !groupIds.has(tx.accountId)) return false;
    return true;
  });
}

export function monthKey(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}`;
}

export function sixMonths(today = new Date()): string[] {
  const keys: string[] = [];
  for (let offset = 5; offset >= 0; offset -= 1) {
    keys.push(monthKey(new Date(today.getFullYear(), today.getMonth() - offset, 1)));
  }
  return keys;
}

function iso(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}
