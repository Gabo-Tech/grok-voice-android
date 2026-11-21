import assert from "node:assert/strict";
import { buildMoneyFlow } from "./moneyFlow.ts";
import { migrateCategories, mergeCategory, reorderCategories, UNCATEGORIZED } from "./financeCategories.ts";
import { boundsOf } from "./chartPeriod.ts";
import type { RateBook } from "./financeMath.ts";

const now = Date.UTC(2026, 10, 21, 12, 0);
const book: RateBook = {
  asOf: "2026-11-21",
  fetchedMs: now,
  perEur: { EUR: 1, CHF: 0.95, USD: 1.08 },
  manual: [],
};

const flow = buildMoneyFlow(
  [
    { amountMinor: 350000, currency: "CHF", direction: "income", category: "Salary", date: "2026-11-01" },
    { amountMinor: 50000, currency: "EUR", direction: "income", category: "Freelance", date: "2026-11-02" },
    { amountMinor: 120000, currency: "EUR", direction: "expense", category: "Housing", date: "2026-11-02", accountKind: "bank" },
    { amountMinor: 4299, currency: "USD", direction: "expense", category: "Groceries", date: "2026-11-04", accountKind: "credit_card" },
    { amountMinor: 20000, currency: "CHF", direction: "payment", category: "Swisscard", date: "2026-11-06", accountKind: "credit_card", fromKind: "bank" },
    { amountMinor: 5000, currency: "CHF", direction: "transfer", category: "Move", date: "2026-11-07", accountKind: "bank", fromKind: "bank" },
    { amountMinor: 900, currency: "CHF", direction: "expense", category: "Coffee", date: "2026-10-02" },
  ],
  { display: "CHF", book, nowMs: now, start: "2026-11-01", end: "2026-11-30", threshold: 0.03 },
);
assert.equal(flow.state, "ready");
if (flow.state !== "ready") throw new Error("flow");
const salary = flow.nodes.find((node) => node.label === "Salary");
const freelance = flow.nodes.find((node) => node.label === "Freelance");
const housing = flow.nodes.find((node) => node.label === "Housing");
const card = flow.nodes.find((node) => node.label === "Swisscard");
const pool = flow.nodes.find((node) => node.id === "pool");
assert.ok(salary && freelance && housing && card && pool);
assert.equal(salary.amountMinor, 350000);
assert.equal(freelance.originals.find((row) => row.currency === "EUR")?.amountMinor, 50000);
assert.notEqual(freelance.amountMinor, 50000);
assert.equal(housing.originals.find((row) => row.currency === "EUR")?.amountMinor, 120000);
assert.equal(card.kind, "debt");
const expenseSum = flow.nodes.filter((node) => node.kind === "expense").reduce((sum, node) => sum + node.amountMinor, 0);
const debtSum = flow.nodes.filter((node) => node.kind === "debt").reduce((sum, node) => sum + node.amountMinor, 0);
assert.equal(debtSum, 20000);
assert.ok(expenseSum > 0);
assert.equal(flow.nodes.some((node) => node.label === "Move"), false);
assert.equal(flow.nodes.some((node) => node.label === "Coffee"), false);
assert.equal(pool.amountMinor, flow.nodes.filter((node) => node.role === "source").reduce((sum, node) => sum + node.amountMinor, 0));
assert.equal(pool.amountMinor, flow.nodes.filter((node) => node.role === "sink").reduce((sum, node) => sum + node.amountMinor, 0));
const spent = expenseSum + debtSum + flow.nodes.filter((node) => node.kind === "other").reduce((sum, node) => sum + node.amountMinor, 0);
assert.ok(spent <= pool.amountMinor);
assert.equal(flow.nodes.some((node) => node.kind === "surplus" || node.kind === "drawn" || node.kind === "other"), true);

const deficit = buildMoneyFlow(
  [
    { amountMinor: 10000, currency: "CHF", direction: "income", category: "Salary", date: "2026-11-01" },
    { amountMinor: 40000, currency: "CHF", direction: "expense", category: "Housing", date: "2026-11-02" },
  ],
  { display: "CHF", book, nowMs: now, start: "2026-11-01", end: "2026-11-30" },
);
assert.equal(deficit.state, "ready");
if (deficit.state === "ready") {
  const drawn = deficit.nodes.find((node) => node.kind === "drawn");
  assert.ok(drawn);
  assert.equal(drawn.amountMinor, 30000);
  assert.equal(deficit.nodes.find((node) => node.id === "pool")?.amountMinor, 40000);
  assert.equal(deficit.nodes.some((node) => node.kind === "surplus"), false);
  assert.notEqual(deficit.nodes.find((node) => node.id === "pool")?.amountMinor, 10000 + 40000);
}

