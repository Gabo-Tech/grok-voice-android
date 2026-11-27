import assert from "node:assert/strict";
import { budgetAsFlow, clampMonth, lineRemaining, monthlyMinor, overBudget, resolveBudget, shiftMonth, suggestBudgetLines, summarizeBudget, type BudgetRecord } from "./budget.ts";
import { buildMoneyFlow, flowConserved } from "./moneyFlow.ts";
import type { RateBook } from "./financeMath.ts";

const now = Date.UTC(2026, 10, 21, 12, 0);
const book: RateBook = {
  asOf: "2026-11-21",
  fetchedMs: now,
  perEur: { EUR: 1, CHF: 0.95, USD: 1.08 },
  manual: [],
};

const budget: BudgetRecord = {
  id: "b1",
  period: "2026-11",
  currency: "EUR",
  copyForward: true,
  notify: true,
  warned: [],
  updatedMs: 1,
  lines: [
    { id: "s1", role: "income", category: "Salary", subcategory: "", label: "Salary 1 Nov", amountMinor: 200_000, currency: "EUR" },
    { id: "s2", role: "income", category: "Salary", subcategory: "", label: "Salary 15 Nov", amountMinor: 200_000, currency: "EUR" },
    { id: "r1", role: "income", category: "Rent", subcategory: "", label: "", amountMinor: 90_500, currency: "EUR" },
    { id: "d1", role: "income", category: "Dividends", subcategory: "", label: "", amountMinor: 5_000, currency: "USD" },
    { id: "e1", role: "expense", category: "Restaurant", subcategory: "", label: "", amountMinor: 20_000, currency: "EUR" },
    { id: "e2", role: "expense", category: "Housing", subcategory: "Rent", label: "", amountMinor: 80_000, currency: "EUR" },
    { id: "i1", role: "investment", category: "Funds", subcategory: "", label: "", amountMinor: 50_000, currency: "EUR" },
    { id: "v1", role: "savings", category: "Savings", subcategory: "", label: "", amountMinor: 100_000, currency: "EUR" },
  ],
};

const summary = summarizeBudget(
  budget,
  [
    { amountMinor: 200_000, currency: "EUR", direction: "income", category: "Salary", date: "2026-11-01" },
    { amountMinor: 50_000, currency: "EUR", direction: "income", category: "Salary", date: "2026-11-15" },
    { amountMinor: 18_000, currency: "EUR", direction: "expense", category: "Restaurant", date: "2026-11-04" },
    { amountMinor: 90_000, currency: "EUR", direction: "expense", category: "Housing", subcategory: "Rent", date: "2026-11-02" },
    { amountMinor: 10_000, currency: "EUR", direction: "transfer", category: "Funds", date: "2026-11-06", accountKind: "investment" },
  ],
  [
    { scheduleId: "rent", name: "Rent in", date: "2026-11-28", amountMinor: 90_500, currency: "EUR", direction: "income", purpose: "recurring", payoutCurrency: "", amountMode: "fixed", category: "Rent", accountId: "" },
    { scheduleId: "food", name: "Groceries", date: "2026-11-25", amountMinor: 4_000, currency: "EUR", direction: "expense", purpose: "recurring", payoutCurrency: "", amountMode: "fixed", category: "Food", accountId: "" },
    { scheduleId: "old", name: "Already", date: "2026-11-01", amountMinor: 9_999, currency: "EUR", direction: "expense", purpose: "recurring", payoutCurrency: "", amountMode: "fixed", category: "Food", accountId: "" },
  ],
  { month: "2026-11", display: "CHF", book, nowMs: now, today: "2026-11-21", categoryKinds: { funds: "investment" } },
);

assert.equal(summary.incomePlanned > 0, true);
assert.notEqual(summary.rows.find((row) => row.id === "d1")?.plannedMinor, 5_000);
assert.equal(summary.rows.find((row) => row.id === "s1")?.actualMinor, 200_000 * 0.95);
assert.equal(summary.rows.find((row) => row.id === "s2")?.actualMinor, 50_000 * 0.95);
const restaurant = lineRemaining(summary, "restaurants") || lineRemaining(summary, "Restaurant");
assert.ok(restaurant);
assert.equal(restaurant.remainingMinor, Math.round(20_000 * 0.95) - Math.round(18_000 * 0.95));
assert.equal(restaurant.warn, true);
assert.equal(restaurant.over, false);
const housing = lineRemaining(summary, "Housing");
assert.ok(housing);
assert.equal(housing.over, true);
assert.equal(overBudget(summary), true);
assert.equal(summary.savingsActual, summary.incomeActual - summary.spendingActual - summary.investmentsActual);
assert.equal(summary.projectedNet, summary.incomeActual + Math.round(90_500 * 0.95) - (summary.spendingActual + Math.round(4_000 * 0.95)) - summary.investmentsActual);

