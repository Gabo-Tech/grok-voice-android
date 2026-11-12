/// <reference types="node" />
import assert from "node:assert/strict";
import { cashFlow, debtPayoff, nextDue } from "./financeMath.ts";
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

console.log("inset and finance math ok");
