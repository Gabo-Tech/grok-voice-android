import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const conf = JSON.parse(readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8")) as {
  app: { security: { csp: string } };
};
const csp = conf.app.security.csp;

assert.match(csp, /script-src 'self'/);
assert.equal(csp.includes("unsafe-eval"), false);
assert.equal(csp.includes("script-src 'self' 'unsafe-inline'"), false);
const connect = csp.split(";").map((part) => part.trim()).find((part) => part.startsWith("connect-src"));
assert.ok(connect);
for (const source of connect.split(/\s+/).slice(1)) {
  assert.notEqual(source, "https:", source);
  assert.notEqual(source, "http:", source);
  assert.notEqual(source, "wss:", source);
  assert.notEqual(source, "*", source);
}
assert.match(csp, /https:\/\/api\.x\.ai/);
assert.match(csp, /wss:\/\/api\.x\.ai/);
assert.match(csp, /wss:\/\/api\.openai\.com/);
assert.match(csp, /https:\/\/api\.1min\.ai/);
assert.match(csp, /https:\/\/huggingface\.co/);
assert.match(csp, /https:\/\/api\.frankfurter\.dev/);
assert.match(csp, /https:\/\/docs\.x\.ai/);
assert.match(csp, /object-src 'none'/);
assert.match(csp, /base-uri 'none'/);
assert.match(csp, /frame-src 'none'/);
assert.match(csp, /style-src 'self' 'unsafe-inline'/);

console.log("csp.test.ts ok");
