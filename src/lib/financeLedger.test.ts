import assert from "node:assert/strict";
import { headlineOf, type RateBook } from "./financeMath.ts";
import { allocation, barsFor, flowParts, formatQuantity, gainOf, monthActivity, netWorthParts, persistedColor, positionOf, scaledBars, shareOf, ALLOCATION_PRESETS, type AllocInput, type Quote } from "./financeLedger.ts";

const now = Date.UTC(2026, 10, 21, 12, 0);
const book: RateBook = {
  asOf: "2026-11-21",
  fetchedMs: now,
  perEur: { EUR: 1, CHF: 0.95, USD: 1.08 },
  manual: [],
};
const quotes: Quote[] = [
  { symbol: "XAG", price: 28.4, currency: "EUR", source: "gold-api", fetchedMs: now },
  { symbol: "XMR", price: 320, currency: "EUR", source: "coingecko", fetchedMs: now },
  { symbol: "ZANO", price: 0.42, currency: "EUR", source: "coingecko", fetchedMs: now - 26 * 60 * 60 * 1000 },
  { symbol: "EPIC", price: 0.18, currency: "EUR", source: "coinpaprika", fetchedMs: now },
];

const rows = monthActivity(
  [
    { currency: "CHF", direction: "expense", amountMinor: 1850, date: "2026-11-03", recurrence: "once" },
    { currency: "EUR", direction: "expense", amountMinor: 120000, date: "2026-11-02", recurrence: "monthly", ruleId: "rent" },
    { currency: "USD", direction: "expense", amountMinor: 4299, date: "2026-11-04" },
    { currency: "CHF", direction: "payment", amountMinor: 20000, date: "2026-11-06" },
    { currency: "CHF", direction: "transfer", amountMinor: 5000, date: "2026-11-07" },
    { currency: "CHF", direction: "income", amountMinor: 350000, date: "2026-11-01" },
    { currency: "CHF", direction: "payment", amountMinor: 15000, date: "2026-11-08" },
    { currency: "CHF", direction: "expense", amountMinor: 900, date: "2026-10-02" },
  ],
  [
    { date: "2026-11-18", direction: "expense", amountMinor: 9000, currency: "CHF" },
    { date: "2026-11-28", direction: "expense", amountMinor: 1000, currency: "CHF" },
    { date: "2026-11-05", direction: "payment", amountMinor: 4000, currency: "CHF" },
  ],
  "2026-11-01",
  "2026-11-30",
  "2026-11-21",
);
const spent = flowParts(rows, "2026-11-01", "2026-11-30", "expense");
const earned = flowParts(rows, "2026-11-01", "2026-11-30", "income");
assert.equal(spent.some((part) => part.currency === "CHF" && part.amountMinor === 1850), true);
assert.equal(spent.some((part) => part.currency === "EUR" && part.amountMinor === 120000), true);
assert.equal(spent.some((part) => part.currency === "USD" && part.amountMinor === 4299), true);
assert.equal(spent.some((part) => part.amountMinor === 9000), true);
assert.equal(spent.some((part) => part.amountMinor === 1000), false);
assert.equal(spent.some((part) => part.amountMinor === 20000), false);
assert.equal(spent.some((part) => part.amountMinor === 5000), false);
assert.equal(spent.some((part) => part.amountMinor === 15000), false);
assert.equal(earned.reduce((sum, part) => sum + part.amountMinor, 0), 350000);

const displayBook = book;
const expenseChf = headlineOf(spent, "CHF", displayBook, now);
const expenseUsd = headlineOf(spent, "USD", displayBook, now);
assert.equal(expenseChf.state, "ready");
assert.equal(expenseUsd.state, "ready");
assert.equal(expenseChf.currency, "CHF");
assert.equal(expenseUsd.currency, "USD");
assert.notEqual(expenseChf.amountMinor, expenseUsd.amountMinor);

const silver = positionOf(
  { name: "Silver", kind: "other", currency: "CHF", units: "5", measureUnit: "ounces", investmentType: "custom", notes: "5 ounces" },
  "CHF",
  book,
  quotes,
  now,
);
assert.equal(silver.quantity, "5");
assert.equal(silver.unpriced, false);
assert.equal(silver.currency, "CHF");
assert.equal(silver.source, "gold-api");
assert.ok((silver.amountMinor || 0) > 0);

const xmr = positionOf({ name: "XMR", kind: "investment", currency: "XMR", units: "2.969859854727", ticker: "XMR" }, "EUR", book, quotes, now);
assert.equal(xmr.quantity, "2.969859854727");
assert.notEqual(xmr.quantity, "2.97");
assert.equal(formatQuantity("16.4600"), "16.46");
assert.equal(formatQuantity("16.46"), "16.46");

const zano = positionOf({ name: "ZANO", kind: "investment", currency: "ZANO", units: "16.46", ticker: "ZANO" }, "CHF", book, quotes, now);
assert.equal(zano.quantity, "16.46");
assert.equal(zano.stale, true);
assert.equal(zano.unpriced, false);

