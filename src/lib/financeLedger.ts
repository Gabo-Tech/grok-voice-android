import { convertMinor, headlineOf, netWorth, worthAccount, type MoneyPart, type RateBook } from "./financeMath.ts";
import { isDebt } from "./financeView.ts";

function looksCrypto(code: string): boolean {
  const text = code.trim().toUpperCase();
  if (!text || text.length === 3) return ["BTC", "ETH", "XMR", "SOL", "LTC", "BCH", "XRP"].includes(text);
  return text.length >= 4 && text.length <= 12 && /^[A-Z]+$/.test(text);
}

export interface FlowHit {
  date: string;
  direction: string;
  amountMinor: number | null;
  currency: string;
}

export interface FlowTx {
  currency: string;
  direction: string;
  amountMinor: number;
  date: string;
  recurrence?: string;
  ruleId?: string;
  occurrenceDate?: string;
}

/** Income and expenses change the month. A debt payment or a transfer between your own accounts does not. */
export function countsAsFlow(direction: string): boolean {
  return direction === "income" || direction === "expense";
}

/**
 * Posted rows plus schedule occurrences that have already come due and are not posted.
 * `hits` must already omit settled and skipped dates (plannedHits does).
 */
export function monthActivity(rows: FlowTx[], hits: FlowHit[], start: string, end: string, today: string): FlowTx[] {
  const extra: FlowTx[] = [];
  for (const hit of hits) {
    if (hit.amountMinor == null || !hit.currency) continue;
    if (!countsAsFlow(hit.direction)) continue;
    if (start && hit.date < start) continue;
    if (end && hit.date > end) continue;
    if (today && hit.date > today) continue;
    extra.push({ currency: hit.currency, direction: hit.direction, amountMinor: hit.amountMinor, date: hit.date, recurrence: "recurring" });
  }
  return [...rows, ...extra];
}

export function flowParts(rows: FlowTx[], start: string, end: string, direction: "income" | "expense"): MoneyPart[] {
  return rows.flatMap((row) => {
    if (row.direction !== direction || !row.currency) return [];
    if (start && row.date < start) return [];
    if (end && row.date > end) return [];
    return [{ amountMinor: row.amountMinor, currency: row.currency }];
  });
}

export interface ScaleInput {
  id: string;
  name: string;
  kind: string;
  lastFour?: string;
  /** Minor units of the main currency. Null when no rate or no price exists. */
  displayMinor: number | null;
  debt: boolean;
  unpriced: boolean;
}

export interface ScaleBar extends ScaleInput {
  label: string;
  included: boolean;
  /** 0 when this row must not affect or fill the scale. */
  width: number;
}

/** Widths share one maximum per side. Unpriced rows stay at 0 so they cannot fill the bar. */
export function scaledBars(rows: ScaleInput[]): { assets: ScaleBar[]; debts: ScaleBar[] } {
  const counts = new Map<string, number>();
  for (const row of rows) counts.set(row.name, (counts.get(row.name) || 0) + 1);
  const paint = (list: ScaleInput[]) => {
    const max = list.reduce((peak, row) => {
      if (row.unpriced || row.displayMinor == null) return peak;
      return Math.max(peak, Math.abs(row.displayMinor));
    }, 0);
    return list.map((row) => {
      const included = !row.unpriced && row.displayMinor != null;
      const extra = (counts.get(row.name) || 0) > 1;
      const kind = kindWord(row.kind);
      const tail = [extra ? kind : "", extra && row.lastFour ? row.lastFour : ""].filter(Boolean).join(" ");
      return {
        ...row,
        label: tail ? `${row.name} · ${tail}` : row.name,
        included,
        width: included && max > 0 ? Math.abs(row.displayMinor || 0) / max : 0,
      };
    });
  };
  return {
    assets: paint(rows.filter((row) => !row.debt)),
    debts: paint(rows.filter((row) => row.debt)),
  };
}

