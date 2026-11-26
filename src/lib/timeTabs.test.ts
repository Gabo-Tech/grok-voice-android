/// <reference types="node" />
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  TIME_TABS,
  addLabel,
  groupTimeItems,
  kindForTab,
  previewTimeItems,
  queryFor,
  rowsForTab,
  sectionKey,
  tabForKind,
} from "./timeTabs.ts";

const now = Date.parse("2026-11-26T12:00:00");
const items = previewTimeItems(now);

assert.deepEqual(rowsForTab(items, "reminders").map((item) => item.id), [1]);
assert.deepEqual(rowsForTab(items, "alarms").map((item) => item.id), [2]);
assert.deepEqual(rowsForTab(items, "countdowns").map((item) => item.id), [3, 4]);
assert.equal(tabForKind("countdown"), "countdowns");
assert.equal(tabForKind(""), "reminders");
assert.equal(kindForTab("countdowns"), "timer");

const reminderQuery = { reminders: "clinic", alarms: "", countdowns: "" };
assert.equal(queryFor(reminderQuery, "reminders"), "clinic");
assert.equal(queryFor(reminderQuery, "alarms"), "");
const alarms = groupTimeItems(items, "alarms", queryFor(reminderQuery, "alarms"), now);
assert.deepEqual(alarms.sections.flatMap((section) => section.items.map((item) => item.kind)), ["alarm"]);
const reminders = groupTimeItems(items, "reminders", queryFor(reminderQuery, "reminders"), now);
assert.deepEqual(reminders.sections.flatMap((section) => section.items.map((item) => item.id)), [1]);
assert.equal(groupTimeItems(items, "alarms", "clinic", now).total, 0);

for (const tab of TIME_TABS) {
  const rows = groupTimeItems(items, tab, "", now);
  for (const section of rows.sections) {
    assert.equal(sectionKey(tab, section.name).startsWith(`${tab}:`), true);
    for (const item of section.items) {
      assert.equal(tabForKind(item.kind), tab, `${item.title} leaked into ${tab}`);
    }
  }
}
assert.notEqual(sectionKey("reminders", "Overdue"), sectionKey("alarms", "Overdue"));
assert.equal(addLabel("reminders"), "Add reminder");
assert.equal(addLabel("alarms"), "Add alarm");
assert.equal(addLabel("countdowns"), "Add countdown");

const page = readFileSync(new URL("../components/RemindersPage.tsx", import.meta.url), "utf8");
assert.equal(page.includes("key={section.name}"), false);
assert.ok(page.includes("sectionKey("));
assert.ok(page.includes("groupTimeItems("));
assert.ok(page.includes("queryFor("));
assert.ok(page.includes('key={tab}'));
for (const tab of TIME_TABS) {
  assert.ok(page.includes(`data-time-tabpanel={tab}`) || page.includes("data-time-tabpanel"), tab);
}
assert.ok(page.includes('t(addLabel(tab))') || page.includes("addLabel(tab)"));

const copy = readFileSync(new URL("./i18n.ts", import.meta.url), "utf8");
for (const key of [
  "Add reminder",
  "Add alarm",
  "Add countdown",
  "Repeat every",
  "Days",
  "Weeks",
  "Months",
  "Years",
  "Stop after",
  "How many times",
  "Spoken alert",
  "Same as Settings",
  "Speak this one",
  "Stay quiet",
  "Delete this reminder?",
  "Delete this alarm?",
  "Delete this countdown?",
  "Tap Delete again.",
  "Every {n} days",
  "Every {n} weeks",
  "Every {n} months",
  "Every {n} years",
  "Ends after {n}",
  "Add a reminder.",
  "Add an alarm.",
  "Add a countdown.",
]) {
  const hits = copy.split(`"${key}"`).length - 1 + copy.split(`${key}:`).length - 1;
  assert.ok(hits >= 4, key);
}

console.log("time tabs ok");
