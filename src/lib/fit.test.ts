import assert from "node:assert/strict";
import { classify, recommend } from "./fit.ts";

const GIB = 1024 * 1024 * 1024;
const SMOL = 144_811_360;
const QWEN08 = 811_843_840;
const QWEN2 = 1_213_300_992;
const GEMMA4 = 2_620_370_976;
const ATOMIC = 2_708_804_608;

assert.equal(classify(8 * GIB, ATOMIC, 8), "fits");
assert.equal(classify(8 * GIB, GEMMA4, 8), "fits");
assert.equal(
  recommend(
    [
      { id: "gemma4", sizeBytes: GEMMA4, minRamGb: 8, strength: 8 },
      { id: "atomic", sizeBytes: ATOMIC, minRamGb: 8, strength: 9 },
      { id: "qwen2", sizeBytes: QWEN2, minRamGb: 3, strength: 7 },
    ],
    8 * GIB,
  ),
  "atomic",
);
assert.equal(classify(2 * GIB, SMOL, 2), "fits");
assert.equal(classify(2 * GIB, QWEN08, 2), "no");
assert.equal(classify(3 * GIB, QWEN2, 3), "tight");
assert.equal(classify(3 * GIB, QWEN08, 2), "fits");
assert.equal(classify(0, ATOMIC, 8), "unknown");
console.log("fit.test.ts ok");