function kindWord(kind: string): string {
  if (kind === "credit_card") return "Card";
  if (kind === "bank") return "Bank";
  if (kind === "cash") return "Cash";
  if (kind === "loan") return "Loan";
  if (kind === "other_debt") return "Debt";
  if (kind === "investment") return "Holding";
  return "Account";
}

export interface Quote {
  symbol: string;
  price: number;
  currency: string;
  source: string;
  fetchedMs: number;
  estimated?: boolean;
}

export interface HoldingShape {
  name: string;
  kind: string;
  currency: string;
  ticker?: string;
  investmentType?: string;
  units?: string;
  measureUnit?: string;
  shownBalanceMinor?: number | null;
  marketWorthMinor?: number | null;
  marketWorthCurrency?: string;
  marketPrice?: number | null;
  marketPriceCurrency?: string;
  marketSource?: string;
  marketFetchedMs?: number;
  marketStale?: boolean;
  marketEstimated?: boolean;
  priceOverride?: boolean;
  notes?: string;
  purchaseMinor?: number | null;
  purchaseCurrency?: string;
  valuations?: { asOf: string; amountMinor: number; currency: string }[];
}

export interface Position {
  quantity: string;
  unit: string;
  symbol: string;
  amountMinor: number | null;
  currency: string;
  nativeMinor: number | null;
  nativeCurrency: string;
  source: string;
  fetchedMs: number;
  stale: boolean;
  estimated: boolean;
  unpriced: boolean;
  reason: string;
  priceText: string;
}

const ALIAS: Record<string, { symbol: string; kind: "metal" | "crypto" | "stock" }> = {
  silver: { symbol: "XAG", kind: "metal" },
  gold: { symbol: "XAU", kind: "metal" },
  platinum: { symbol: "XPT", kind: "metal" },
  palladium: { symbol: "XPD", kind: "metal" },
  xag: { symbol: "XAG", kind: "metal" },
  xau: { symbol: "XAU", kind: "metal" },
  xpt: { symbol: "XPT", kind: "metal" },
  xpd: { symbol: "XPD", kind: "metal" },
  zano: { symbol: "ZANO", kind: "crypto" },
  xmr: { symbol: "XMR", kind: "crypto" },
  monero: { symbol: "XMR", kind: "crypto" },
  epic: { symbol: "EPIC", kind: "crypto" },
  "epic cash": { symbol: "EPIC", kind: "crypto" },
  epiccash: { symbol: "EPIC", kind: "crypto" },
};

const TROY_OZ_GRAMS = 31.1034768;

/** Full decimal text. Trailing zeros go. The value is never rounded. */
export function formatQuantity(raw: string): string {
  const text = raw.trim().replace(/\s/g, "").replace(",", ".");
  if (!text) return "";
  if (!/^[+-]?\d+(\.\d+)?$/.test(text)) return raw.trim();
  const negative = text.startsWith("-");
  const body = text.replace(/^[+-]/, "");
  const [whole, frac = ""] = body.split(".");
  const trimmed = frac.replace(/0+$/, "");
  const shown = trimmed ? `${whole}.${trimmed}` : whole;
  return negative ? `-${shown}` : shown;
}

export function quantityOf(account: HoldingShape): string {
  const units = (account.units || "").trim();
  if (units && !/[a-z]/i.test(units)) return formatQuantity(units);
  const parsed = parseQty(units) || parseQty(account.notes || "") || parseQty(`${account.notes || ""} ${account.name}`);
  if (parsed) return formatQuantity(parsed);
  return "";
}

function parseQty(text: string): string {
  const match = text.trim().match(/(\d+(?:[.,]\d+)?)/);
  return match ? match[1].replace(",", ".") : "";
}

function aliasKey(value: string): string {
  return value.trim().toLowerCase().replace(/[_/-]+/g, " ").replace(/\s+/g, " ");
}

