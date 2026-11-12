/// <reference types="node" />
import assert from "node:assert/strict";
import {
  cashFlow,
  combinedMinor,
  comingRows,
  headlineOf,
  missingCodes,
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
  { currency: "CHF", incoming: 500000, outgoing: 200000, left: 300000 },
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
assert.equal(comingRows([stipend], [], "2026-11-12", "in").length, 1);
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
assert.ok(incomeComing.some((row) => row.name === "June"));
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
assert.equal(convertMinor(1100, "USD", "EUR", manualBook, 1_000)?.amountMinor, 550);
assert.equal(convertMinor(1100, "USD", "EUR", manualBook, 1_000)?.source, "manual");
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

console.log("inset and finance math ok");
