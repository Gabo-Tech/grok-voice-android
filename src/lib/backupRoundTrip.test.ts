/// <reference types="node" />
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { PERSISTED_WEB_KEYS, readPersistedWeb, writePersistedWeb } from "./persistedKeys.ts";
import { countPhrase } from "./plurals.ts";

const root = new URL("../../", import.meta.url);

function read(path: string) {
  return readFileSync(new URL(path, root), "utf8");
}

function rustList(source: string, name: string) {
  const start = source.indexOf(`const ${name}:`);
  assert.ok(start >= 0, name);
  const end = source.indexOf("];", start);
  const body = source.slice(start, end);
  return [...body.matchAll(/"([^"]+)"/g)].map((match) => match[1]);
}

function walk(dir: string, suffix: RegExp, out: string[] = []) {
  for (const name of readdirSync(dir)) {
    if (name === "node_modules" || name === "target" || name === "gen") continue;
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path, suffix, out);
    else if (suffix.test(name)) out.push(path);
  }
  return out;
}

const persist = read("src-tauri/src/persist.rs");
const stores = rustList(persist, "STORES");
const excluded = rustList(persist, "EXCLUDED_JOINS");
const webKeys = rustList(persist, "WEB_KEYS");
const sessionKeys = rustList(persist, "SESSION_KEYS");
function rustNames(source: string, name: string) {
  const start = source.indexOf(`const ${name}:`);
  assert.ok(start >= 0, name);
  const end = source.indexOf("];", start);
  const body = source.slice(start, end);
  return [...body.matchAll(/\("([^"]+)"/g)].map((match) => match[1]);
}

const nativePrefs = rustNames(persist, "NATIVE_PREFS");

assert.deepEqual([...PERSISTED_WEB_KEYS], webKeys, "persistedKeys.ts drifted from WEB_KEYS");

const lib = read("src-tauri/src/lib.rs");
for (const match of lib.matchAll(/\.join\("([^"]+)"\)/g)) {
  const name = match[1];
  assert.ok(stores.includes(name) || excluded.includes(name), `unclassified store ${name}`);
}

const backup = read("src-tauri/src/backup.rs");
for (const id of stores) {
  assert.ok(backup.includes(`"${id}"`), `backup round-trip does not mention ${id}`);
}

function storageKeys(text: string, api: "localStorage" | "sessionStorage") {
  const hits = new Set<string>();
  const constants = new Map<string, string>();
  for (const match of text.matchAll(/const (\w+) = "([^"]+)"/g)) constants.set(match[1], match[2]);
  const call = new RegExp(`${api}\\.(?:get|set|remove)Item\\(\\s*([^),\\s]+)`, "g");
  for (const match of text.matchAll(call)) {
    const arg = match[1].trim();
    const named = arg.match(/^"([^"]+)"$/);
    const key = named?.[1] ?? constants.get(arg);
    if (key) hits.add(key);
  }
  return hits;
}

const src = new URL("src", root).pathname;
const webHits = new Set<string>();
const sessionHits = new Set<string>();
for (const file of walk(src, /\.(tsx|ts)$/)) {
  if (file.endsWith("backupRoundTrip.test.ts") || file.endsWith("persistedKeys.ts")) continue;
  const text = readFileSync(file, "utf8");
  for (const key of storageKeys(text, "localStorage")) webHits.add(key);
  for (const key of storageKeys(text, "sessionStorage")) sessionHits.add(key);
}
for (const key of webHits) assert.ok(webKeys.includes(key), `localStorage key ${key} is not in WEB_KEYS`);
for (const key of webKeys) assert.ok(webHits.has(key), `WEB_KEYS entry ${key} is unused`);
for (const key of sessionHits) assert.ok(sessionKeys.includes(key), `sessionStorage key ${key} is not in SESSION_KEYS`);