function aliasOf(account: HoldingShape): { symbol: string; kind: string } | null {
  const keys = [account.name, account.ticker, account.currency, account.notes].map((value) => aliasKey(value || ""));
  for (const key of keys) {
    if (!key) continue;
    if (ALIAS[key]) return ALIAS[key];
    const compact = key.replace(/ /g, "");
    if (ALIAS[compact]) return ALIAS[compact];
  }
  if (looksCrypto(account.currency || "")) return { symbol: (account.currency || "").toUpperCase(), kind: "crypto" };
  return null;
}

function unitPrice(quote: Quote, measure: string, kind: string): number {
  if (kind !== "metal") return quote.price;
  if (measure === "gram") return quote.price / TROY_OZ_GRAMS;
  if (measure === "kilogram") return (quote.price * 1000) / TROY_OZ_GRAMS;
  return quote.price;
}

function measureOf(account: HoldingShape, kind: string): string {
  const raw = (account.measureUnit || "").toLowerCase();
  if (raw === "ounces" || raw === "ounce" || raw === "oz" || raw === "troy_oz") return "troy_oz";
  if (raw === "gram" || raw === "grams" || raw === "g") return "gram";
  if (raw === "kilogram" || raw === "kg") return "kilogram";
  if (raw === "share" || raw === "shares") return "share";
  if (kind === "metal") return "troy_oz";
  if (kind === "stock") return "share";
  if (kind === "crypto") return "coin";
  return raw;
}

/** One holding in the main currency. A missing price stays visible as unpriced. A stale quote is still a value. */
export function positionOf(account: HoldingShape, display: string, book: RateBook | null, quotes: Quote[], nowMs: number): Position {
  const target = display.trim().toUpperCase();
  const alias = aliasOf(account);
  const quantity = quantityOf(account);
  const kind = alias?.kind || (account.investmentType === "stock" || account.investmentType === "fund" ? "stock" : account.investmentType || "");
  const unit = measureOf(account, kind);
  const symbol = (alias?.symbol || account.ticker || "").toUpperCase();
  const quote = quotes.find((row) => row.symbol.toUpperCase() === symbol && row.price > 0);
  const manual = Boolean(account.priceOverride) || account.investmentType === "collectible" || account.investmentType === "property" || account.investmentType === "custom";

  if (account.marketWorthMinor != null && account.marketWorthCurrency) {
    const converted = convertMinor(account.marketWorthMinor, account.marketWorthCurrency, target, book, nowMs);
    return {
      quantity,
      unit,
      symbol,
      amountMinor: converted ? converted.amountMinor : null,
      currency: target,
      nativeMinor: account.marketWorthMinor,
      nativeCurrency: account.marketWorthCurrency,
      source: account.marketSource || (account.marketEstimated ? "estimate" : "cached"),
      fetchedMs: account.marketFetchedMs || 0,
      stale: Boolean(account.marketStale || converted?.stale),
      estimated: Boolean(account.marketEstimated),
      unpriced: !converted,
      reason: converted ? "" : symbol || account.name,
      priceText: account.marketPrice != null ? `${account.marketPrice} ${account.marketPriceCurrency || ""}`.trim() : "",
    };
  }

  if (quote && quantity) {
    const qty = Number(quantity);
    const each = unitPrice(quote, unit, kind);
    const major = qty * each;
    if (Number.isFinite(major) && major > 0) {
      const exp = quote.currency.toUpperCase() === "JPY" ? 0 : 2;
      const minor = Math.round(major * 10 ** exp);
      const converted = convertMinor(minor, quote.currency, target, book, nowMs);
      const stale = quote.fetchedMs > 0 && nowMs - quote.fetchedMs > 24 * 60 * 60 * 1000;
      return {
        quantity,
        unit,
        symbol,
        amountMinor: converted ? converted.amountMinor : null,
        currency: target,
        nativeMinor: minor,
        nativeCurrency: quote.currency.toUpperCase(),
        source: quote.source,
        fetchedMs: quote.fetchedMs,
        stale: stale || Boolean(converted?.stale),
        estimated: Boolean(quote.estimated),
        unpriced: !converted,
        reason: converted ? "" : quote.currency.toUpperCase(),
        priceText: `${each} ${quote.currency}`,
      };
    }
  }

  const stated = account.shownBalanceMinor ?? latestValuation(account);
  const statedCurrency = account.shownBalanceMinor != null ? account.currency : account.valuations?.[account.valuations.length - 1]?.currency || account.currency;
  if (stated != null && statedCurrency) {
    const converted = convertMinor(stated, statedCurrency, target, book, nowMs);
    const estimate = manual || !alias;
    return {
      quantity,
      unit,
      symbol,
      amountMinor: converted ? converted.amountMinor : null,
      currency: target,
      nativeMinor: stated,
      nativeCurrency: statedCurrency.toUpperCase(),
      source: estimate ? "estimate" : "stated",
      fetchedMs: 0,
      stale: Boolean(converted?.stale),
      estimated: estimate,
      unpriced: !converted,
      reason: converted ? "" : statedCurrency.toUpperCase(),
      priceText: "",
    };
  }

  return {
    quantity,
    unit,
    symbol,
    amountMinor: null,
    currency: target,
    nativeMinor: null,
    nativeCurrency: "",
    source: "",
    fetchedMs: 0,
    stale: false,
    estimated: false,
    unpriced: true,
    reason: symbol || account.name,
    priceText: "",
  };
}

