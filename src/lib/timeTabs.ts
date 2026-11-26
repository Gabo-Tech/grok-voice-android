export const TIME_TABS = ["reminders", "alarms", "countdowns"] as const;
export type TimeTab = (typeof TIME_TABS)[number];

export type TimeRow = {
  id: number;
  kind: string;
  title: string;
  note: string;
  atMs: number;
};

/** Countdown and timer are one list. Anything else that is not an alarm is a reminder. */
export function tabForKind(kind: string): TimeTab {
  if (kind === "alarm") return "alarms";
  if (kind === "timer" || kind === "countdown") return "countdowns";
  return "reminders";
}

export function kindForTab(tab: TimeTab): "reminder" | "alarm" | "timer" {
  if (tab === "alarms") return "alarm";
  if (tab === "countdowns") return "timer";
  return "reminder";
}

export function addLabel(tab: TimeTab): string {
  if (tab === "alarms") return "Add alarm";
  if (tab === "countdowns") return "Add countdown";
  return "Add reminder";
}

export function tabLabel(tab: TimeTab): string {
  if (tab === "alarms") return "Alarms";
  if (tab === "countdowns") return "Countdowns";
  return "Reminders";
}

export function rowsForTab<T extends { kind: string }>(items: T[], tab: TimeTab): T[] {
  return items.filter((item) => tabForKind(item.kind) === tab);
}

/** Section keys include the subtab. Overdue/Today/Later alone are the same on every subtab. */
export function sectionKey(tab: TimeTab, name: string): string {
  return `${tab}:${name}`;
}

export function queryFor(queries: Partial<Record<TimeTab, string>>, tab: TimeTab): string {
  return queries[tab] ?? "";
}

export function groupTimeItems<T extends TimeRow>(items: T[], tab: TimeTab, query: string, now = Date.now()) {
  const needle = query.trim().toLowerCase();
  const endToday = new Date(now);
  endToday.setHours(24, 0, 0, 0);
  const filtered = rowsForTab(items, tab)
    .filter((item) => !needle || `${item.title} ${item.note}`.toLowerCase().includes(needle))
    .slice()
    .sort((a, b) => a.atMs - b.atMs);
  const overdue = filtered.filter((item) => item.atMs < now);
  const today = filtered.filter((item) => item.atMs >= now && item.atMs < endToday.getTime());
  const later = filtered.filter((item) => item.atMs >= endToday.getTime());
  const sections = [
    { name: "Overdue", items: overdue },
    { name: "Today", items: today },
    { name: "Later", items: later },
  ].filter((section) => section.items.length > 0);
  return { sections, overdue: overdue.length, today: today.length, later: later.length, total: filtered.length };
}

export type RepeatUnit = "days" | "weeks" | "months" | "years";

export function recurrenceValue(repeat: string, unit: RepeatUnit): string {
  if (repeat === "custom") {
    if (unit === "weeks") return "every_n_weeks";
    if (unit === "months") return "every_n_months";
    if (unit === "years") return "every_n_years";
    return "every_n_days";
  }
  if (repeat === "none" || repeat === "") return "none";
  return repeat;
}

export function repeatFrom(recurrence: string | undefined, interval: number | undefined): { repeat: string; unit: RepeatUnit; interval: number } {
  const every = Math.max(1, interval || 1);
  if (recurrence === "every_n_days") return { repeat: "custom", unit: "days", interval: every };
  if (recurrence === "every_n_weeks") return { repeat: "custom", unit: "weeks", interval: every };
  if (recurrence === "every_n_months") return { repeat: "custom", unit: "months", interval: every };
  if (recurrence === "every_n_years") return { repeat: "custom", unit: "years", interval: every };
  if (recurrence === "daily" || recurrence === "weekly" || recurrence === "monthly" || recurrence === "yearly" || recurrence === "once") {
    return { repeat: recurrence, unit: "days", interval: 1 };
  }
  return { repeat: "none", unit: "days", interval: 1 };
}

export function isRepeating(recurrence: string): boolean {
  return recurrence === "daily" || recurrence === "weekly" || recurrence === "monthly" || recurrence === "yearly" || recurrence.startsWith("every_n_");
}

export function previewTimeItems(now = Date.parse("2026-11-26T12:00:00")): TimeRow[] {
  return [
    { id: 1, kind: "reminder", title: "Call the clinic", note: "Bring the referral", atMs: now - 3_600_000 },
    { id: 2, kind: "alarm", title: "Wake up", note: "", atMs: now + 60 * 60_000 },
    { id: 3, kind: "timer", title: "Pasta", note: "", atMs: now + 10 * 60_000 },
    { id: 4, kind: "countdown", title: "Tea", note: "", atMs: now + 5 * 60_000 },
  ];
}
