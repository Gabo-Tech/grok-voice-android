import assert from "node:assert/strict";
import { changeRatio, comparisonKey, dashboardRange, moveSection, normalizeLayout, previousRange, savingsRate, spendingPace, topCategories, totalInRange } from "./dashboardMath.ts";
import type { RateBook } from "./financeMath.ts";

const today = new Date(2026, 10, 21);
const thisRange = dashboardRange("this", today);
assert.equal(thisRange.start, "2026-11-01");
assert.equal(thisRange.end, "2026-11-30");
assert.deepEqual(previousRange("this", today, thisRange), { start: "2026-10-01", end: "2026-10-31" });
assert.equal(comparisonKey("this"), "vs last month");

const last = dashboardRange("last", today);
assert.equal(last.start, "2026-10-01");
assert.equal(last.end, "2026-10-31");
assert.deepEqual(previousRange("last", today, last), { start: "2026-09-01", end: "2026-09-30" });

const three = dashboardRange("3m", today);
assert.equal(three.start, "2026-09-01");
assert.equal(three.end, "2026-11-30");
assert.deepEqual(previousRange("3m", today, three), { start: "2026-06-01", end: "2026-08-31" });

const ytd = dashboardRange("ytd", today);
assert.equal(ytd.start, "2026-01-01");
assert.equal(ytd.end, "2026-11-21");
assert.deepEqual(previousRange("ytd", today, ytd), { start: "2025-01-01", end: "2025-11-21" });

const year = dashboardRange("1y", today);
assert.equal(year.start, "2025-11-21");
assert.equal(year.end, "2026-11-21");
const yearPrev = previousRange("1y", today, year);
assert.equal(yearPrev?.end, "2025-11-20");
assert.ok(yearPrev && yearPrev.start < yearPrev.end);

const custom = { start: "2026-03-01", end: "2026-03-10" };
assert.deepEqual(previousRange("custom", today, custom), { start: "2026-02-19", end: "2026-02-28" });
assert.equal(previousRange("custom", today, { start: "", end: "" }), null);

assert.equal(savingsRate(10_000, 4_000), 0.6);
assert.equal(savingsRate(0, 4_000), null);
assert.equal(savingsRate(-1, 4_000), null);
assert.equal(changeRatio(112, 100), 0.12);
assert.equal(changeRatio(88, 100), -0.12);
assert.equal(changeRatio(50, 0), null);

const now = Date.UTC(2026, 10, 21);
const book: RateBook = { asOf: "2026-11-21", fetchedMs: now, perEur: { EUR: 1, CHF: 0.95, USD: 1.08 }, manual: [] };
const rows = [
  { amountMinor: 3_000, currency: "CHF", direction: "income", date: "2026-11-02", category: "Salary" },
  { amountMinor: 1_000, currency: "CHF", direction: "expense", date: "2026-11-04", category: "Rent" },
  { amountMinor: 400, currency: "CHF", direction: "expense", date: "2026-11-08", category: "Food" },
  { amountMinor: 100, currency: "EUR", direction: "expense", date: "2026-11-09", category: "Food" },
  { amountMinor: 9_000, currency: "CHF", direction: "expense", date: "2026-12-01", category: "Rent" },
  { amountMinor: 2_000, currency: "CHF", direction: "expense", date: "2026-10-10", category: "Rent" },
  { amountMinor: 500, currency: "CHF", direction: "expense", date: "2026-10-28", category: "Food" },
  { amountMinor: 800, currency: "CHF", direction: "expense", date: "2026-08-05", category: "Rent" },
];
const november = totalInRange(rows, "expense", thisRange, "CHF", book, now);
assert.equal(november.missing.length, 0);
assert.ok(november.amountMinor != null && november.amountMinor > 1_400);
assert.notEqual(november.amountMinor, 1_500);
const income = totalInRange(rows, "income", thisRange, "CHF", book, now);
assert.equal(income.amountMinor, 3_000);
assert.equal(savingsRate(income.amountMinor || 0, november.amountMinor || 0) != null, true);

const top = topCategories(rows, thisRange, "CHF", book, now, 5);
assert.equal(top.rows[0].name, "Rent");
assert.equal(top.rows[1].name, "Food");
assert.ok(top.rows[1].amountMinor > 400);

const blocked = totalInRange([{ amountMinor: 100, currency: "USD", direction: "expense", date: "2026-11-02" }], "expense", thisRange, "CHF", null, now);
assert.equal(blocked.amountMinor, null);
assert.deepEqual(blocked.missing, ["USD"]);

const pace = spendingPace(rows, today, "CHF", book, now);
assert.ok(pace);
assert.equal(pace?.months, 2);
assert.equal(pace?.through, "2026-11-21");
assert.equal(pace?.averageMinor, Math.round((2_000 + 800) / 2));
assert.ok((pace?.spentMinor || 0) > 1_400);
const emptyPace = spendingPace([{ amountMinor: 100, currency: "CHF", direction: "expense", date: "2026-11-02" }], today, "CHF", null, now);
assert.equal(emptyPace, null);

const layout = normalizeLayout(["debt", "hero", "nope"], ["flow", "flow"]);
assert.equal(layout.order[0], "debt");
assert.equal(layout.order[1], "hero");
assert.equal(layout.order.includes("pace"), true);
assert.deepEqual(layout.hidden, ["flow"]);
assert.deepEqual(moveSection(layout.order, "hero", -1)[0], "hero");
assert.deepEqual(moveSection(["hero", "result"], "hero", 1), ["result", "hero"]);

console.log("dashboard math ok");