function latestValuation(account: HoldingShape): number | null {
  const rows = account.valuations || [];
  if (rows.length === 0) return null;
  return rows[rows.length - 1].amountMinor;
}

export interface Gain {
  amountMinor: number | null;
  currency: string;
  unpriced: boolean;
}

/** Current value minus purchase, both in the main currency. Missing either side is unpriced, never zero. */
export function gainOf(account: HoldingShape, display: string, book: RateBook | null, quotes: Quote[], nowMs: number): Gain {
  const target = display.trim().toUpperCase();
  if (account.purchaseMinor == null || !account.purchaseCurrency) return { amountMinor: null, currency: target, unpriced: true };
  const now = positionOf(account, target, book, quotes, nowMs);
  const bought = convertMinor(account.purchaseMinor, account.purchaseCurrency, target, book, nowMs);
  if (now.unpriced || now.amountMinor == null || !bought) return { amountMinor: null, currency: target, unpriced: true };
  return { amountMinor: now.amountMinor - bought.amountMinor, currency: target, unpriced: false };
}

export function barsFor(
  accounts: Array<HoldingShape & { id: string; lastFour?: string }>,
  display: string,
  book: RateBook | null,
  quotes: Quote[],
  nowMs: number,
): { assets: ScaleBar[]; debts: ScaleBar[] } {
  const rows: ScaleInput[] = accounts.map((account) => {
    const debt = isDebt(account.kind);
    if (debt) {
      const owed = account.shownBalanceMinor;
      const converted = owed == null ? null : convertMinor(owed, account.currency, display, book, nowMs);
      return {
        id: account.id,
        name: account.name,
        kind: account.kind,
        lastFour: account.lastFour,
        displayMinor: converted ? converted.amountMinor : null,
        debt: true,
        unpriced: owed == null || !converted,
      };
    }
    const position = positionOf(account, display, book, quotes, nowMs);
    return {
      id: account.id,
      name: account.name,
      kind: account.kind,
      lastFour: account.lastFour,
      displayMinor: position.amountMinor,
      debt: false,
      unpriced: position.unpriced,
    };
  });
  return scaledBars(rows);
}