const epic = positionOf({ name: "EPIC Cash", kind: "other", currency: "CHF", notes: "128.60340395", investmentType: "custom" }, "CHF", book, quotes, now);
assert.equal(epic.quantity, "128.60340395");
assert.equal(epic.symbol, "EPIC");
assert.equal(epic.unpriced, false);

const cached = positionOf(
  { name: "Silver", kind: "investment", currency: "CHF", units: "5", measureUnit: "troy_oz" },
  "CHF",
  book,
  [{ symbol: "XAG", price: 28.4, currency: "EUR", source: "gold-api", fetchedMs: now - 3 * 24 * 60 * 60 * 1000 }],
  now,
);
assert.equal(cached.unpriced, false);
assert.equal(cached.stale, true);

const missing = positionOf({ name: "Silver", kind: "investment", currency: "CHF", units: "5", measureUnit: "troy_oz" }, "CHF", book, [], now);
assert.equal(missing.unpriced, true);
assert.equal(missing.quantity, "5");

const accounts = [
  { id: "zano", name: "ZANO", kind: "investment", currency: "ZANO", units: "16.46", ticker: "ZANO", shownBalanceMinor: 1_646_000_000 },
  { id: "quan", name: "Quanloop", kind: "investment", currency: "EUR", investmentType: "custom", shownBalanceMinor: 462153 },
  { id: "swiss", name: "Swisscard", kind: "credit_card", currency: "CHF", shownBalanceMinor: 62785, lastFour: "8810" },
  { id: "abanca-b", name: "Abanca", kind: "bank", currency: "EUR", shownBalanceMinor: 70 },
  { id: "abanca-c", name: "Abanca", kind: "credit_card", currency: "EUR", shownBalanceMinor: 1035, lastFour: "4412" },
  { id: "epic", name: "EPIC Cash", kind: "other", currency: "CHF", notes: "128.60340395" },
  { id: "mystery", name: "Unknown coin", kind: "investment", currency: "CHF", units: "3", ticker: "NOPE" },
];
const bars = barsFor(accounts, "CHF", book, quotes, now);
const zanoBar = bars.assets.find((row) => row.id === "zano");
const quanBar = bars.assets.find((row) => row.id === "quan");
const mystery = bars.assets.find((row) => row.id === "mystery");
assert.ok(zanoBar && quanBar && mystery);
assert.ok(quanBar.width > zanoBar.width);
assert.ok(zanoBar.width > 0 && zanoBar.width < 1);
assert.equal(mystery.width, 0);
assert.equal(mystery.included, false);
assert.equal(bars.debts.some((row) => row.id === "swiss"), true);
assert.equal(bars.assets.some((row) => row.kind === "credit_card"), false);
assert.ok(bars.assets.some((row) => row.label.includes("Bank")));
assert.ok(bars.debts.some((row) => row.label.includes("4412")));

const raw = scaledBars([
  { id: "zano", name: "ZANO", kind: "investment", displayMinor: 1_646_000_000, debt: false, unpriced: false },
  { id: "chf", name: "Swisscard", kind: "credit_card", displayMinor: 62785, debt: true, unpriced: false },
]);
assert.equal(raw.assets[0].width, 1);
const fair = scaledBars([
  { id: "zano", name: "ZANO", kind: "investment", displayMinor: 700, debt: false, unpriced: false },
  { id: "quan", name: "Quanloop", kind: "investment", displayMinor: 400000, debt: false, unpriced: false },
  { id: "gap", name: "Gap", kind: "investment", displayMinor: null, debt: false, unpriced: true },
]);
assert.ok(fair.assets[1].width === 1);
assert.ok(fair.assets[0].width < 0.01);
assert.equal(fair.assets[2].width, 0);

const worthChf = netWorthParts(
  [
    { name: "ZKB", kind: "bank", currency: "CHF", shownBalanceMinor: 100000 },
    { name: "ZANO", kind: "investment", currency: "ZANO", units: "16.46", ticker: "ZANO", shownBalanceMinor: 1_646_000_000 },
    { name: "Swisscard", kind: "credit_card", currency: "CHF", shownBalanceMinor: 30000 },
    { name: "Loan", kind: "loan", currency: "CHF", shownBalanceMinor: 10000 },
  ],
  "CHF",
  book,
  quotes,
  now,
);
const worthUsd = netWorthParts(
  [
    { name: "ZKB", kind: "bank", currency: "CHF", shownBalanceMinor: 100000 },
    { name: "ZANO", kind: "investment", currency: "ZANO", units: "16.46", ticker: "ZANO", shownBalanceMinor: 1_646_000_000 },
    { name: "Swisscard", kind: "credit_card", currency: "CHF", shownBalanceMinor: 30000 },
    { name: "Loan", kind: "loan", currency: "CHF", shownBalanceMinor: 10000 },
  ],
  "USD",
  book,
  quotes,
  now,
);
const chfHead = headlineOf(worthChf.parts, "CHF", book, now);
const usdHead = headlineOf(worthUsd.parts, "USD", book, now);
assert.equal(chfHead.currency, "CHF");
assert.equal(usdHead.currency, "USD");
assert.notEqual(chfHead.amountMinor, usdHead.amountMinor);
assert.ok((chfHead.amountMinor || 0) > 0);

