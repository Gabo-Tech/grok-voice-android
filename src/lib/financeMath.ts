export interface FlowInput {
  currency: string;
  direction: string;
  amountMinor: number;
  date: string;
}

export interface FlowRow {
  currency: string;
  incoming: number;
  outgoing: number;
  left: number;
}

/** Incoming, outgoing, and leftover for one period. Currencies are never combined. */
export function cashFlow(rows: FlowInput[], start: string, end: string): FlowRow[] {
  const by = new Map<string, FlowRow>();
  for (const tx of rows) {
    if (!tx.currency) continue;
    if (start && tx.date < start) continue;
    if (end && tx.date > end) continue;
    const row = by.get(tx.currency) ?? { currency: tx.currency, incoming: 0, outgoing: 0, left: 0 };
    if (tx.direction === "income") row.incoming += tx.amountMinor;
    else if (tx.direction === "expense" || tx.direction === "payment") row.outgoing += tx.amountMinor;
    row.left = row.incoming - row.outgoing;
    by.set(tx.currency, row);
  }
  return [...by.values()].sort((a, b) => a.currency.localeCompare(b.currency));
}

export interface PayoffAccount {
  id: string;
  kind: string;
  currency: string;
  shownBalanceMinor: number | null;
  balanceUpdatedMs: number;
  balanceAsOf: string;
}

export interface PayoffTx {
  accountId: string;
  fromAccountId: string;
  currency: string;
  direction: string;
  amountMinor: number;
  createdMs: number;
  date: string;
}

export interface Payoff {
  remaining: number;
  paid: number;
  ratio: number;
}

const DEBT = new Set(["credit_card", "loan", "other_debt"]);

/** Paid-down share of a debt. Payments after the stated balance count. No invented principal. */
export function debtPayoff(account: PayoffAccount, transactions: PayoffTx[]): Payoff | null {
  if (!DEBT.has(account.kind) || account.shownBalanceMinor == null) return null;
  const remaining = Math.max(0, account.shownBalanceMinor);
  let paid = 0;
  for (const tx of transactions) {
    const hits = tx.accountId === account.id || tx.fromAccountId === account.id;
    if (!hits || tx.direction !== "payment") continue;
    if (account.currency && tx.currency && tx.currency !== account.currency) continue;
    if (account.balanceUpdatedMs > 0 && tx.createdMs <= account.balanceUpdatedMs) continue;
    if (account.balanceAsOf && tx.date < account.balanceAsOf) continue;
    paid += tx.amountMinor;
  }
  const started = remaining + paid;
  const ratio = started <= 0 ? 0 : Math.min(1, paid / started);
  return { remaining, paid, ratio };
}

export interface DueRule {
  id: string;
  accountId: string;
  nextDate: string;
  amountMinor: number | null;
  currency: string;
}

export interface NextDue {
  id: string;
  date: string;
  amount: number | null;
  currency: string;
}

export function nextDue(
  account: { id: string; currency: string; dueDate: string; paymentMinor: number | null },
  rules: DueRule[],
): NextDue | null {
  const linked = rules
    .filter((rule) => rule.accountId === account.id && rule.nextDate)
    .sort((a, b) => a.nextDate.localeCompare(b.nextDate))[0];
  if (linked) {
    return { id: linked.id, date: linked.nextDate, amount: linked.amountMinor, currency: linked.currency || account.currency };
  }
  if (account.dueDate || account.paymentMinor != null) {
    return { id: "", date: account.dueDate, amount: account.paymentMinor, currency: account.currency };
  }
  return null;
}