const kotlinRoot = new URL("src-tauri/gen/android/app/src/main/java", root).pathname;
const prefHits = new Set<string>();
for (const file of walk(kotlinRoot, /\.kt$/)) {
  const text = readFileSync(file, "utf8");
  const constants = new Map<string, string>();
  for (const match of text.matchAll(/const val (\w+) = "([^"]+)"/g)) constants.set(match[1], match[2]);
  for (const match of text.matchAll(/getSharedPreferences\(\s*([^,)]+)/g)) {
    const arg = match[1].trim();
    const named = arg.match(/^"([^"]+)"$/);
    const name = named ? named[1] : constants.get(arg);
    assert.ok(name, `unresolved SharedPreferences in ${file}: ${arg}`);
    prefHits.add(name);
  }
}
for (const name of prefHits) assert.ok(nativePrefs.includes(name), `SharedPreferences ${name} is not classified`);
for (const name of nativePrefs) assert.ok(prefHits.has(name), `native pref ${name} is unused`);

const memory = new Map<string, string>();
Object.defineProperty(globalThis, "localStorage", {
  configurable: true,
  value: {
    getItem: (key: string) => memory.get(key) ?? null,
    setItem: (key: string, value: string) => memory.set(key, value),
    removeItem: (key: string) => memory.delete(key),
  },
});

const original: Record<string, string> = {};
for (const key of PERSISTED_WEB_KEYS) {
  original[key] = `saved:${key}`;
  localStorage.setItem(key, original[key]);
}
localStorage.setItem("not-a-user-key", "drop me");
assert.deepEqual(readPersistedWeb(), original);
for (const key of PERSISTED_WEB_KEYS) localStorage.removeItem(key);
assert.deepEqual(readPersistedWeb(), {});
writePersistedWeb(original, true);
assert.deepEqual(readPersistedWeb(), original);
writePersistedWeb({}, true);
assert.deepEqual(readPersistedWeb(), {});
localStorage.setItem(PERSISTED_WEB_KEYS[0], "keep");
writePersistedWeb(undefined, false);
assert.equal(localStorage.getItem(PERSISTED_WEB_KEYS[0]), "keep");

const kinds = ["note", "person", "event", "reminder", "alarm", "countdown", "account", "key", "fact", "transaction", "schedule"] as const;
for (const locale of ["en", "es", "de", "pt", "ca-ES-valencia"]) {
  for (const kind of kinds) {
    const zero = countPhrase(locale, kind, 0);
    const one = countPhrase(locale, kind, 1);
    const many = countPhrase(locale, kind, 5);
    assert.match(zero, /^0 /);
    assert.match(one, /^1 /);
    assert.match(many, /^5 /);
    assert.notEqual(one, many);
  }
}
assert.equal(countPhrase("en", "person", 1), "1 person");
assert.equal(countPhrase("en", "event", 1), "1 event");
assert.equal(countPhrase("en", "reminder", 1), "1 reminder");
assert.equal(countPhrase("en", "alarm", 1), "1 alarm");
assert.equal(countPhrase("en", "countdown", 1), "1 countdown");
assert.equal(countPhrase("en", "account", 1), "1 account");
assert.equal(countPhrase("en", "key", 1), "1 key");
assert.equal(countPhrase("en", "fact", 1), "1 pinned fact");
assert.equal(countPhrase("en", "person", 0), "0 people");
assert.equal(countPhrase("es", "person", 1), "1 persona");
assert.equal(countPhrase("es", "person", 4), "4 personas");
assert.equal(countPhrase("de", "account", 1), "1 Konto");
assert.equal(countPhrase("de", "account", 2), "2 Konten");
assert.equal(countPhrase("pt", "key", 1), "1 chave");
assert.equal(countPhrase("ca-ES-valencia", "fact", 1), "1 dada fixada");
assert.equal(countPhrase("ca-ES-valencia", "fact", 3), "3 dades fixades");
assert.equal(new Intl.PluralRules("en").select(1), "one");
assert.equal(new Intl.PluralRules("ca").select(5), "other");

console.log("backupRoundTrip.test.ts ok");
