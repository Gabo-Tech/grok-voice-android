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

const FIAT = new Set(["AED", "AUD", "BGN", "BRL", "CAD", "CHF", "CNY", "CZK", "DKK", "EUR", "GBP", "HKD", "HUF", "IDR", "ILS", "INR", "ISK", "JPY", "KRW", "MXN", "MYR", "NOK", "NZD", "PHP", "PLN", "RON", "SAR", "SEK", "SGD", "THB", "TRY", "USD", "ZAR", "VND", "CLP", "BHD", "KWD", "OMR"]);
const CRYPTO = new Set(["BTC", "ETH", "XMR", "SOL", "LTC", "BCH", "XRP", "XLM", "DOGE", "ADA", "DOT", "AVAX", "ATOM", "LINK", "UNI", "AAVE", "MATIC", "POL", "NEAR", "APT", "SUI", "TON", "TRX", "ETC", "FIL", "ICP", "HBAR", "ALGO", "EOS", "XTZ", "DASH", "ZEC", "NEO", "USDT", "USDC", "DAI", "SHIB", "PEPE", "ARB", "OP", "INJ", "TIA", "SEI", "RENDER", "FET", "WLD", "BONK", "FLOKI"]);

/** Crypto is a currency. Longer tickers count too. Ordinary words are not coins. */
export function isCrypto(currency: string): boolean {
  const code = currency.trim().toUpperCase();
  if (!code || FIAT.has(code)) return false;
  if (CRYPTO.has(code)) return true;
  return code.length >= 4 && code.length <= 12 && /^[A-Z]+$/.test(code) && !["CREDIT", "BALANCE", "WORTH", "CUSTOM", "PROPERTY", "WALLET", "BITCOIN", "ETHEREUM"].includes(code);
}

export function minorExponent(currency: string): number {
  const code = currency.toUpperCase();
  if (["JPY", "KRW", "VND", "CLP"].includes(code)) return 0;
  if (["BHD", "KWD", "OMR"].includes(code)) return 3;
  if (isCrypto(code)) return 8;
  return 2;
}

export function majorString(minor: number | null | undefined, currency: string): string {
  if (minor == null) return "";
  const exp = minorExponent(currency);
  const value = minor / 10 ** exp;
  if (exp === 0) return String(Math.round(value));
  const text = value.toFixed(exp);
  return exp > 2 ? text.replace(/(\.\d{2,}?)0+$/, "$1") : text;
}

export function formatMoney(minor: number | null | undefined, currency: string): string {
  if (minor == null || !currency) return "";
  const exp = minorExponent(currency);
  const value = minor / 10 ** exp;
  try {
    return new Intl.NumberFormat(undefined, { style: "currency", currency }).format(value);
  } catch {
    const text = value.toFixed(exp);
    const shown = exp > 2 ? text.replace(/(\.\d{2,}?)0+$/, "$1") : text;
    return `${shown} ${currency}`;
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
    categories: [],
  };
}

export function shownBalance(account: FinanceAccount, transactions: FinanceTransaction[]): number | null {
  if (account.balanceMinor == null) return null;
  if (account.kind === "investment" && !isCrypto(account.currency)) {
    const priced = unitsWorth(account);
    return priced ?? account.balanceMinor;
  }
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
    balance += source ? -tx.amountMinor : delta(account.kind, account.currency, tx.direction, tx.amountMinor);
  }
  return balance;
}

function unitsWorth(account: FinanceAccount): number | null {
  const units = Number(String(account.units || "").replace(",", "."));
  if (!account.units || !Number.isFinite(units) || units <= 0 || account.priceMinor == null) return null;
  return Math.round(units * account.priceMinor);
}

function delta(kind: string, currency: string, direction: string, amount: number): number {
  if (kind === "credit_card" || kind === "loan" || kind === "other_debt") {
    if (direction === "expense") return amount;
    if (direction === "payment" || direction === "income") return -amount;
    return 0;
  }
  if (kind === "investment" && !isCrypto(currency)) return 0;
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
      daysUntil: daysUntil(rule.nextDate),
      leadDays: rule.leadDays ?? 2,
    }))
    .sort((a, b) => a.date.localeCompare(b.date));
}

function daysUntil(date: string): number {
  if (!date) return 0;
  const today = new Date();
  const start = Date.UTC(today.getFullYear(), today.getMonth(), today.getDate());
  const parts = date.split("-").map(Number);
  const due = Date.UTC(parts[0] || 0, (parts[1] || 1) - 1, parts[2] || 1);
  return Math.round((due - start) / 86_400_000);
}

export function kindLabel(kind: string): string {
  if (kind === "bank") return "Bank";
  if (kind === "cash") return "Cash";
  if (kind === "credit_card") return "Credit card";
  if (kind === "loan") return "Loan";
  if (kind === "other_debt") return "Other debt";
  if (kind === "investment") return "Investment";
  return "Other";
}

export function typeLabel(kind: string): string {
  if (kind === "stock") return "Stock";
  if (kind === "fund") return "Fund";
  if (kind === "crypto") return "Crypto";
  if (kind === "property") return "Property";
  if (kind === "collectible") return "Collectible";
  if (kind === "stake") return "Private stake";
  if (kind === "custom") return "Custom";
  return kind;
}
