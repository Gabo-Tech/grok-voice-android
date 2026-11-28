/// <reference types="node" />
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const app = readFileSync(new URL("../App.tsx", import.meta.url), "utf8");
const css = `${readFileSync(new URL("../index.css", import.meta.url), "utf8")}\n${readFileSync(new URL("../finance.css", import.meta.url), "utf8")}`;

assert.match(app, /function StagePane/);
assert.match(app, /hidden=\{!active\}/);
assert.match(app, /aria-hidden=\{active \? undefined : true\}/);
assert.match(app, /inert=\{active \? undefined : true\}/);
assert.match(app, /className=\{active \? "stage-page is-active/);

const hidden = css.match(/\.app-stage > \.stage-page:not\(\.is-active\) \{[^}]+\}/);
assert.ok(hidden, "inactive stage panes need an explicit hide rule");
assert.match(hidden[0], /display:\s*none\s*!important/);
assert.match(hidden[0], /pointer-events:\s*none/);

assert.doesNotMatch(
  css,
  /\.app-stage > div:has\(> \.finance\) \{[^}]*display\s*:/,
  "the finance layout rule must not set display, or a hidden Finance pane stays on screen",
);

const financePane = css.match(/\.app-stage > \.stage-page\.is-active:has\(> \.finance\) \{[^}]+\}/);
assert.ok(financePane, "the active finance pane still needs its column scroll lock");
assert.doesNotMatch(financePane[0], /display\s*:/);

const active = css.match(/\.app-stage > \.stage-page\.is-active \{[^}]+\}/);
assert.ok(active, "the active pane needs an explicit animation lock");
assert.match(active[0], /animation:\s*none/);
assert.doesNotMatch(active[0], /page-in/);
assert.match(css, /@media \(prefers-reduced-motion: reduce\) \{[\s\S]*\.app-stage > \.stage-page\.is-active \{[\s\S]*animation:\s*none\s*!important/);
assert.match(css, /html\[data-animator="off"\] \.app-stage > \.stage-page\.is-active \{\s*animation:\s*none\s*!important/);

console.log("stagePage.test.ts ok");
