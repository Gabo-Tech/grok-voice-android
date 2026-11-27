/// <reference types="node" />
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const locales = ["es", "de", "pt", "ca-ES-valencia"] as const;
const root = new URL("../../", import.meta.url);

function read(path: string) {
  return readFileSync(new URL(path, root), "utf8");
}

function parseStringMap(source: string) {
  const map = new Map<string, string>();
  let i = 0;
  const n = source.length;
  const skipWs = () => {
    while (i < n && /\s/.test(source[i] ?? "")) i += 1;
  };
  const readString = () => {
    const quote = source[i];
    if (quote !== '"' && quote !== "'") return null;
    i += 1;
    let out = "";
    while (i < n) {
      const char = source[i];
      i += 1;
      if (char === "\\") {
        const next = source[i] ?? "";
        i += 1;
        out += next === "n" ? "\n" : next === "t" ? "\t" : next;
        continue;
      }
      if (char === quote) return out;
      out += char;
    }
    return null;
  };
  while (i < n) {
    skipWs();
    if (source[i] === "/" && source[i + 1] === "/") {
      while (i < n && source[i] !== "\n") i += 1;
      continue;
    }
    let key: string | null = null;
    if (source[i] === '"' || source[i] === "'") key = readString();
    else if (/[A-Za-z_]/.test(source[i] ?? "")) {
      const start = i;
      while (i < n && /[A-Za-z0-9_]/.test(source[i] ?? "")) i += 1;
      key = source.slice(start, i);
    } else {
      i += 1;
      continue;
    }
    skipWs();
    if (source[i] !== ":") continue;
    i += 1;
    skipWs();
    const value = readString();
    if (key && value != null) map.set(key, value);
  }
  return map;
}

function extractBlock(source: string, marker: string) {
  const at = source.indexOf(marker);
  assert.ok(at >= 0, marker);
  const brace = source.indexOf("{", at);
  let depth = 0;
  for (let i = brace; i < source.length; i += 1) {
    if (source[i] === "{") depth += 1;
    else if (source[i] === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(brace + 1, i);
    }
  }
  throw new Error(`unclosed ${marker}`);
}

function localeMarker(locale: string) {
  return locale === "ca-ES-valencia" ? '"ca-ES-valencia": {' : `${locale}: {`;
}

const i18n = read("src/lib/i18n.ts");
const finance = read("src/lib/financeCopy.ts");
const setup = JSON.parse(read("src/lib/setup-i18n.json")) as Record<string, Record<string, string>>;
const phonon = JSON.parse(read("src/lib/phonon-i18n.json")) as Record<string, Record<string, string>>;

function catalog(locale: (typeof locales)[number]) {
  const map = parseStringMap(extractBlock(i18n, localeMarker(locale)));
  for (const [key, value] of parseStringMap(extractBlock(finance, localeMarker(locale)))) map.set(key, value);
  for (const [key, value] of Object.entries(setup[locale] ?? {})) map.set(key, value);
  for (const [key, value] of Object.entries(phonon[locale] ?? {})) map.set(key, value);
  return map;
}

const catalogs = Object.fromEntries(locales.map((locale) => [locale, catalog(locale)])) as Record<(typeof locales)[number], Map<string, string>>;
const baseKeys = [...catalogs.es.keys()].sort();
for (const locale of locales) {
  const keys = [...catalogs[locale].keys()].sort();
  assert.deepEqual(keys, baseKeys, `${locale} catalog keys differ`);
  for (const [key, value] of catalogs[locale]) {
    assert.ok(value.trim(), `${locale} empty ${key}`);
  }
}

/** Same spelling in that language, or a name that must stay. A new English leftover fails this test. */
const sameSpelling = new Set([
  "grok",
  "Moonshine",
  "Phonon",
  "Piper",
  "Piper Lessac",
  "Piper Davefx",
  "Piper Thorsten",
  "gpt-realtime",
  "SmolLM2",
  "NeMo",
  "Grok",
  "Grok Voice",
  "Hey Grok",
  "WhatsApp",
  "Signal",
  "Musify",
  "Auto",
  "Llama 3",
  "Llama 3.2",
  "Normal",
  "Pause",
  "Name",
  "Mo",
  "Fr",
  "Sa",
  "Ana Lopez",
  "Person",
  "A–Z",
  "{n} min",
  "{minutes} min · {count} {unit}",
  "SHA-256",
  "Phase",
  "WebSocket",
  "YouTube",
  "YouTube Music",
  "Bank",
  "System",
  "Sand",
  "1 {code} =",
  "1 {code} = {rate} {quote}",
  "Plan",
  "1min.ai",
  "Qwen3.5 0.8B",
  "Qwen3.5 2B",
  "Qwen3.5 4B",
  "Gemma 4 E2B",
  "Apache-2.0",
  "Gemma",
  "LFM1.0",
  "Notes",
  "Model",
  "sessions",
  "Dates",
  "Sessions",
  "visible",
  "Irregular",
  "Net",
  "Categories",
  "Color",
  "Accent",
  "Gram",
  "Colors",
  "Manual",
  "Metal",
  "Panel",
  "3M",
  "6M",
  "Total",
  "Details",
  "Palette",
  "Symbol",
  "Rest",
  "Budget",
  "Agenda",
]);

