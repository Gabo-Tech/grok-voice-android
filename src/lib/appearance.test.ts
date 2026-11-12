/// <reference types="node" />
import assert from "node:assert/strict";
import {
  PRESETS,
  colorsFromPreset,
  customFromString,
  customToString,
  normalizeAppearance,
  paletteFromCustom,
  relativeLuminance,
} from "./appearance.ts";

assert.equal(normalizeAppearance(""), "graphite");
assert.equal(normalizeAppearance("komi"), "graphite");
assert.equal(normalizeAppearance("Midnight"), "midnight");
assert.equal(normalizeAppearance("custom"), "custom");
assert.equal(normalizeAppearance("system"), "system");

const names = Object.keys(PRESETS).sort();
assert.deepEqual(names, ["carbon", "forest", "graphite", "ink", "ivory", "midnight", "plum", "sand", "stone"]);

for (const id of ["ivory", "sand", "stone"] as const) {
  assert.equal(PRESETS[id].scheme, "light", id);
  assert.ok(relativeLuminance(PRESETS[id].canvas) > 0.45, id);
}
for (const id of ["graphite", "ink", "midnight", "carbon", "forest", "plum"] as const) {
  assert.equal(PRESETS[id].scheme, "dark", id);
  assert.ok(relativeLuminance(PRESETS[id].canvas) < 0.2, id);
}

const graphite = colorsFromPreset("graphite");
assert.equal(graphite.canvas, "#0E0F11");
assert.equal(graphite.income, "#7D9A86");
assert.equal(graphite.expense, "#C48984");
assert.equal(customToString(graphite).split(",").length, 7);
assert.deepEqual(customFromString(customToString(graphite)), graphite);
assert.equal(customFromString("#fff"), null);
assert.equal(customFromString("red,green,blue,white,gold,sage,rose"), null);

const custom = paletteFromCustom(graphite);
assert.equal(custom.canvas, "#0E0F11");
assert.equal(custom.scheme, "dark");
const ivory = paletteFromCustom(colorsFromPreset("ivory"));
assert.equal(ivory.scheme, "light");
assert.equal(ivory.sage, PRESETS.ivory.sage);

console.log("appearance ok");