/** Net worth in one currency: cash and banks, plus holdings at market or stated value, minus debts. */
export function netWorthParts(
  accounts: Array<HoldingShape & { id?: string }>,
  display: string,
  book: RateBook | null,
  quotes: Quote[],
  nowMs: number,
): { parts: MoneyPart[]; unpriced: string[] } {
  const parts: MoneyPart[] = [];
  const unpriced: string[] = [];
  for (const account of accounts) {
    const holding = account.kind === "investment" || account.kind === "other" || Boolean(quantityOf(account));
    if (!isDebt(account.kind) && holding && account.kind !== "cash" && account.kind !== "bank") {
      const position = positionOf(account, display, book, quotes, nowMs);
      if (position.unpriced || position.amountMinor == null) unpriced.push(position.reason || account.name);
      else parts.push({ amountMinor: position.amountMinor, currency: display });
      continue;
    }
    if (account.shownBalanceMinor == null || !account.currency) {
      if (isDebt(account.kind)) unpriced.push(account.name);
      continue;
    }
    const converted = convertMinor(account.shownBalanceMinor, account.currency, display, book, nowMs);
    if (!converted) {
      unpriced.push(account.currency.toUpperCase());
      continue;
    }
    // A positive card or loan balance is owed, so it lowers net worth. A negative balance is a credit and raises it.
    const signed = isDebt(account.kind) ? -converted.amountMinor : converted.amountMinor;
    parts.push({ amountMinor: signed, currency: display });
  }
  return { parts, unpriced: [...new Set(unpriced)].sort() };
}

export function headlineFor(parts: MoneyPart[], display: string, book: RateBook | null, nowMs: number) {
  return headlineOf(parts, display, book, nowMs);
}

/** Colorblind-friendly default (Okabe–Ito). Other presets stay distinct on light and dark themes. */
export const ALLOCATION_PRESETS: Record<string, string[]> = {
  okabe: ["#0072B2", "#E69F00", "#009E73", "#CC79A7", "#56B4E9", "#D55E00", "#F0E442", "#000000"],
  dusk: ["#8EC8FF", "#F0C36A", "#7DCEA0", "#E7A0C4", "#F2E38A", "#E07A5F", "#C9B6A1", "#F4F1EA"],
  ink: ["#1F4E79", "#B86E00", "#1B7F5A", "#8C3A62", "#3D6F8C", "#9A3412", "#6B5B3A", "#3F3F46"],
};

export function persistedColor(saved: string | undefined, fallback: string): string {
  return /^#[0-9A-Fa-f]{6}$/.test(saved || "") ? (saved || "").toUpperCase() : fallback.toUpperCase();
}

export type AllocGroup = "item" | "class";

export interface AllocInput {
  id: string;
  name: string;
  kind: string;
  assetClass: string;
  color?: string;
  amountMinor: number | null;
  unpriced: boolean;
  quantity: string;
  source: string;
  fetchedMs: number;
  stale: boolean;
  estimated: boolean;
  cash: boolean;
}

export interface AllocSlice {
  id: string;
  label: string;
  amountMinor: number;
  share: number;
  color: string;
  quantity: string;
  source: string;
  fetchedMs: number;
  stale: boolean;
  estimated: boolean;
  members: AllocInput[];
  other: boolean;
}

