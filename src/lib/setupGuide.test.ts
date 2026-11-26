/// <reference types="node" />
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { recommend } from "./fit.ts";
import { recoveryFor } from "./flow.ts";
import {
  adviceFor,
  guideKeys,
  localModelCandidates,
  progressIndex,
  screenFor,
} from "./setupGuide.ts";

const GIB = 1024 * 1024 * 1024;
const locales = ["es", "de", "pt", "ca-ES-valencia"] as const;
const setup = JSON.parse(readFileSync(new URL("./setup-i18n.json", import.meta.url), "utf8")) as Record<string, Record<string, string>>;
const phonon = JSON.parse(readFileSync(new URL("./phonon-i18n.json", import.meta.url), "utf8")) as Record<string, Record<string, string>>;
const i18n = readFileSync(new URL("./i18n.ts", import.meta.url), "utf8");
const onboarding = readFileSync(new URL("../components/Onboarding.tsx", import.meta.url), "utf8");
const voiceHome = readFileSync(new URL("../components/VoiceHome.tsx", import.meta.url), "utf8");

function translatedSomewhere(key: string): boolean {
  const catalogs = [setup, phonon];
  if (catalogs.some((catalog) => locales.every((locale) => typeof catalog[locale]?.[key] === "string" && catalog[locale][key].trim() && catalog[locale][key] !== key))) {
    return true;
  }
  const quoted = i18n.split(`"${key}":`).length - 1;
  if (quoted >= 4) return true;
  if (/^[A-Za-z][A-Za-z0-9]*$/.test(key)) {
    const bare = i18n.split(`\n    ${key}:`).length - 1;
    if (bare >= 4) return true;
  }
  return false;
}

const keys = guideKeys();
assert.ok(keys.length > 40, "guide copy is missing");
for (const key of keys) {
  for (const locale of locales) {
    const text = setup[locale]?.[key];
    assert.equal(typeof text, "string", `${locale} missing ${key}`);
    assert.ok(text.trim(), `${locale} empty ${key}`);
    assert.notEqual(text, key, `${locale} left ${key} in English`);
  }
}

const literals = new Set<string>();
for (const source of [onboarding, voiceHome]) {
  for (const match of source.matchAll(/\b(?:t|fill)\(\s*"((?:\\.|[^"\\])*)"/g)) {
    literals.add(match[1].replace(/\\"/g, '"'));
  }
}
assert.ok(literals.has("Finish later"), "onboarding lost the skip action");
assert.ok(literals.has("Open the key page"), "key page link is missing");
for (const key of literals) {
  assert.equal(translatedSomewhere(key), true, `missing translations for ${key}`);
}

assert.equal(screenFor("packs-run"), "local");
assert.equal(screenFor("packs"), "local");
assert.equal(screenFor("preview"), "test");
assert.equal(screenFor("cloud"), "path");
assert.equal(screenFor("xai"), "xai");
assert.equal(screenFor("onemin"), "onemin");
assert.equal(screenFor("mixed"), "mixed");
assert.equal(screenFor(undefined), "language");
assert.equal(progressIndex("language"), 0);
assert.equal(progressIndex("path"), 1);
assert.equal(progressIndex("openai"), 2);
assert.equal(progressIndex("test"), 3);

const four = adviceFor(4 * GIB);
assert.equal(four.modelId, recommend(localModelCandidates(), 4 * GIB));
assert.ok(four.modelId);
assert.notEqual(four.modelId, "llm-qwen35-4b");
assert.equal(four.knownMemory, true);
assert.ok(four.storageBytes > 400_000_000);
const unknown = adviceFor(0);
assert.equal(unknown.modelId, null);
assert.equal(unknown.knownMemory, false);

const refused = recoveryFor("1min.ai returned 401");
assert.equal(refused?.go, "voice");
assert.equal(
  refused?.hint,
  "Check the key in Voice and AI. Create a new one if it was refused, then try Talk again.",
);
assert.equal(recoveryFor("no voice key yet")?.go, "voice");

const planted = "This string is not translated anywhere.";
assert.equal(translatedSomewhere(planted), false);

console.log("setupGuide.test.ts ok");