const rolex = {
  name: "Rolex",
  kind: "investment",
  currency: "CHF",
  investmentType: "collectible",
  shownBalanceMinor: 1_400_000,
  priceOverride: true,
  purchaseMinor: 1_200_000,
  purchaseCurrency: "CHF",
  valuations: [
    { asOf: "2024-06-01", amountMinor: 1_200_000, currency: "CHF" },
    { asOf: "2026-11-01", amountMinor: 1_400_000, currency: "CHF" },
  ],
};
const gain = gainOf(rolex, "CHF", book, [], now);
assert.equal(gain.unpriced, false);
assert.equal(gain.amountMinor, 200_000);
const gainUsd = gainOf(rolex, "USD", book, [], now);
assert.equal(gainUsd.currency, "USD");
assert.notEqual(gainUsd.amountMinor, gain.amountMinor);

const held: AllocInput[] = [
  { id: "xmr", name: "XMR", kind: "investment", assetClass: "Crypto", color: "#112233", amountMinor: 50000, unpriced: false, quantity: "2.969859854727", source: "coingecko", fetchedMs: now, stale: false, estimated: false, cash: false },
  { id: "zano", name: "ZANO", kind: "investment", assetClass: "Crypto", amountMinor: 30000, unpriced: false, quantity: "16.46", source: "coingecko", fetchedMs: now, stale: true, estimated: false, cash: false },
  { id: "silver", name: "Silver", kind: "investment", assetClass: "Metals", amountMinor: 15000, unpriced: false, quantity: "5", source: "gold-api", fetchedMs: now, stale: false, estimated: false, cash: false },
  { id: "rolex", name: "Rolex", kind: "investment", assetClass: "Manual", amountMinor: 4000, unpriced: false, quantity: "", source: "estimate", fetchedMs: 0, stale: false, estimated: true, cash: false },
  { id: "dust", name: "Dust", kind: "investment", assetClass: "Crypto", amountMinor: 500, unpriced: false, quantity: "1", source: "coingecko", fetchedMs: now, stale: false, estimated: false, cash: false },
  { id: "pebble", name: "Pebble", kind: "investment", assetClass: "Manual", amountMinor: 400, unpriced: false, quantity: "", source: "estimate", fetchedMs: 0, stale: false, estimated: true, cash: false },
  { id: "gap", name: "Mystery", kind: "investment", assetClass: "Other", amountMinor: null, unpriced: true, quantity: "3", source: "", fetchedMs: 0, stale: false, estimated: false, cash: false },
  { id: "zkb", name: "ZKB", kind: "bank", assetClass: "Cash", amountMinor: 100000, unpriced: false, quantity: "", source: "stated", fetchedMs: 0, stale: false, estimated: false, cash: true },
];
const pie = allocation(held, { group: "item", includeCash: false, palette: ALLOCATION_PRESETS.okabe });
assert.equal(pie.unpriced.length, 1);
assert.equal(pie.unpriced[0].name, "Mystery");
assert.equal(pie.total, 50000 + 30000 + 15000 + 4000 + 500 + 400);
assert.equal(pie.slices.some((slice) => slice.id === "zkb"), false);
const xmrShare = shareOf(pie.slices, "XMR");
assert.ok(xmrShare != null && Math.abs(xmrShare - 50000 / pie.total) < 1e-9);
assert.equal(pie.slices.find((slice) => slice.id === "xmr")?.color, "#112233");
assert.equal(persistedColor("#112233", "#000000"), "#112233");
assert.equal(persistedColor("blue", "#0072B2"), "#0072B2");
const tiny = pie.slices.find((slice) => slice.other);
assert.ok(tiny && tiny.members.some((row) => row.name === "Dust"));
const withCash = allocation(held, { group: "item", includeCash: true, expanded: true });
assert.ok(withCash.slices.some((slice) => slice.id === "zkb"));
const byClass = allocation(held, { group: "class", includeCash: false, expanded: true });
const crypto = byClass.slices.find((slice) => slice.label === "Crypto");
assert.ok(crypto && crypto.members.length >= 2);
assert.ok(Math.abs((shareOf(byClass.slices, "XMR") || 0) - 50000 / byClass.total) < 1e-9);
const again = allocation(held.map((row) => (row.id === "silver" ? { ...row, color: "#8A8A8A" } : row)), { group: "item", includeCash: false, expanded: true });
assert.equal(again.slices.find((slice) => slice.id === "silver")?.color, "#8A8A8A");

console.log("finance ledger ok");