/** Shares of priced holdings in one currency. Unpriced rows stay out of the total. */
export function allocation(
  rows: AllocInput[],
  opts: { group: AllocGroup; includeCash: boolean; otherBelow?: number; palette?: string[]; expanded?: boolean },
): { slices: AllocSlice[]; unpriced: AllocInput[]; total: number } {
  const palette = opts.palette && opts.palette.length > 0 ? opts.palette : ALLOCATION_PRESETS.okabe;
  const floor = opts.otherBelow ?? 0.025;
  const usable = rows.filter((row) => (opts.includeCash || !row.cash) && !row.unpriced && row.amountMinor != null && row.amountMinor > 0);
  const unpriced = rows.filter((row) => (opts.includeCash || !row.cash) && (row.unpriced || row.amountMinor == null));
  const total = usable.reduce((sum, row) => sum + (row.amountMinor || 0), 0);
  const buckets = new Map<string, AllocInput[]>();
  for (const row of usable) {
    const key = opts.group === "class" ? row.assetClass || "Other" : row.id;
    const list = buckets.get(key) || [];
    list.push(row);
    buckets.set(key, list);
  }
  let slices: AllocSlice[] = [...buckets.entries()].map(([key, members], index) => {
    const amount = members.reduce((sum, row) => sum + (row.amountMinor || 0), 0);
    const lead = members[0];
    return {
      id: key,
      label: opts.group === "class" ? lead.assetClass : lead.name,
      amountMinor: amount,
      share: total > 0 ? amount / total : 0,
      color: persistedColor(members.length === 1 ? lead.color : "", palette[index % palette.length]),
      quantity: members.length === 1 ? lead.quantity : "",
      source: members.length === 1 ? lead.source : "",
      fetchedMs: members.length === 1 ? lead.fetchedMs : 0,
      stale: members.some((row) => row.stale),
      estimated: members.some((row) => row.estimated),
      members,
      other: false,
    };
  });
  slices.sort((a, b) => b.amountMinor - a.amountMinor);
  if (!opts.expanded) {
    const small = slices.filter((slice) => slice.share < floor && slice.share > 0);
    const large = slices.filter((slice) => slice.share >= floor);
    if (small.length > 1) {
      const members = small.flatMap((slice) => slice.members);
      const amount = small.reduce((sum, slice) => sum + slice.amountMinor, 0);
      large.push({
        id: "other",
        label: "Other",
        amountMinor: amount,
        share: total > 0 ? amount / total : 0,
        color: palette[large.length % palette.length],
        quantity: "",
        source: "",
        fetchedMs: 0,
        stale: small.some((slice) => slice.stale),
        estimated: small.some((slice) => slice.estimated),
        members,
        other: true,
      });
      slices = large;
    }
  }
  return { slices, unpriced, total };
}

export function shareOf(slices: AllocSlice[], name: string): number | null {
  const needle = name.trim().toLowerCase();
  if (!needle) return null;
  for (const slice of slices) {
    if (slice.label.toLowerCase() === needle) return slice.share;
    const member = slice.members.find((row) => row.name.toLowerCase() === needle || row.id.toLowerCase() === needle);
    if (member) {
      const whole = slices.reduce((sum, row) => sum + row.amountMinor, 0);
      return whole > 0 ? (member.amountMinor || 0) / whole : 0;
    }
  }
  return null;
}

export function assetClassOf(kind: string, investmentType: string, symbolKind: string): string {
  if (kind === "cash" || kind === "bank") return "Cash";
  if (symbolKind === "crypto" || investmentType === "crypto") return "Crypto";
  if (symbolKind === "metal" || investmentType === "metal") return "Metals";
  if (investmentType === "stock" || investmentType === "fund" || symbolKind === "stock") return "Stocks";
  if (investmentType === "collectible" || investmentType === "property" || investmentType === "stake") return "Manual";
  return "Other";
}

/** Banks, cash, cards, and loans stay on Accounts. Everything else is a holding. */
export function isHoldingAccount(account: { kind: string }): boolean {
  return account.kind !== "bank" && account.kind !== "cash" && account.kind !== "credit_card" && account.kind !== "loan" && account.kind !== "other_debt";
}

export interface CashTx {
  currency: string;
  direction: string;
  amountMinor: number;
  date: string;
  accountKind?: string;
  fromKind?: string;
}

export interface CashLine {
  currency: string;
  income: number;
  /** Every purchase, including a charge on a card. A card payment is not in this figure. */
  expenses: number;
  /** Purchases paid from cash or a bank. Card charges are left out so the later payment is not a second outflow. */
  cashExpenses: number;
  cardPayments: number;
  debtPayments: number;
  /** Income minus expenses. Payments do not change it. */
  result: number;
  /** Income minus cash expenses, card payments, and other debt payments. */
  cashFlow: number;
}

/**
 * A card purchase is an expense when it is charged. It counts once.
 * Paying that card from a bank or wallet is cash leaving. It lowers the cash
 * balance and the amount owed. It is not a second expense.
 *
 * Result = income − expenses.
 * Cash flow = income − expenses paid from cash or a bank − card payments − other debt payments.
 * A transfer between your own accounts is ignored.
 */
