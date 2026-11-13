/// <reference types="node" />
import assert from "node:assert/strict";
import {
  bookIsStale,
  cashFlow,
  combinedMinor,
  addDays,
  cardDueDate,
  comingCashRows,
  comingRows,
  paymentFits,
  utilization,
  headlineOf,
  missingCodes,
  quoteRate,
  sameLocalDay,
  subtractHeadlines,
  convertMinor,
  debtPayoff,
  monthBounds,
  netWorth,
  netWorthChange,
  nextDue,
  occurrenceDates,
  payoutTrack,
  plannedHits,
  projectWindow,
  scheduleProgress,
  type RateBook,
  type ScheduleShape,
} from "./financeMath.ts";
import { catalogHas, catalogStats } from "./i18n.ts";
import { financePanels, FINANCE_TABS, readFinanceTab } from "./financeTabs.ts";
import { appliedFilters, clearedFilters, withoutFilter, type LedgerFilters } from "./financeFilters.ts";
import { readFileSync } from "node:fs";
import { resolveInset, scrollDelta } from "./keyboard.ts";

const overlay = resolveInset(800, 800, 400, 0, 400);
assert.equal(overlay.open, true);
assert.equal(overlay.pad, 400);
assert.equal(overlay.visibleBottom, 400);

const resized = resolveInset(800, 400, 400, 0, 400);
assert.equal(resized.open, true);
assert.equal(resized.pad, 0, "a resized WebView must not pad the keyboard height again");
assert.equal(resized.visibleBottom, 400);

const closed = resolveInset(800, 800, 800, 0, 0);
assert.equal(closed.open, false);
assert.equal(closed.pad, 0);
assert.equal(closed.visibleBottom, 800);

const blindViewport = resolveInset(800, 800, 800, 0, 420);
assert.equal(blindViewport.open, true);
assert.equal(blindViewport.pad, 420);
assert.equal(blindViewport.visibleBottom, 380);

assert.equal(scrollDelta(780, 400, 12), 392);
assert.equal(scrollDelta(300, 400, 12), 0);
assert.equal(scrollDelta(388, 400, 12), 0);

const flow = cashFlow(
  [
    { currency: "CHF", direction: "income", amountMinor: 500000, date: "2026-11-02" },
    { currency: "CHF", direction: "expense", amountMinor: 120000, date: "2026-11-03" },
    { currency: "CHF", direction: "payment", amountMinor: 80000, date: "2026-11-04" },
    { currency: "EUR", direction: "income", amountMinor: 20000, date: "2026-11-04" },
    { currency: "USD", direction: "expense", amountMinor: 999, date: "2026-10-30" },
    { currency: "EUR", direction: "expense", amountMinor: 100, date: "2026-12-01" },
  ],
  "2026-11-01",
  "2026-11-30",
);
assert.deepEqual(flow, [
  { currency: "CHF", incoming: 500000, outgoing: 120000, left: 380000 },
  { currency: "EUR", incoming: 20000, outgoing: 0, left: 20000 },
]);

const card = {
  id: "card",
  kind: "credit_card",
  currency: "CHF",
  shownBalanceMinor: 450000,
  balanceUpdatedMs: 100,
  balanceAsOf: "2026-11-01",
};
const payoff = debtPayoff(card, [
  { accountId: "card", fromAccountId: "", currency: "CHF", direction: "payment", amountMinor: 50000, createdMs: 50, date: "2026-10-01" },
  { accountId: "card", fromAccountId: "", currency: "CHF", direction: "payment", amountMinor: 50000, createdMs: 200, date: "2026-11-05" },
  { accountId: "card", fromAccountId: "", currency: "EUR", direction: "payment", amountMinor: 90000, createdMs: 200, date: "2026-11-06" },
  { accountId: "other", fromAccountId: "", currency: "CHF", direction: "payment", amountMinor: 10000, createdMs: 200, date: "2026-11-06" },
]);
assert.deepEqual(payoff, { remaining: 450000, paid: 50000, ratio: 50000 / 500000 });
assert.equal(debtPayoff({ ...card, kind: "bank" }, []), null);
assert.equal(debtPayoff({ ...card, shownBalanceMinor: null }, []), null);

