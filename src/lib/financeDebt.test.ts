import assert from "node:assert/strict";
import { selectDebt } from "./financeDebt.ts";
import type { RateBook } from "./financeMath.ts";

const now = Date.UTC(2026, 10, 21, 12, 0);
const book: RateBook = {
  asOf: "2026-11-21",
  fetchedMs: now,
  perEur: { EUR: 1, CHF: 0.95, USD: 1.08 },
  manual: [],
};
const today = "2026-11-21";
const accounts = [
  { id: "bank", name: "ZKB", kind: "bank", currency: "CHF", shownBalanceMinor: 80_000 },
  { id: "card", name: "Abanca", kind: "credit_card", currency: "EUR", shownBalanceMinor: 10_000 },
  { id: "credit", name: "Swiss", kind: "credit_card", currency: "CHF", shownBalanceMinor: -4_000 },
  { id: "loan", name: "Car", kind: "loan", currency: "CHF", shownBalanceMinor: 25_000 },
  { id: "iou", name: "Ana", kind: "other_debt", currency: "CHF", shownBalanceMinor: 5_000 },
  { id: "fund", name: "Funds", kind: "investment", currency: "CHF", shownBalanceMinor: 40_000 },
];
const bills = [
  { id: "rent-due", name: "Rent", amountMinor: 3_000, currency: "CHF", date: "2026-11-01", direction: "expense", accountId: "bank" },
  { id: "rent-today", name: "Power", amountMinor: 800, currency: "CHF", date: today, direction: "expense", accountId: "bank" },
  { id: "future", name: "Internet", amountMinor: 9_000, currency: "CHF", date: "2026-12-01", direction: "expense", accountId: "bank" },
  { id: "card-bill", name: "Card bill", amountMinor: 10_000, currency: "EUR", date: "2026-11-10", direction: "expense", accountId: "card" },
  { id: "pay", name: "Loan payment", amountMinor: 2_000, currency: "CHF", date: "2026-11-02", direction: "payment", accountId: "loan" },
  { id: "income", name: "Salary", amountMinor: 50_000, currency: "CHF", date: "2026-11-01", direction: "income", accountId: "bank" },
];

const debt = selectDebt(accounts, bills, "CHF", book, now, today);
const cards = debt.slices.find((slice) => slice.id === "cards");
const loans = debt.slices.find((slice) => slice.id === "loans");
const people = debt.slices.find((slice) => slice.id === "people");
const overdue = debt.slices.find((slice) => slice.id === "bills");

assert.ok(cards && loans && people && overdue);
assert.equal(cards.items.length, 1);
assert.equal(cards.items[0].label, "Abanca");
assert.equal(loans.amountMinor, 25_000);
assert.equal(people.amountMinor, 5_000);
assert.equal(overdue.amountMinor, 3_000 + 800);
assert.equal(overdue.items.some((item) => item.label === "Internet"), false);
assert.equal(overdue.items.some((item) => item.label === "Card bill"), false);
assert.equal(debt.slices.some((slice) => slice.items.some((item) => item.label === "Swiss")), false);
assert.equal(debt.amountMinor, cards.amountMinor + 25_000 + 5_000 + 3_800);
assert.ok(cards.amountMinor > 0);
assert.notEqual(cards.amountMinor, 10_000);
assert.equal(debt.natives.find((row) => row.currency === "EUR")?.amountMinor, 10_000);
assert.equal(debt.natives.some((row) => row.currency === "CHF"), false);

const assets = 80_000 + 40_000 + 4_000;
const net = assets - debt.amountMinor;
assert.equal(net, assets - debt.amountMinor);
assert.notEqual(net, assets + debt.amountMinor);

const same = selectDebt(accounts, bills, "CHF", book, now, today);
assert.equal(same.amountMinor, debt.amountMinor);
assert.deepEqual(same.slices.map((slice) => slice.id), debt.slices.map((slice) => slice.id));
