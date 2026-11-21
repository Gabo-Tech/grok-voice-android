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

function aliasOf(account: HoldingShape): { symbol: string; kind: string } | null {
  const keys = [account.ticker, account.currency, account.name, account.notes].map((value) => (value || "").trim().toLowerCase());
  for (const key of keys) {
    if (ALIAS[key]) return ALIAS[key];
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
  const symbol = (account.ticker || alias?.symbol || "").toUpperCase();
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
    const signed = isDebt(account.kind) ? -Math.abs(converted.amountMinor) : converted.amountMinor;
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