const due = nextDue(
  { id: "loan", currency: "EUR", dueDate: "2026-12-01", paymentMinor: 10000 },
  [
    { id: "later", accountId: "loan", nextDate: "2027-01-01", amountMinor: 10000, currency: "EUR" },
    { id: "soon", accountId: "loan", nextDate: "2026-11-20", amountMinor: 10000, currency: "" },
    { id: "other", accountId: "card", nextDate: "2026-11-01", amountMinor: 1, currency: "EUR" },
  ],
);
assert.deepEqual(due, { id: "soon", date: "2026-11-20", amount: 10000, currency: "EUR" });
assert.deepEqual(nextDue({ id: "loan", currency: "EUR", dueDate: "2026-12-01", paymentMinor: 10000 }, []), {
  id: "",
  date: "2026-12-01",
  amount: 10000,
  currency: "EUR",
});

const course: ScheduleShape = {
  id: "course",
  name: "Course",
  direction: "expense",
  purpose: "recurring",
  amountMinor: 27000,
  currency: "EUR",
  cadence: "monthly",
  startDate: "2026-11-01",
  endKind: "after_count",
  endCount: 9,
  status: "active",
};
const courseDates = occurrenceDates(course, "2026-11-01");
assert.equal(courseDates.length, 9);
assert.equal(courseDates[0], "2026-11-01");
assert.equal(courseDates[8], "2027-07-01");
const coursePaid = courseDates.slice(0, 3).map((date) => ({
  ruleId: "course",
  occurrenceDate: date,
  date,
  amountMinor: 27000,
  currency: "EUR",
  direction: "expense",
}));
const courseProgress = scheduleProgress(course, coursePaid, "2027-02-01");
assert.equal(courseProgress.mode, "fixed");
assert.equal(courseProgress.paidCount, 3);
assert.equal(courseProgress.totalCount, 9);
assert.equal(courseProgress.remainingCount, 6);
assert.equal(courseProgress.remainingMinor, 162000);
assert.equal(courseProgress.currency, "EUR");
const coursePlanned = plannedHits([course], coursePaid, "2027-02-01").filter((hit) => hit.date.startsWith("2026-11"));
assert.equal(coursePlanned.length, 0);
const courseFlow = cashFlow(coursePaid, "2026-11-01", "2026-11-30");
assert.equal(courseFlow[0].outgoing, 27000);
assert.equal(courseFlow[0].currency, "EUR");

const rent: ScheduleShape = {
  id: "rent",
  name: "Rent",
  direction: "expense",
  purpose: "recurring",
  amountMinor: 200000,
  currency: "CHF",
  cadence: "monthly",
  startDate: "2026-11-01",
  endKind: "none",
  status: "active",
};
const rentProgress = scheduleProgress(rent, [], "2026-11-01");
assert.equal(rentProgress.mode, "open");
assert.equal(rentProgress.totalCount, null);
assert.equal(rentProgress.remainingCount, null);
assert.equal(rentProgress.remainingMinor, null);
assert.equal(rentProgress.perPeriodMinor, 200000);
assert.equal(rentProgress.nearTermCount, 12);
assert.equal(rentProgress.nearTermMinor, 2_400_000);
const rentHits = plannedHits([rent], [], "2026-11-01");
const rentWindow = projectWindow([], rentHits, "2026-11-01", "2026-11-30", "2026-11-01");
assert.equal(rentWindow.length, 1);
assert.ok(rentWindow[0].comingOut < 100 * 200000);
assert.equal(rentWindow[0].comingOut, 200000);