export function monthCash(rows: CashTx[], start = "", end = ""): CashLine[] {
  const by = new Map<string, CashLine>();
  const ensure = (currency: string) => {
    const row = by.get(currency) ?? {
      currency,
      income: 0,
      expenses: 0,
      cashExpenses: 0,
      cardPayments: 0,
      debtPayments: 0,
      result: 0,
      cashFlow: 0,
    };
    by.set(currency, row);
    return row;
  };
  for (const tx of rows) {
    const currency = tx.currency.trim().toUpperCase();
    if (!currency) continue;
    if (start && tx.date < start) continue;
    if (end && tx.date > end) continue;
    if (tx.direction === "transfer") continue;
    const row = ensure(currency);
    if (tx.direction === "income") row.income += tx.amountMinor;
    else if (tx.direction === "expense") {
      row.expenses += tx.amountMinor;
      if (tx.accountKind !== "credit_card") row.cashExpenses += tx.amountMinor;
    } else if (tx.direction === "payment") {
      if (tx.accountKind === "credit_card") row.cardPayments += tx.amountMinor;
      else row.debtPayments += tx.amountMinor;
    }
  }
  for (const row of by.values()) {
    row.result = row.income - row.expenses;
    row.cashFlow = row.income - row.cashExpenses - row.cardPayments - row.debtPayments;
  }
  return [...by.values()]
    .filter((row) => row.income || row.expenses || row.cardPayments || row.debtPayments)
    .sort((a, b) => a.currency.localeCompare(b.currency));
}

export interface WorthBucket {
  id: "money" | "debt" | "investment" | "credit";
  amountMinor: number;
  natives: { amountMinor: number; currency: string }[];
  unpriced: string[];
}

/** Assets in view, minus the full amount owed. Hiding a debt slice must not add it back. */
export function netOfVisible(buckets: WorthBucket[], hidden: Iterable<string> = []): { assets: number; owed: number; net: number } {
  const hide = new Set(hidden);
  let assets = 0;
  let owed = 0;
  for (const bucket of buckets) {
    if (bucket.id === "debt") {
      owed += Math.max(0, bucket.amountMinor);
      continue;
    }
    if (hide.has(bucket.id)) continue;
    assets += Math.max(0, bucket.amountMinor);
  }
  return { assets, owed, net: assets - owed };
}

/** Owed divided by assets. Above 1 means debts are larger than what is owned. Zero when nothing is owed. */
export function debtRatio(assets: number, owed: number): number {
  if (owed <= 0) return 0;
  if (assets <= 0) return 1;
  return owed / assets;
}