const surplus = buildMoneyFlow(
  [
    { amountMinor: 80000, currency: "CHF", direction: "income", category: "", date: "2026-11-01" },
    { amountMinor: 10000, currency: "CHF", direction: "expense", category: "Food", date: "2026-11-03" },
    { amountMinor: 5000, currency: "CHF", direction: "transfer", category: "Investing", date: "2026-11-04", accountKind: "investment" },
  ],
  { display: "CHF", book, nowMs: now, start: "2026-11-01", end: "2026-11-30", categories: [{ name: "Investing", color: "#4C78D8", kind: "investment" }] },
);
assert.equal(surplus.state, "ready");
if (surplus.state === "ready") {
  assert.equal(surplus.nodes.find((node) => node.label === "Uncategorized" && node.role === "source")?.amountMinor, 80000);
  assert.equal(surplus.nodes.find((node) => node.kind === "investment")?.amountMinor, 5000);
  assert.equal(surplus.nodes.find((node) => node.kind === "surplus")?.amountMinor, 65000);
  assert.equal(surplus.nodes.filter((node) => node.kind === "expense").reduce((sum, node) => sum + node.amountMinor, 0), 10000);
}

const tiny = buildMoneyFlow(
  [
    { amountMinor: 100000, currency: "CHF", direction: "income", category: "Salary", date: "2026-11-01" },
    { amountMinor: 50000, currency: "CHF", direction: "expense", category: "Housing", date: "2026-11-02" },
    { amountMinor: 1000, currency: "CHF", direction: "expense", category: "Coffee", date: "2026-11-03" },
    { amountMinor: 800, currency: "CHF", direction: "expense", category: "Tips", date: "2026-11-04" },
  ],
  { display: "CHF", book, nowMs: now, start: "2026-11-01", end: "2026-11-30", threshold: 0.03 },
);
assert.equal(tiny.state, "ready");
if (tiny.state === "ready") {
  const other = tiny.nodes.find((node) => node.id === "out:other");
  assert.ok(other);
  assert.equal(other.amountMinor, 1800);
  assert.equal(other.members.length, 2);
  assert.equal(tiny.nodes.some((node) => node.label === "Coffee"), false);
  const open = buildMoneyFlow(
    [
      { amountMinor: 100000, currency: "CHF", direction: "income", category: "Salary", date: "2026-11-01" },
      { amountMinor: 50000, currency: "CHF", direction: "expense", category: "Housing", date: "2026-11-02" },
      { amountMinor: 1000, currency: "CHF", direction: "expense", category: "Coffee", date: "2026-11-03" },
      { amountMinor: 800, currency: "CHF", direction: "expense", category: "Tips", date: "2026-11-04" },
    ],
    { display: "CHF", book, nowMs: now, start: "2026-11-01", end: "2026-11-30", threshold: 0.03, expandedOther: true },
  );
  assert.equal(open.state, "ready");
  if (open.state === "ready") assert.equal(open.nodes.some((node) => node.label === "Coffee"), true);
}

const blocked = buildMoneyFlow(
  [{ amountMinor: 100, currency: "JPY", direction: "income", category: "Salary", date: "2026-11-01" }],
  { display: "CHF", book, nowMs: now, start: "2026-11-01", end: "2026-11-30" },
);
assert.equal(blocked.state, "needs-rate");
if (blocked.state === "needs-rate") assert.deepEqual(blocked.missing, ["JPY"]);

const saved = migrateCategories(
  [{ name: "Groceries", color: "#112233" }, { name: "Salary", color: "nope" }],
  [
    { category: "", direction: "expense" },
    { category: "Groceries", direction: "expense" },
    { category: "Salary", direction: "income" },
  ],
);
assert.equal(saved.categories.find((row) => row.name === "Groceries")?.color, "#112233");
assert.equal(saved.categories.find((row) => row.name === "Salary")?.kind, "income");
assert.equal(saved.categories.some((row) => row.name === UNCATEGORIZED), true);
assert.deepEqual(saved.rewrites, [{ from: "", to: UNCATEGORIZED }]);
const merged = mergeCategory(saved.categories, "Groceries", UNCATEGORIZED);
assert.equal(merged.some((row) => row.name === "Groceries"), false);
assert.equal(merged.some((row) => row.name === UNCATEGORIZED), true);
const ordered = reorderCategories(
  [
    { name: "A", color: "#112233", order: 0 },
    { name: "B", color: "#223344", order: 1 },
  ],
  "B",
  -1,
);
assert.equal(ordered[0].name, "B");
assert.equal(ordered[0].order, 0);

const november = boundsOf({ preset: "this", start: "", end: "" }, new Date(2026, 10, 21));
assert.equal(november.start, "2026-11-01");
assert.equal(november.end, "2026-11-30");
const last = boundsOf({ preset: "last", start: "", end: "" }, new Date(2026, 10, 21));
assert.equal(last.start, "2026-10-01");
assert.equal(last.end, "2026-10-31");
const custom = boundsOf({ preset: "custom", start: "2026-01-05", end: "2026-02-02" }, new Date(2026, 10, 21));
assert.equal(custom.start, "2026-01-05");
assert.equal(custom.end, "2026-02-02");

console.log("money flow ok");