const eggs: ScheduleShape = {
  id: "eggs",
  name: "Nitroeggs",
  direction: "income",
  purpose: "investment_payout",
  amountMinor: 1400,
  currency: "USD",
  amountMode: "quote",
  payoutCurrency: "XMR",
  cadence: "every_n_days",
  intervalDays: 10,
  startDate: "2026-11-01",
  endKind: "none",
  status: "active",
};
assert.deepEqual(occurrenceDates(eggs, "2026-11-12").slice(0, 3), ["2026-11-01", "2026-11-11", "2026-11-21"]);
const eggTrack = payoutTrack(
  eggs,
  [{ ruleId: "eggs", occurrenceDate: "2026-11-01", date: "2026-11-01", amountMinor: 9_000_000, currency: "XMR", direction: "income" }],
  "2026-11-12",
);
assert.equal(eggTrack.expectedCount, 2);
assert.equal(eggTrack.expectedMinor, 2800);
assert.equal(eggTrack.expectedCurrency, "USD");
assert.equal(eggTrack.receivedCount, 1);
assert.deepEqual(eggTrack.received, [{ currency: "XMR", amountMinor: 9_000_000 }]);
assert.equal(eggTrack.nextDate, "2026-11-11");
assert.notEqual(eggTrack.expectedMinor, eggTrack.received[0].amountMinor);

const stipend: ScheduleShape = {
  id: "june",
  name: "June",
  direction: "income",
  purpose: "one_off",
  amountMinor: 50000,
  currency: "EUR",
  cadence: "",
  startDate: "2027-06-01",
  endKind: "after_count",
  endCount: 1,
  status: "active",
};
assert.equal(cashFlow([{ currency: "EUR", direction: "income", amountMinor: 50000, date: "2027-06-01" }], "2026-11-01", "2026-11-30").length, 0);
assert.equal(comingRows([stipend], [], "2026-11-12", "in").length, 0);
assert.equal(plannedHits([stipend], [], "2026-11-12")[0].date, "2027-06-01");
const onceProgress = scheduleProgress(stipend, [], "2026-11-12");
assert.equal(onceProgress.mode, "once");
assert.equal(onceProgress.remainingMinor, 50000);

const sub: ScheduleShape = {
  id: "netflix",
  name: "Netflix",
  direction: "expense",
  purpose: "subscription",
  amountMinor: 1500,
  currency: "EUR",
  cadence: "monthly",
  startDate: "2026-11-12",
  endKind: "none",
  status: "stopped",
};
assert.equal(comingRows([sub], [], "2026-11-12", "out").length, 0);
assert.equal(scheduleProgress(sub, [], "2026-11-12").nearTermMinor, 0);
assert.equal(scheduleProgress(sub, [], "2026-11-12").remainingMinor, null);

const worth = netWorth([
  { kind: "bank", currency: "EUR", shownBalanceMinor: 100000 },
  { kind: "cash", currency: "EUR", shownBalanceMinor: 5000 },
  { kind: "investment", currency: "USD", shownBalanceMinor: 70000 },
  { kind: "credit_card", currency: "CHF", shownBalanceMinor: 19000 },
  { kind: "loan", currency: "EUR", shownBalanceMinor: 20000 },
]);
const eurWorth = worth.find((row) => row.currency === "EUR");
assert.equal(eurWorth?.cashSavings, 105000);
assert.equal(eurWorth?.owed, 20000);
assert.equal(eurWorth?.owned, 105000);
assert.equal(eurWorth?.net, 85000);
assert.equal(worth.find((row) => row.currency === "USD")?.invested, 70000);
assert.equal(worth.find((row) => row.currency === "CHF")?.net, -19000);
assert.equal(worth.reduce((sum, row) => sum + row.net, 0) === 85000 + 70000 - 19000, true);
const month = monthBounds("2026-11-12");
const monthFlow = cashFlow(
  [
    { currency: "EUR", direction: "income", amountMinor: 300000, date: "2026-11-02" },
    { currency: "EUR", direction: "expense", amountMinor: 40000, date: "2026-11-03" },
  ],
  month.start,
  month.end,
);
assert.equal(monthFlow[0].left, 260000);
assert.notEqual(monthFlow[0].left, eurWorth?.net);
const bills = comingRows([course, rent], coursePaid, "2027-02-01", "out");
assert.ok(bills.some((row) => row.name === "Course" && row.progress.paidCount === 3 && row.progress.remainingCount === 6));
assert.ok(bills.some((row) => row.name === "Rent" && row.progress.remainingMinor === null));
const incomeComing = comingRows([eggs, stipend], [], "2026-11-12", "in");
assert.ok(incomeComing.some((row) => row.name === "Nitroeggs"));
assert.equal(incomeComing.some((row) => row.name === "June"), false);
const leftover = projectWindow(monthFlow, plannedHits([rent], [], "2026-11-12"), month.start, month.end, "2026-11-12", true);
assert.equal(leftover.find((row) => row.currency === "EUR")?.projected, 260000);
assert.equal(leftover.find((row) => row.currency === "CHF")?.comingOut, 200000);
assert.equal(leftover.find((row) => row.currency === "CHF")?.projected, -200000);
const rentAhead = plannedHits([rent], [], "2026-11-12");
const nextThirty = projectWindow([], rentAhead, "2026-11-12", "2026-12-12", "2026-11-12", false);
const nextChf = nextThirty.find((row) => row.currency === "CHF");
assert.equal(nextChf?.actualLeft, 0);
assert.equal(nextChf?.comingOut, 200000);
assert.equal(nextThirty.find((row) => row.currency === "EUR"), undefined);
const overdueOnly = projectWindow([], rentAhead, "1970-01-01", "2026-11-11", "2026-11-12", false);
assert.equal(overdueOnly.find((row) => row.currency === "CHF")?.comingOut, 200000);
assert.equal(overdueOnly.find((row) => row.currency === "CHF")?.actualLeft, 0);

