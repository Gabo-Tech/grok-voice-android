import type { FinanceAccount, FinanceTransaction } from "@/lib/api";
import { SERIES_HEX, shownColor } from "./entityColor.ts";

export const PALETTE = [...SERIES_HEX];

export type PeriodId = "this" | "last" | "six" | "year" | "all";

export function displayColor(color: string, index: number): string {
  return shownColor(`slot-${index}`, color);
}

export function isDebt(kind: string): boolean {
  return kind === "credit_card" || kind === "loan" || kind === "other_debt";
}

export function groupOf(kind: string): "money" | "debt" | "investment" {
  if (isDebt(kind)) return "debt";
  if (kind === "investment" || kind === "other") return "investment";
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

const GENERIC_TITLES = new Set(["expense", "income", "payment", "uncategorized", "transfer", "gasto", "ingreso"]);

/** Category, notes, or name. A bare Expense or Income title is not used when something more specific is known. */
export function entryTitle(parts: { category?: string; subcategory?: string; notes?: string; name?: string }): string {
  const category = (parts.category || "").trim();
  const sub = (parts.subcategory || "").trim();
  const named = (value: string) => value && !GENERIC_TITLES.has(value.toLowerCase());
  if (named(sub) && named(category)) return `${category} · ${sub}`;
  if (named(sub)) return sub;
  if (named(category)) return category;
  const notes = (parts.notes || "").trim();
  if (notes) return notes;
  const name = (parts.name || "").trim();
  if (named(name)) return name;
  return "";
}

export interface RuleAmountInput {
  amountMinor: number | null;
  currency: string;
  name: string;
  accountId: string;
  direction: string;
}

export interface RuleAmountAccount {
  id: string;
  name: string;
  kind: string;
  currency: string;
  shownBalanceMinor: number | null;
  paymentMinor: number | null;
}

export interface RuleAmount {
  amountMinor: number | null;
  currency: string;
  /** Saved on the rule, the card statement, the balance due, or nothing to show. */
  source: "saved" | "statement" | "balance" | "none";
}

/** A rule with no amount uses the linked card's statement, then the balance due. */
export function ruleAmount(rule: RuleAmountInput, accounts: RuleAmountAccount[]): RuleAmount {
  const linked = linkedAccount(rule, accounts);
  if (rule.amountMinor != null && rule.amountMinor > 0) {
    return { amountMinor: rule.amountMinor, currency: rule.currency || linked?.currency || "", source: "saved" };
  }
  if (!linked || !isDebt(linked.kind)) return { amountMinor: null, currency: rule.currency || linked?.currency || "", source: "none" };
  if (linked.paymentMinor != null && linked.paymentMinor > 0) {
    return { amountMinor: linked.paymentMinor, currency: linked.currency || rule.currency, source: "statement" };
  }
  if (linked.shownBalanceMinor != null && linked.shownBalanceMinor > 0) {
    return { amountMinor: linked.shownBalanceMinor, currency: linked.currency || rule.currency, source: "balance" };
  }
  return { amountMinor: null, currency: rule.currency || linked.currency, source: "none" };
}

function linkedAccount(rule: RuleAmountInput, accounts: RuleAmountAccount[]): RuleAmountAccount | undefined {
  if (rule.accountId) {
    const byId = accounts.find((account) => account.id === rule.accountId);
    if (byId) return byId;
  }
  const name = rule.name.trim().toLowerCase();
  if (!name) return undefined;
  const matches = accounts.filter((account) => {
    const label = account.name.trim().toLowerCase();
    return label && (name === label || name.startsWith(label) || label.startsWith(name));
  });
  if (rule.direction === "payment") return matches.find((account) => isDebt(account.kind)) || matches[0];
  return matches[0];
}

export function filterRules<T extends { id: string; name: string; category: string; notes: string; currency: string; direction: string; accountId: string; cadence: string; purpose?: string }>(
  rules: T[],
  opts: { accountId: string; category: string; currency: string; direction: string; recurrence: string; group: string; query?: string; accounts: FinanceAccount[] },
): T[] {
  const groupIds = new Set(opts.accounts.filter((account) => opts.group === "all" || groupOf(account.kind) === opts.group).map((account) => account.id));
  const query = (opts.query || "").trim().toLowerCase();
  const currency = opts.currency.trim().toUpperCase();
  return rules.filter((rule) => {
    if (opts.accountId !== "all" && rule.accountId !== opts.accountId) return false;
    if (opts.category !== "all" && rule.category !== opts.category && rule.name !== opts.category) return false;
    if (currency && currency !== "ALL") {
      const own = (rule.currency || "").toUpperCase();
      const account = opts.accounts.find((row) => row.id === rule.accountId);
      const fallback = (account?.currency || "").toUpperCase();
      if (own !== currency && fallback !== currency) return false;
    }
    if (opts.direction !== "all" && rule.direction !== opts.direction) return false;
    if (opts.recurrence === "once" && rule.purpose !== "one_off" && Boolean(rule.cadence)) return false;
    if (opts.recurrence === "recurring" && (rule.purpose === "one_off" || !rule.cadence)) return false;
    if (opts.recurrence === "irregular") return false;
    if (opts.group !== "all" && rule.accountId && !groupIds.has(rule.accountId)) return false;
    if (query && !`${rule.name} ${rule.category} ${rule.notes} ${rule.direction}`.toLowerCase().includes(query)) return false;
    return true;
  });
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
