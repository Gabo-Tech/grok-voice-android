import { convertMinor, type RateBook } from "./financeMath.ts";

export type DebtSliceId = "cards" | "loans" | "people" | "bills";

export interface DebtAccount {
  id?: string;
  name: string;
  kind: string;
  currency: string;
  shownBalanceMinor?: number | null;
}

export interface DebtBill {
  id: string;
  name: string;
  amountMinor: number | null;
  currency: string;
  date: string;
  direction: string;
  accountId?: string;
}

export interface DebtItem {
  id: string;
  label: string;
  amountMinor: number;
  currency: string;
  displayMinor: number | null;
}

export interface DebtSlice {
  id: DebtSliceId;
  amountMinor: number;
  natives: { amountMinor: number; currency: string }[];
  items: DebtItem[];
}

export interface DebtSelection {
  amountMinor: number;
  currency: string;
  natives: { amountMinor: number; currency: string }[];
  slices: DebtSlice[];
  missing: string[];
}

const ORDER: DebtSliceId[] = ["cards", "loans", "people", "bills"];

function sliceOf(kind: string): DebtSliceId | null {
  if (kind === "credit_card" || kind === "card") return "cards";
  if (kind === "loan") return "loans";
  if (kind === "other_debt") return "people";
  return null;
}

/**
 * Everything owed, in the main currency.
 * Cards, loans, and money owed to people use a positive balance.
 * A negative card balance is a credit and is left out.
 * Bills are unpaid planned expenses already due or overdue. Future ones stay out.
 * A bill on a card, loan, or personal debt is left out so the balance is not counted twice.
 * Debts are never added into asset totals. Net worth is assets minus this amount.
 */
export function selectDebt(
  accounts: DebtAccount[],
  bills: DebtBill[],
  display: string,
  book: RateBook | null,
  nowMs: number,
  today: string,
): DebtSelection {
  const target = display.trim().toUpperCase();
  const slices = new Map<DebtSliceId, DebtSlice>();
  for (const id of ORDER) slices.set(id, { id, amountMinor: 0, natives: [], items: [] });
  const missing = new Set<string>();
  const debtIds = new Set(accounts.filter((account) => sliceOf(account.kind) && account.id).map((account) => account.id as string));

  const add = (id: DebtSliceId, item: DebtItem) => {
    const slice = slices.get(id);
    if (!slice) return;
    slice.items.push(item);
    if (item.displayMinor == null) {
      if (item.currency) missing.add(item.currency.toUpperCase());
      return;
    }
    if (item.displayMinor <= 0) return;
    slice.amountMinor += item.displayMinor;
    const code = item.currency.toUpperCase();
    if (!code || code === target) return;
    const found = slice.natives.find((row) => row.currency === code);
    if (found) found.amountMinor += item.amountMinor;
    else slice.natives.push({ amountMinor: item.amountMinor, currency: code });
  };

  for (const account of accounts) {
    const id = sliceOf(account.kind);
    if (!id || account.shownBalanceMinor == null || account.shownBalanceMinor <= 0 || !account.currency) continue;
    const converted = convertMinor(account.shownBalanceMinor, account.currency, target, book, nowMs);
    add(id, {
      id: account.id || account.name,
      label: account.name,
      amountMinor: account.shownBalanceMinor,
      currency: account.currency,
      displayMinor: converted?.amountMinor ?? null,
    });
  }

  for (const bill of bills) {
    if (bill.direction !== "expense") continue;
    if (!bill.date || bill.date > today) continue;
    if (bill.amountMinor == null || bill.amountMinor <= 0 || !bill.currency) continue;
    if (bill.accountId && debtIds.has(bill.accountId)) continue;
    const converted = convertMinor(bill.amountMinor, bill.currency, target, book, nowMs);
    add("bills", {
      id: bill.id,
      label: bill.name,
      amountMinor: bill.amountMinor,
      currency: bill.currency,
      displayMinor: converted?.amountMinor ?? null,
    });
  }

  const list = ORDER.map((id) => slices.get(id)).filter((slice): slice is DebtSlice => Boolean(slice && slice.items.length));
  const natives: DebtSelection["natives"] = [];
  let amountMinor = 0;
  for (const slice of list) {
    amountMinor += slice.amountMinor;
    for (const native of slice.natives) {
      const found = natives.find((row) => row.currency === native.currency);
      if (found) found.amountMinor += native.amountMinor;
      else natives.push({ amountMinor: native.amountMinor, currency: native.currency });
    }
  }
  return { amountMinor, currency: target, natives, slices: list, missing: [...missing] };
}