const fresh: RateBook = { asOf: "2026-11-12", fetchedMs: 1_000, perEur: { EUR: 1, USD: 1.1, CHF: 0.95 }, manual: [] };
const changed = convertMinor(1100, "USD", "EUR", fresh, 1_000);
assert.equal(changed?.amountMinor, 1000);
assert.equal(changed?.stale, false);
assert.equal(changed?.source, "cached");
assert.equal(convertMinor(1100, "USD", "XMR", fresh, 1_000), null);
const hero = combinedMinor(
  [
    { amountMinor: 10000, currency: "EUR" },
    { amountMinor: 1100, currency: "USD" },
  ],
  "EUR",
  fresh,
  1_000,
);
assert.equal(hero?.amountMinor, 11000);
assert.notEqual(hero?.amountMinor, 11100);
assert.equal(combinedMinor([{ amountMinor: 10000, currency: "EUR" }, { amountMinor: 100, currency: "XMR" }], "EUR", fresh, 1_000), null);
const staleBook: RateBook = { ...fresh, fetchedMs: 1_000 - 49 * 60 * 60 * 1000 };
assert.equal(convertMinor(1100, "USD", "EUR", staleBook, 1_000)?.stale, true);
assert.equal(convertMinor(1100, "USD", "EUR", staleBook, 1_000)?.amountMinor, 1000);
const manualBook: RateBook = { ...fresh, manual: [{ base: "USD", quote: "EUR", rate: 0.5, asOf: "2026-01-01" }] };
assert.equal(convertMinor(1100, "USD", "EUR", manualBook, 1_000)?.amountMinor, 1000);
assert.equal(convertMinor(1100, "USD", "EUR", manualBook, 1_000)?.source, "cached");
assert.notEqual(convertMinor(1100, "USD", "EUR", manualBook, 1_000)?.amountMinor, 550);
const overrideBook: RateBook = { ...fresh, manual: [{ base: "USD", quote: "EUR", rate: 0.5, asOf: "2026-01-01", overrideLive: true }] };
assert.equal(convertMinor(1100, "USD", "EUR", overrideBook, 1_000)?.amountMinor, 550);
assert.equal(convertMinor(1100, "USD", "EUR", overrideBook, 1_000)?.source, "manual");
assert.equal(convertMinor(-3000, "EUR", "USD", fresh, 1_000)?.amountMinor, -3300);
assert.equal(combinedMinor([{ amountMinor: 85000, currency: "EUR" }], "EUR", null, 1_000)?.source, "identity");