const leftovers: string[] = [];
for (const locale of locales) {
  for (const [key, value] of catalogs[locale]) {
    if (value === key && !sameSpelling.has(key)) leftovers.push(`${locale} ${JSON.stringify(key)}`);
  }
}
assert.deepEqual(leftovers, [], leftovers.slice(0, 40).join("\n"));

function walk(dir: string, out: string[] = []) {
  for (const name of readdirSync(dir)) {
    if (name === "node_modules") continue;
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path, out);
    else if (/\.(tsx|ts)$/.test(name) && !name.endsWith(".test.ts")) out.push(path);
  }
  return out;
}

const literals = new Set<string>();
const call = /\b(?:t|fill)\(\s*"((?:\\.|[^"\\])*)"/g;
for (const file of walk(new URL("src", root).pathname)) {
  if (file.endsWith("i18n.ts") || file.endsWith("financeCopy.ts")) continue;
  const text = readFileSync(file, "utf8");
  for (const match of text.matchAll(call)) {
    literals.add(match[1].replace(/\\n/g, "\n").replace(/\\"/g, '"'));
  }
}
assert.ok(literals.size > 500, "the UI string scan missed the components");
for (const key of literals) {
  for (const locale of locales) {
    const value = catalogs[locale].get(key);
    assert.equal(typeof value, "string", `${locale} missing ${JSON.stringify(key)}`);
    assert.ok(value?.trim(), `${locale} empty ${JSON.stringify(key)}`);
  }
}
assert.equal(catalogs.es.has("This string is not translated anywhere."), false);

function xmlStrings(path: string) {
  const xml = read(path);
  const map = new Map<string, string>();
  for (const match of xml.matchAll(/<string name="([^"]+)">([\s\S]*?)<\/string>/g)) {
    map.set(match[1], match[2].replace(/\\'/g, "'").replace(/^"|"$/g, ""));
  }
  return map;
}

const xmlLocales = {
  en: xmlStrings("src-tauri/gen/android/app/src/main/res/values/strings.xml"),
  es: xmlStrings("src-tauri/gen/android/app/src/main/res/values-es/strings.xml"),
  de: xmlStrings("src-tauri/gen/android/app/src/main/res/values-de/strings.xml"),
  pt: xmlStrings("src-tauri/gen/android/app/src/main/res/values-pt/strings.xml"),
  "ca-ES-valencia": xmlStrings("src-tauri/gen/android/app/src/main/res/values-b+ca+ES+valencia/strings.xml"),
};
const xmlKeys = [...xmlLocales.en.keys()].sort();
const xmlSame = new Set(["Grok Voice", "Grok", "Hey Grok", "WhatsApp", "Signal", "YouTube", "Pause", "Budget"]);
for (const [locale, map] of Object.entries(xmlLocales)) {
  assert.deepEqual([...map.keys()].sort(), xmlKeys, `${locale} Android strings differ`);
  for (const [key, value] of map) {
    assert.ok(value.trim(), `${locale} empty Android string ${key}`);
    if (locale !== "en" && value === xmlLocales.en.get(key) && !xmlSame.has(value)) {
      assert.fail(`${locale} Android string ${key} is still English`);
    }
  }
}

function rustTable(source: string, name: string) {
  const start = source.indexOf(`const ${name}:`);
  assert.ok(start >= 0, name);
  const next = source.indexOf("\nconst ", start + 8);
  const body = source.slice(start, next < 0 ? source.length : next);
  const map = new Map<string, string>();
  for (const match of body.matchAll(/^\s+\("([^"]+)", "", "((?:\\.|[^"])*)"\)/gm)) map.set(match[1], match[2]);
  return map;
}

const rust = read("src-tauri/src/i18n.rs");
const english = rustTable(rust, "EN");
assert.ok(english.size > 100, "spoken English table was not read");
for (const name of ["ES", "DE", "PT", "CA"]) {
  const table = rustTable(rust, name);
  assert.deepEqual([...table.keys()].sort(), [...english.keys()].sort(), `${name} spoken keys differ`);
  for (const [key, value] of table) {
    assert.ok(value.trim(), `${name} empty spoken ${key}`);
    assert.notEqual(value, english.get(key), `${name} spoken ${key} is still English`);
    assert.notEqual(value, key, `${name} spoken ${key} is the key`);
  }
}

console.log("i18nCoverage.test.ts ok");
