import type { FinanceAccount, FinanceSnapshot, FinanceTotal, FinanceTransaction, FinanceUpcoming } from "@/lib/api";

const REGIONS: Array<[string, string]> = [
  ["en-gb", "GBP"],
  ["en-au", "AUD"],
  ["en-ca", "CAD"],
  ["en-nz", "NZD"],
  ["en-ie", "EUR"],
  ["en-in", "INR"],
  ["en-ch", "CHF"],
  ["pt-br", "BRL"],
  ["es-mx", "MXN"],
  ["es-ar", "ARS"],
  ["es-cl", "CLP"],
  ["es-co", "COP"],
  ["es-us", "USD"],
  ["de-ch", "CHF"],
  ["fr-ch", "CHF"],
  ["it-ch", "CHF"],
  ["zh-cn", "CNY"],
  ["zh-tw", "TWD"],
  ["zh-hk", "HKD"],
  ["ja", "JPY"],
  ["ko", "KRW"],
  ["sv", "SEK"],
  ["da", "DKK"],
  ["pl", "PLN"],
  ["de", "EUR"],
  ["es", "EUR"],
  ["fr", "EUR"],
  ["it", "EUR"],
  ["nl", "EUR"],
  ["pt", "EUR"],
  ["ca", "EUR"],
  ["en", "USD"],
];

export function defaultCurrency(locale: string): string {
  const tag = locale.trim().replace(/_/g, "-").toLowerCase();
  for (const [prefix, code] of REGIONS) {
    if (tag === prefix || tag.startsWith(`${prefix}-`)) return code;
  }
  return "USD";
}

export function minorExponent(currency: string): number {
  if (["JPY", "KRW", "VND", "CLP"].includes(currency)) return 0;
  if (["BHD", "KWD", "OMR"].includes(currency)) return 3;
  return 2;
}

export function majorString(minor: number | null | undefined, currency: string): string {
  if (minor == null) return "";
  const exp = minorExponent(currency);
  const value = minor / 10 ** exp;
  return exp === 0 ? String(Math.round(value)) : value.toFixed(exp);
}

export function formatMoney(minor: number | null | undefined, currency: string): string {
  if (minor == null || !currency) return "";
  const exp = minorExponent(currency);
  const value = minor / 10 ** exp;
  try {
    return new Intl.NumberFormat(undefined, { style: "currency", currency }).format(value);
  } catch {
    return `${value.toFixed(exp)} ${currency}`;
  }
}

export function emptyFinance(locale = ""): FinanceSnapshot {
  const today = new Date();
  const period = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}`;
  return {
    accounts: [],
    transactions: [],
    rules: [],
    totals: [],
    upcoming: [],
    period,
    defaultCurrency: defaultCurrency(locale),
  };
}

export function shownBalance(account: FinanceAccount, transactions: FinanceTransaction[]): number | null {
  if (account.balanceMinor == null) return null;
  if (account.kind === "investment") return account.balanceMinor;
  let balance = account.balanceMinor;
  const rows = transactions
    .filter((tx) => tx.accountId === account.id || tx.fromAccountId === account.id)
    .slice()
    .sort((a, b) => a.date.localeCompare(b.date) || a.createdMs - b.createdMs);
  for (const tx of rows) {
    if (account.currency && tx.currency && tx.currency !== account.currency) continue;
    if (account.balanceUpdatedMs > 0 && tx.createdMs <= account.balanceUpdatedMs) continue;
    if (account.balanceAsOf && tx.date < account.balanceAsOf) continue;
    const source = tx.accountId !== account.id;
    balance += source ? -tx.amountMinor : delta(account.kind, tx.direction, tx.amountMinor);
  }
  return balance;
}

function delta(kind: string, direction: string, amount: number): number {
  if (kind === "credit_card" || kind === "loan") {
    if (direction === "expense") return amount;
    if (direction === "payment") return -amount;
    return 0;
  }
  if (kind === "investment") return 0;
  if (direction === "income") return amount;
  if (direction === "expense" || direction === "payment") return -amount;
  return 0;
}

export function withShown(snapshot: FinanceSnapshot): FinanceSnapshot {
  const accounts = snapshot.accounts.map((account) => ({
    ...account,
    shownBalanceMinor: shownBalance(account, snapshot.transactions),
  }));
  return { ...snapshot, accounts, totals: totalsFor(snapshot.transactions, snapshot.period), upcoming: upcomingFor(snapshot) };
}

function totalsFor(transactions: FinanceTransaction[], period: string): FinanceTotal[] {
  const by = new Map<string, FinanceTotal>();
  for (const tx of transactions) {
    if (period && !tx.date.startsWith(period)) continue;
    const row = by.get(tx.currency) ?? { currency: tx.currency, incomeMinor: 0, expenseMinor: 0, paymentMinor: 0 };
    if (tx.direction === "income") row.incomeMinor += tx.amountMinor;
    if (tx.direction === "expense") row.expenseMinor += tx.amountMinor;
    if (tx.direction === "payment") row.paymentMinor += tx.amountMinor;
    by.set(tx.currency, row);
  }
  return [...by.values()];
}

function upcomingFor(snapshot: FinanceSnapshot): FinanceUpcoming[] {
  return snapshot.rules
    .filter((rule) => rule.nextDate)
    .map((rule) => ({
      id: rule.id,
      source: "rule",
      title: rule.name,
      date: rule.nextDate,
      amountMinor: rule.amountMinor,
      currency: rule.currency,
      direction: rule.direction,
      calendarEventId: rule.calendarEventId,
    }))
    .sort((a, b) => a.date.localeCompare(b.date));
}

export function kindLabel(kind: string): string {
  if (kind === "bank") return "Bank";
  if (kind === "cash") return "Cash";
  if (kind === "credit_card") return "Credit card";
  if (kind === "loan") return "Loan";
  if (kind === "investment") return "Investment";
  return "Other";
}