const mixed = headlineOf(
  [
    { amountMinor: 10_000, currency: "EUR" },
    { amountMinor: 1_100, currency: "USD" },
  ],
  "EUR",
  fresh,
  1_000,
);
assert.equal(mixed.state, "ready");
assert.equal(mixed.amountMinor, 11_000);
assert.equal(mixed.currency, "EUR");
assert.notEqual(mixed.amountMinor, 11_100);
assert.deepEqual(mixed.missing, []);

const blocked = headlineOf(
  [
    { amountMinor: 10_000, currency: "EUR" },
    { amountMinor: 100, currency: "XMR" },
  ],
  "EUR",
  fresh,
  1_000,
);
assert.equal(blocked.state, "needs-rate");
assert.equal(blocked.amountMinor, 0);
assert.deepEqual(blocked.missing, ["XMR"]);
assert.notEqual(blocked.amountMinor, 10_100);
assert.deepEqual(missingCodes([{ amountMinor: 10_000, currency: "EUR" }, { amountMinor: 100, currency: "XMR" }], "EUR", fresh, 1_000), ["XMR"]);

const usdOut = headlineOf([{ amountMinor: 1_100, currency: "USD" }], "EUR", fresh, 1_000);
const eurIn = headlineOf([{ amountMinor: 10_000, currency: "EUR" }], "EUR", fresh, 1_000);
const net = subtractHeadlines(eurIn, usdOut, "EUR");
assert.equal(net.state, "ready");
assert.equal(net.amountMinor, 9_000);
assert.notEqual(net.amountMinor, 8_900);
const blockedNet = subtractHeadlines(eurIn, headlineOf([{ amountMinor: 100, currency: "XMR" }], "EUR", fresh, 1_000), "EUR");
assert.equal(blockedNet.state, "needs-rate");
assert.equal(blockedNet.amountMinor, 0);
assert.deepEqual(blockedNet.missing, ["XMR"]);
const moved = netWorthChange(
  [
    { currency: "EUR", direction: "income", amountMinor: 300000, date: "2026-11-02", accountKind: "bank" },
    { currency: "EUR", direction: "expense", amountMinor: 40000, date: "2026-11-03", accountKind: "bank" },
    { currency: "EUR", direction: "payment", amountMinor: 10000, date: "2026-11-04", accountKind: "credit_card", fromKind: "bank" },
  ],
  month.start,
  month.end,
);
assert.equal(moved[0].change, 260000);

const now = Date.now();
const start = new Date(now);
start.setHours(0, 0, 0, 0);
const yesterdayMs = start.getTime() - 60_000;
assert.equal(sameLocalDay(now, now), true);
assert.equal(sameLocalDay(yesterdayMs, now), false);
assert.equal(now - yesterdayMs < 48 * 60 * 60 * 1000, true);
const yesterdayBook: RateBook = { ...fresh, fetchedMs: yesterdayMs, manual: [{ base: "USD", quote: "EUR", rate: 0.5, asOf: "2026-01-01" }] };
assert.equal(bookIsStale(yesterdayBook, now), true);
assert.equal(convertMinor(1100, "USD", "EUR", yesterdayBook, now)?.amountMinor, 1000);
assert.equal(convertMinor(1100, "USD", "EUR", yesterdayBook, now)?.stale, true);
assert.equal(convertMinor(1100, "USD", "EUR", yesterdayBook, now)?.source, "cached");
assert.notEqual(convertMinor(1100, "USD", "EUR", yesterdayBook, now)?.amountMinor, 550);
const todayBook: RateBook = { ...fresh, fetchedMs: now, manual: yesterdayBook.manual };
assert.equal(bookIsStale(todayBook, now), false);
assert.equal(quoteRate("USD", "EUR", todayBook, now)?.source, "cached");
assert.notEqual(quoteRate("USD", "EUR", todayBook, now)?.rate, 1);
assert.equal(quoteRate("USD", "XMR", todayBook, now), null);
const fallbackOnly: RateBook = { asOf: "", fetchedMs: 0, perEur: {}, manual: [{ base: "XMR", quote: "EUR", rate: 150, asOf: "2026-01-01" }] };
assert.equal(convertMinor(100_000_000, "XMR", "EUR", fallbackOnly, now)?.amountMinor, 15_000);
assert.equal(convertMinor(100_000_000, "XMR", "EUR", fallbackOnly, now)?.source, "manual");
assert.equal(convertMinor(1100, "USD", "EUR", fallbackOnly, now), null);
const stillBlocked = headlineOf(
  [
    { amountMinor: 10_000, currency: "EUR" },
    { amountMinor: 1_100, currency: "USD" },
  ],
  "EUR",
  fallbackOnly,
  now,
);
assert.equal(stillBlocked.state, "needs-rate");
assert.equal(stillBlocked.amountMinor, 0);
assert.notEqual(stillBlocked.amountMinor, 11_100);

