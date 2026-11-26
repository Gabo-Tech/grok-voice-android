import assert from "node:assert/strict";
import {
  SERIES_HEX,
  THEME_CANVAS,
  classToken,
  contrastRatio,
  defaultToken,
  isLegacyColor,
  namedColor,
  paintColors,
  paletteIndex,
  resolveColor,
  shownColor,
  colorWord,
} from "./entityColor.ts";

assert.equal(paletteIndex("quanloop"), 11);
assert.equal(paletteIndex("xmr"), 3);
assert.equal(paletteIndex("silver"), 5);
assert.notEqual(defaultToken("quanloop"), defaultToken("xmr"));
assert.notEqual(defaultToken("quanloop"), defaultToken("silver"));
assert.equal(defaultToken("quanloop"), defaultToken("quanloop"));

assert.equal(shownColor("quanloop", ""), defaultToken("quanloop"));
assert.equal(shownColor("quanloop", "#C4B5A0"), defaultToken("quanloop"));
assert.equal(isLegacyColor("#6E8B74"), true);
assert.equal(shownColor("xmr", "#009E73"), "#009E73");
assert.equal(shownColor("xmr", "var(--series-4)"), "var(--series-4)");
assert.equal(shownColor("silver", "#8A8A8A"), "#8A8A8A");

const painted = paintColors([], "account", "quanloop", "#009E73");
assert.equal(resolveColor(painted, "account", "quanloop", "#C4B5A0"), "#009E73");
assert.equal(resolveColor(painted, "account", "xmr", ""), defaultToken("xmr"));
const cleared = paintColors(painted, "account", "quanloop", "");
assert.equal(resolveColor(cleared, "account", "quanloop", ""), defaultToken("quanloop"));

assert.equal(resolveColor([], "class", "Crypto", "#009E73"), classToken("Crypto"));
assert.notEqual(classToken("Crypto"), classToken("Metals"));
assert.notEqual(classToken("Crypto"), resolveColor([], "account", "xmr", ""));
const classPaint = paintColors([], "class", "Crypto", "#0072B2");
assert.equal(resolveColor(classPaint, "class", "Crypto", ""), "#0072B2");

assert.equal(resolveColor([], "income", "Salary", ""), defaultToken("Salary"));
const income = paintColors([], "income", "Salary", "#0B7A5A");
assert.equal(resolveColor(income, "category", "Salary", "#C4B5A0"), "#0B7A5A");
assert.equal(resolveColor([], "series", "trend:income", ""), "var(--positive)");
assert.equal(resolveColor([], "series", "portfolio:total", ""), "var(--brand)");

assert.equal(namedColor("green"), "#009E73");
assert.equal(namedColor("verde"), "#009E73");
assert.equal(namedColor("gris"), "#8A8A8A");
assert.equal(namedColor("#009e73"), "#009E73");
assert.equal(colorWord("#009E73"), "green");
assert.equal(colorWord("var(--series-1)"), "blue");

for (const hex of SERIES_HEX) {
  for (const canvas of Object.values(THEME_CANVAS)) {
    assert.ok(contrastRatio(hex, canvas) >= 3, `${hex} on ${canvas} is ${contrastRatio(hex, canvas)}`);
  }
}

const round = paintColors(paintColors([], "node", "pool", "var(--series-8)"), "account", "silver", "#8A8A8A");
const again = JSON.parse(JSON.stringify(round)) as typeof round;
assert.equal(resolveColor(again, "node", "pool", ""), "var(--series-8)");
assert.equal(resolveColor(again, "account", "silver", ""), "#8A8A8A");