const template: BudgetRecord = { ...budget, id: "tpl", period: "", lines: budget.lines.slice(0, 2) };
const resolved = resolveBudget([template, { ...budget, period: "2026-10" }], "2026-11");
assert.equal(resolved.inherited, true);
assert.equal(resolved.budget?.period, "2026-11");
assert.equal(resolved.budget?.lines.length, 2);
const own = resolveBudget([template, budget], "2026-11");
assert.equal(own.inherited, false);
assert.equal(own.budget?.lines.length, budget.lines.length);
assert.equal(shiftMonth("2026-11", 1), "2026-12");
assert.equal(shiftMonth("2026-12", 1), "2027-01");

const planned = buildMoneyFlow(budgetAsFlow(budget.lines, "2026-11"), {
  display: "EUR",
  book,
  nowMs: now,
  start: "2026-11-01",
  end: "2026-11-30",
  categories: [{ name: "Funds", color: "", kind: "investment" }],
});
assert.equal(planned.state, "ready");
if (planned.state === "ready") {
  assert.ok(planned.nodes.some((node) => node.label === "Salary" && node.role === "source"));
  assert.ok(planned.nodes.some((node) => node.id === "spend"));
  assert.ok(planned.nodes.some((node) => node.kind === "investment"));
  assert.ok(flowConserved(planned));
}

const quiet = summarizeBudget(
  { ...budget, lines: [budget.lines[4]] },
  [{ amountMinor: 1_000, currency: "EUR", direction: "expense", category: "Restaurant", date: "2026-11-04" }],
  [],
  { month: "2026-11", display: "EUR", book, nowMs: now, today: "2026-11-21" },
);
assert.equal(quiet.rows[0].warn, false);
assert.equal(overBudget(quiet), false);

assert.equal(monthlyMinor(1200, "monthly"), 1200);
assert.equal(monthlyMinor(100, "weekly"), Math.round((100 * 52) / 12));
assert.equal(clampMonth("2020-01", "2024-11", "2027-11"), "2024-11");
assert.equal(clampMonth("2030-01", "2024-11", "2027-11"), "2027-11");

const suggested = suggestBudgetLines({
  source: "both",
  month: "2026-11",
  display: "EUR",
  book,
  nowMs: now,
  rules: [
    { name: "Rent", amountMinor: 80_000, currency: "EUR", direction: "expense", category: "Housing", cadence: "monthly", purpose: "recurring", status: "active" },
    { name: "Mobile", amountMinor: 3_000, currency: "EUR", direction: "expense", category: "Subscriptions", cadence: "monthly", purpose: "recurring", status: "active" },
    { name: "Paused", amountMinor: 9_000, currency: "EUR", direction: "expense", category: "Health", cadence: "monthly", purpose: "recurring", status: "paused" },
  ],
  transactions: [
    { amountMinor: 200_000, currency: "EUR", direction: "income", category: "Salary", date: "2026-10-01" },
    { amountMinor: 200_000, currency: "EUR", direction: "income", category: "Salary", date: "2026-09-01" },
    { amountMinor: 40_000, currency: "EUR", direction: "expense", category: "Food", date: "2026-10-04" },
    { amountMinor: 20_000, currency: "EUR", direction: "expense", category: "Food", date: "2026-09-04" },
    { amountMinor: 80_000, currency: "EUR", direction: "expense", category: "Housing", date: "2026-10-02" },
  ],
});
assert.equal(suggested.find((line) => line.category === "Housing")?.amountMinor, 80_000);
assert.equal(suggested.find((line) => line.category === "Subscriptions")?.amountMinor, 3_000);
assert.equal(suggested.some((line) => line.category === "Health"), false);
assert.equal(suggested.find((line) => line.category === "Salary")?.role, "income");
assert.ok((suggested.find((line) => line.category === "Food")?.amountMinor || 0) > 0);
assert.equal(suggested.find((line) => line.role === "savings")?.category, "Savings");
assert.equal(suggestBudgetLines({ source: "rules", month: "2026-11", display: "EUR", book, nowMs: now, rules: [], transactions: [] }).length, 0);

console.log("budget ok");