const counts = catalogStats();
assert.equal(new Set(counts.map((row) => row.count)).size, 1);
for (const key of [
  "Today's rate",
  "Fetched {time}",
  "Live rate for the day. Native amounts stay on each item.",
  "1 {code} = {rate} {quote}",
  "Manual rates",
  "A fallback when today's rate is missing.",
  "Use this instead of the live rate",
  "Set a manual rate in Settings.",
  "Using your rate",
]) {
  assert.equal(catalogHas(key), true, key);
}

const dashboard = readFileSync(new URL("../components/FinanceDashboard.tsx", import.meta.url), "utf8");
const charts = readFileSync(new URL("../components/FinanceCharts.tsx", import.meta.url), "utf8");
const page = readFileSync(new URL("../components/FinancePage.tsx", import.meta.url), "utf8");
const combined = readFileSync(new URL("../components/CombinedTotal.tsx", import.meta.url), "utf8");
const settings = readFileSync(new URL("../components/CurrencyEditor.tsx", import.meta.url), "utf8");
const more = readFileSync(new URL("../components/MorePage.tsx", import.meta.url), "utf8");
const math = readFileSync(new URL("./financeMath.ts", import.meta.url), "utf8");
assert.equal(dashboard.includes("setManualRate"), false);
assert.equal(dashboard.includes("RatePanel"), false);
assert.equal(charts.includes("RatePanel"), false);
assert.equal(settings.includes("Manual rates"), true);
assert.equal(settings.includes("Use this instead of the live rate"), true);
assert.equal(settings.includes('inputMode="decimal"'), true);

assert.deepEqual(financePanels("overview"), ["net-worth", "expenses", "income", "result", "upcoming"]);
assert.deepEqual(financePanels("expenses"), ["expenses", "activity", "coming-expenses"]);
assert.deepEqual(financePanels("income"), ["income", "activity", "coming-income", "payouts"]);
let panelAt = -1;
for (const id of financePanels("overview")) {
  const token = `data-finance-panel="${id}"`;
  const next = dashboard.indexOf(token);
  assert.ok(next > panelAt, token);
  panelAt = next;
}
assert.ok(dashboard.indexOf('t("Net worth")') < dashboard.indexOf('t("This month\'s expenses")'));
assert.ok(dashboard.indexOf('t("This month\'s expenses")') < dashboard.indexOf('t("This month\'s income")'));
assert.ok(dashboard.indexOf('t("This month\'s income")') < dashboard.indexOf('t("This month\'s result")'));
assert.ok(dashboard.includes("financePanels(place)"));
assert.equal(dashboard.includes("LiveRates"), false);

const rateSurfaces = [dashboard, charts, page, combined];
for (const src of rateSurfaces) {
  assert.equal(src.includes("LiveRates"), false, "LiveRates");
  assert.equal(src.includes("RatePanel"), false, "RatePanel");
  assert.equal(src.includes("Today's rate"), false, "Today's rate");
  assert.equal(src.includes("1 {code} = {rate} {quote}"), false, "rate book");
}
assert.equal(page.includes("CurrencyEditor"), false);
assert.equal(more.includes("<CurrencyEditor"), true);
assert.equal(settings.includes("Today's rate"), true);
assert.equal(settings.includes("Manual rates"), true);