/** Same net as net worth: cash and banks, priced holdings, and card credit, minus debts. Currencies fold into `display`. */
export function worthBuckets(
  accounts: Array<HoldingShape & { id?: string; kind: string }>,
  display: string,
  book: RateBook | null,
  quotes: Quote[],
  nowMs: number,
): { buckets: WorthBucket[]; assets: number; owed: number; net: number; currency: string } {
  const target = display.trim().toUpperCase();
  const buckets: Record<WorthBucket["id"], WorthBucket> = {
    money: { id: "money", amountMinor: 0, natives: [], unpriced: [] },
    debt: { id: "debt", amountMinor: 0, natives: [], unpriced: [] },
    investment: { id: "investment", amountMinor: 0, natives: [], unpriced: [] },
    credit: { id: "credit", amountMinor: 0, natives: [], unpriced: [] },
  };
  const addNative = (bucket: WorthBucket, amountMinor: number, currency: string) => {
    const code = currency.toUpperCase();
    if (!code || code === target || amountMinor === 0) return;
    const found = bucket.natives.find((row) => row.currency === code);
    if (found) found.amountMinor += amountMinor;
    else bucket.natives.push({ amountMinor, currency: code });
  };
  for (const account of accounts) {
    if (isDebt(account.kind)) {
      if (account.shownBalanceMinor == null || !account.currency) {
        buckets.debt.unpriced.push(account.name);
        continue;
      }
      const converted = convertMinor(account.shownBalanceMinor, account.currency, target, book, nowMs);
      if (!converted) buckets.debt.unpriced.push(account.currency.toUpperCase());
      else if (converted.amountMinor < 0) {
        buckets.credit.amountMinor += -converted.amountMinor;
        addNative(buckets.credit, -account.shownBalanceMinor, account.currency);
      } else if (converted.amountMinor > 0) {
        buckets.debt.amountMinor += converted.amountMinor;
        addNative(buckets.debt, account.shownBalanceMinor, account.currency);
      }
      continue;
    }
    if (isHoldingAccount(account)) {
      const position = positionOf(account, target, book, quotes, nowMs);
      if (position.unpriced || position.amountMinor == null) {
        buckets.investment.unpriced.push(position.reason || account.name);
        continue;
      }
      buckets.investment.amountMinor += position.amountMinor;
      const original = originalHolding(account, position, target);
      if (original.nativeMinor != null && original.nativeCurrency && original.nativeCurrency !== target) {
        addNative(buckets.investment, original.nativeMinor, original.nativeCurrency);
      }
      continue;
    }
    if (account.shownBalanceMinor == null || !account.currency) continue;
    const converted = convertMinor(account.shownBalanceMinor, account.currency, target, book, nowMs);
    if (!converted) buckets.money.unpriced.push(account.currency.toUpperCase());
    else {
      buckets.money.amountMinor += converted.amountMinor;
      addNative(buckets.money, account.shownBalanceMinor, account.currency);
    }
  }
  const folded = netOfVisible([buckets.money, buckets.debt, buckets.investment, buckets.credit]);
  return {
    buckets: [buckets.money, buckets.debt, buckets.investment, buckets.credit],
    assets: folded.assets,
    owed: folded.owed,
    net: folded.net,
    currency: target,
  };
}

export interface HoldingOriginal {
  quantity: string;
  nativeMinor: number | null;
  nativeCurrency: string;
}

/** Quantity plus the value in the price currency, when that currency is not the main one. */
export function originalHolding(account: HoldingShape, position: Position, display: string): HoldingOriginal {
  const target = display.trim().toUpperCase();
  const qty = position.quantity;
  const symbol = position.symbol;
  let quantity = "";
  if (qty) {
    if (position.unit === "troy_oz") quantity = `${qty} oz`;
    else if (position.unit === "gram") quantity = `${qty} g`;
    else if (position.unit === "kilogram") quantity = `${qty} kg`;
    else if (symbol) quantity = `${qty} ${symbol}`;
    else quantity = qty;
  }
  let nativeMinor = position.nativeMinor;
  let nativeCurrency = (position.nativeCurrency || "").toUpperCase();
  const priceCurrency = (account.marketPriceCurrency || "").toUpperCase();
  if (account.marketPrice != null && priceCurrency && qty && (!nativeCurrency || nativeCurrency === target)) {
    const major = Number(qty) * account.marketPrice;
    if (Number.isFinite(major) && major > 0) {
      const exp = priceCurrency === "JPY" ? 0 : 2;
      nativeMinor = Math.round(major * 10 ** exp);
      nativeCurrency = priceCurrency;
    }
  }
  if (nativeCurrency === target) {
    nativeMinor = null;
    nativeCurrency = "";
  }
  return { quantity, nativeMinor, nativeCurrency };
}

/** Point-in-time rows already expressed per currency, then folded into the main currency. */
export function netFromBalances(
  accounts: { kind: string; currency: string; shownBalanceMinor: number | null; marketWorthMinor?: number | null; marketWorthCurrency?: string }[],
  display: string,
  book: RateBook | null,
  nowMs: number,
) {
  const rows = netWorth(accounts.map((account) => worthAccount(account)));
  return headlineOf(
    rows.map((row) => ({ amountMinor: row.net, currency: row.currency })),
    display,
    book,
    nowMs,
  );
}
