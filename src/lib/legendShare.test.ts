import assert from "node:assert/strict";
import { legendShares } from "./legendShare.ts";

const halved = legendShares([
  { id: "total", amount: 1_000_000, whole: true },
  { id: "quanloop", amount: 431_000 },
  { id: "xmr", amount: 200_000 },
  { id: "rest", amount: 369_000 },
]);
assert.equal(halved.get("total"), 100);
assert.equal((halved.get("quanloop") || 0) + (halved.get("xmr") || 0) + (halved.get("rest") || 0), 100);
assert.ok(Math.abs((halved.get("quanloop") || 0) - 43.1) < 0.15);

const hidden = legendShares([
  { id: "total", amount: 500, whole: true },
  { id: "a", amount: 250 },
  { id: "b", amount: 250, hidden: true },
]);
assert.equal(hidden.get("total"), 100);
assert.equal(hidden.get("a"), 100);
assert.equal(hidden.get("b"), null);

const omitted = legendShares([
  { id: "cash", amount: 40 },
  { id: "debt", amount: 60, omit: true },
]);
assert.equal(omitted.get("cash"), 100);
assert.equal(omitted.get("debt"), null);