assert.deepEqual([...FINANCE_TABS], ["overview", "expenses", "income", "accounts", "charts"]);
assert.equal(readFinanceTab(null), "overview");
assert.equal(readFinanceTab("charts"), "charts");
assert.equal(readFinanceTab("activity"), "overview");
assert.equal(readFinanceTab("rates"), "overview");
for (const id of FINANCE_TABS) {
  assert.ok(page.includes(`tab === "${id}"`), id);
  assert.ok(page.includes(`data-finance-tabpanel="${id}"`), id);
  assert.ok(page.includes("role=\"tablist\""), id);
}
assert.ok(page.includes("sessionStorage"));
assert.ok(page.includes("grok-finance-tab"));
assert.ok(page.includes("useFinanceRateWatch"));
assert.ok(page.includes("refreshRates"));
assert.ok(page.includes("appliedFilters"));
assert.ok(page.includes('t("Filters")'));
assert.ok(page.includes("fin-actions"));
assert.ok(page.includes("chip-scroller"));
assert.equal(page.includes('className="mb-2 flex flex-wrap gap-2"'), false);

const blankFilters: LedgerFilters = { account: "all", category: "all", currency: "all", recurrence: "all", direction: "expense", group: "all" };
assert.deepEqual(appliedFilters(blankFilters, { account: "Mercury" }, "expense"), []);
const narrowed = { ...blankFilters, account: "acct-mercury", currency: "CHF", direction: "payment" };
const chips = appliedFilters(narrowed, { account: "Mercury", currency: "CHF", direction: "Payment" }, "expense");
assert.deepEqual(chips.map((chip) => chip.id), ["account", "currency", "direction"]);
assert.equal(withoutFilter(narrowed, "currency", "expense").currency, "all");
assert.equal(withoutFilter(narrowed, "direction", "expense").direction, "expense");
assert.equal(clearedFilters("income").direction, "income");
assert.equal(clearedFilters("any").direction, "all");

const rentDates = occurrenceDates(
  { id: "rent", name: "Rent", direction: "expense", cadence: "monthly", startDate: "2026-01-20", amountMinor: 120000, currency: "EUR" },
  "2026-11-12",
);
assert.ok(rentDates.includes("2026-11-20"));
assert.ok(rentDates.includes("2026-12-20"));
const payoutDates = occurrenceDates(
  { id: "eggs", name: "Nitroeggs", direction: "income", purpose: "investment_payout", cadence: "every_n_days", intervalDays: 10, startDate: "2026-11-10", amountMinor: 1400, currency: "USD" },
  "2026-11-12",
);
assert.ok(payoutDates.includes("2026-11-10"));
assert.ok(payoutDates.includes("2026-11-20"));
const once = occurrenceDates(
  { id: "bonus", name: "Bonus", direction: "income", cadence: "", startDate: "2026-11-20", amountMinor: 50000, currency: "EUR" },
  "2026-11-12",
);
assert.deepEqual(once, ["2026-11-20"]);

const calendar = readFileSync(new URL("../components/CalendarPage.tsx", import.meta.url), "utf8");
assert.ok(calendar.includes('event.source === "finance"'));
assert.ok(calendar.includes('t("From Finance")'));
assert.ok(calendar.includes('t("Scheduled")'));

const overrideAt = math.indexOf("manualQuote(book, sourceCode, target, true)");
const cachedAt = math.indexOf("cachedQuote(book, sourceCode, target)");
const fallbackAt = math.indexOf("manualQuote(book, sourceCode, target, false)");
assert.ok(overrideAt > 0 && overrideAt < cachedAt && cachedAt < fallbackAt);
assert.ok(math.includes("sourceCode === target"));

for (const key of [
  "This month's expenses",
  "This month's income",
  "This month's result",
  "Paid this month.",
  "Received this month.",
  "Still ahead. Open a tab for the full list.",
  "Add expense",
  "Investment payouts",
  "Tap a chart to filter this screen.",
  "Filters",
  "Clear filters",
  "Remove filter",
  "Categories",
  "Currencies",
  "Scheduled",
  "From Finance",
]) {
  assert.equal(catalogHas(key), true, key);
}

const today = "2026-11-12";
const inWindow: ScheduleShape = {
  id: "soon",
  name: "Soon",
  direction: "expense",
  amountMinor: 1500,
  currency: "EUR",
  cadence: "",
  startDate: "2026-12-12",
  endKind: "after_count",
  endCount: 1,
  status: "active",
};
const day31: ScheduleShape = { ...inWindow, id: "day31", name: "Day 31", startDate: addDays(today, 31) };
const travel: ScheduleShape = {
  id: "travel",
  name: "Renew travel card",
  direction: "expense",
  amountMinor: 9000,
  currency: "EUR",
  cadence: "yearly",
  startDate: "2025-10-12",
  endKind: "none",
  status: "active",
};
const windowRows = comingRows([inWindow, day31, travel], [], today, "out");
assert.ok(windowRows.some((row) => row.name === "Soon" && row.date === "2026-12-12"));
assert.equal(windowRows.some((row) => row.name === "Day 31"), false);
assert.equal(windowRows.some((row) => row.name === "Renew travel card"), false);

const bankMove = netWorthChange(
  [{ currency: "EUR", direction: "expense", amountMinor: 4000, date: today, accountKind: "bank" }],
  today,
  today,
);
const cardMove = netWorthChange(
  [{ currency: "EUR", direction: "expense", amountMinor: 4000, date: today, accountKind: "credit_card" }],
  today,
  today,
);
assert.equal(bankMove[0].change, -4000);
assert.equal(cardMove[0].change, -4000);
const payMove = netWorthChange(
  [{ currency: "EUR", direction: "payment", amountMinor: 4000, date: today, accountKind: "credit_card", fromKind: "bank" }],
  today,
  today,
);
assert.equal(payMove[0].change, 0);
const result = cashFlow(
  [
    { currency: "EUR", direction: "expense", amountMinor: 4000, date: today },
    { currency: "EUR", direction: "payment", amountMinor: 4000, date: today },
  ],
  today,
  today,
);
assert.equal(result[0].outgoing, 4000);
assert.equal(result[0].left, -4000);

const visa = { id: "visa", name: "Visa", kind: "credit_card", currency: "EUR", shownBalanceMinor: 8000, paymentMinor: null, dueDate: "2026-11-20", dueDay: 20 };
const rentDue: ScheduleShape = {
  id: "rent-bank",
  name: "Rent",
  direction: "expense",
  amountMinor: 2000,
  currency: "EUR",
  cadence: "",
  startDate: "2026-11-18",
  accountId: "bank",
  status: "active",
};
const grocery: ScheduleShape = {
  id: "groceries",
  name: "Groceries",
  direction: "expense",
  amountMinor: 3000,
  currency: "EUR",
  cadence: "",
  startDate: "2026-11-15",
  accountId: "visa",
  status: "active",
};
const cashComing = comingCashRows(
  [rentDue, grocery],
  [],
  [visa, { id: "bank", name: "Checking", kind: "bank", currency: "EUR", shownBalanceMinor: 50000, paymentMinor: null, dueDate: "", dueDay: null }],
  today,
  "out",
);
assert.ok(cashComing.some((row) => row.name === "Rent"));
assert.equal(cashComing.some((row) => row.name === "Groceries"), false);
assert.ok(cashComing.some((row) => row.name === "Visa payment" && row.date === "2026-11-20" && row.amountMinor === 8000));
assert.equal(cardDueDate(visa, today), "2026-11-20");
assert.equal(paymentFits(8000, 8001), "over");
assert.equal(paymentFits(8000, 8000), "ok");
assert.equal(paymentFits(null, 100), "unknown");
assert.equal(utilization(4000, 8000), 0.5);
assert.equal(utilization(4000, null), null);

const incomeSoon: ScheduleShape = { ...inWindow, id: "pay", name: "Payday", direction: "income", startDate: "2026-12-01" };
const incomeLate: ScheduleShape = { ...incomeSoon, id: "late", name: "Later pay", startDate: addDays(today, 31) };
const incomeRows = comingRows([incomeSoon, incomeLate], [], today, "in");
assert.ok(incomeRows.some((row) => row.name === "Payday"));
assert.equal(incomeRows.some((row) => row.name === "Later pay"), false);

console.log("inset and finance math ok");
