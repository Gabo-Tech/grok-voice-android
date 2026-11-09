import { useMemo, useState } from "react";
import { Trash2 } from "lucide-react";
import { EmptyState, Notice, PageChrome } from "@/components/PageChrome";
import { ShareButton } from "@/components/ShareButton";
import { Sheet } from "@/components/Sheet";
import { Field } from "@/components/Field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import type { AgendaState } from "@/hooks/useAgenda";
import { formatClock, fromLocalInput, toLocalInput } from "@/hooks/useAgenda";
import { RepeatFields, endOfDay, untilInput, type RepeatChoice } from "@/components/RepeatFields";
import { api, type ScheduledItem } from "@/lib/api";
import { useT } from "@/lib/i18n";

const KINDS = ["reminder", "alarm", "timer"] as const;
const WEEKDAYS = [
  { id: 1, label: "Mon" },
  { id: 2, label: "Tue" },
  { id: 3, label: "Wed" },
  { id: 4, label: "Thu" },
  { id: 5, label: "Fri" },
  { id: 6, label: "Sat" },
  { id: 7, label: "Sun" },
] as const;

type Repeat = RepeatChoice | "once";

export function RemindersPage({ agenda, active = true }: { agenda: AgendaState; active?: boolean }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<ScheduledItem | null>(null);
  const [title, setTitle] = useState("");
  const [at, setAt] = useState(() => toLocalInput(Date.now() + 3_600_000));
  const [minutes, setMinutes] = useState(10);
  const [note, setNote] = useState("");
  const [kind, setKind] = useState<(typeof KINDS)[number]>("reminder");
  const [kindFilter, setKindFilter] = useState<"all" | (typeof KINDS)[number]>("all");
  const [query, setQuery] = useState("");
  const [repeat, setRepeat] = useState<Repeat>("none");
  const [weekdays, setWeekdays] = useState<number[]>([]);
  const [clock, setClock] = useState("09:00");
  const [date, setDate] = useState("");
  const [leadDays, setLeadDays] = useState(0);
  const [personId, setPersonId] = useState("");
  const [until, setUntil] = useState("");

  const items = agenda.agenda.reminders;

  const openNew = (next: (typeof KINDS)[number] = "reminder") => {
    setEditing(null);
    setTitle("");
    setKind(next);
    setAt(toLocalInput(Date.now() + 3_600_000));
    setMinutes(next === "timer" ? 10 : 60);
    setNote("");
    setRepeat("none");
    setWeekdays([]);
    setClock("09:00");
    setDate("");
    setLeadDays(0);
    setPersonId("");
    setUntil("");
    setOpen(true);
  };

  const openEdit = (item: ScheduledItem) => {
    setEditing(item);
    setTitle(item.title);
    setKind(KINDS.includes(item.kind as (typeof KINDS)[number]) ? (item.kind as (typeof KINDS)[number]) : "reminder");
    setAt(toLocalInput(item.atMs));
    setMinutes(Math.max(1, Math.round((item.atMs - Date.now()) / 60_000)));
    setNote(item.note || "");
    const nextRepeat = (item.recurrence || "none") as Repeat;
    setRepeat(
      nextRepeat === "none" || nextRepeat === "daily" || nextRepeat === "weekly" || nextRepeat === "monthly" || nextRepeat === "yearly" || nextRepeat === "once"
        ? nextRepeat
        : "none",
    );
    setWeekdays(item.weekdays ?? []);
    setClock(item.time || clockFrom(item.atMs));
    setDate(item.date || "");
    setLeadDays(item.leadDays ?? 0);
    setPersonId(item.personId || "");
    setUntil(untilInput(item.untilMs));
    setOpen(true);
  };

  const close = () => {
    setOpen(false);
    setEditing(null);
  };

  const whenMs = () => (kind === "timer" ? Date.now() + Math.max(1, minutes) * 60_000 : fromLocalInput(at));
  const linked = Boolean(personId);
  const repeating = kind === "reminder" && (repeat === "daily" || repeat === "weekly" || repeat === "monthly" || repeat === "yearly" || linked);

  const save = () => {
    const next = title.trim();
    if (!next) return;
    void agenda.run(async () => {
      const snap = await api.saveScheduled({
        title: next,
        atMs: repeating ? Date.now() + 120_000 : whenMs(),
        note: note.trim(),
        kind,
        id: editing?.id,
        recurrence: kind === "reminder" ? (repeat === "none" ? "none" : repeat) : "",
        weekdays: repeat === "weekly" ? weekdays : [],
        time: repeating ? clock : "",
        date: linked || repeat === "yearly" || repeat === "once" || repeat === "monthly" ? date : "",
        leadDays: linked || repeat === "yearly" ? leadDays : 0,
        untilMs: repeating && until ? endOfDay(until) : 0,
      });
      close();
      return snap;
    });
  };

  const remove = (id: number) => {
    void agenda.run(async () => {
      const snap = await api.cancelScheduled(id);
      if (editing?.id === id) close();
      return snap;
    });
  };

  const finish = (id: number) => {
    void agenda.run(async () => api.completeScheduled(id));
  };

  const grouped = useMemo(
    () => groupItems(items.filter((item) => kindFilter === "all" || item.kind === kindFilter), query),
    [items, query, kindFilter],
  );
  const subtitle = waitingLine(grouped);

  return (
    <>
      <PageChrome title={t("Time")} subtitle={subtitle} onAdd={() => openNew("reminder")} addLabel={t("New reminder")} active={active}>
        <div className="mb-4 grid grid-cols-2 gap-2">
          <button type="button" className="h-12 border border-border text-sm" onClick={() => openNew("alarm")}>
            {t("New alarm")}
          </button>
          <button type="button" className="h-12 border border-border text-sm" onClick={() => openNew("timer")}>
            {t("New countdown")}
          </button>
        </div>
        <div className="mb-4 flex gap-2 overflow-x-auto">
          {(["all", ...KINDS] as const).map((item) => (
            <button
              key={item}
              type="button"
              onClick={() => setKindFilter(item)}
              className={
                kindFilter === item
                  ? "h-12 shrink-0 border border-foreground px-3 text-sm text-foreground"
                  : "h-12 shrink-0 border border-border px-3 text-sm text-muted-foreground"
              }
            >
              {item === "all" ? "All" : item === "timer" ? "Countdowns" : item === "alarm" ? "Alarms" : "Reminders"}
            </button>
          ))}
        </div>
        {(items.length > 0 || query) && (
        <div className="mb-6">
          <Field label="Search">
            <div className="relative">
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="A title or a note"
                aria-label="Search reminders"
                className="pr-16"
              />
              {query && (
                <button
                  type="button"
                  className="absolute top-0 right-0 h-12 px-3 text-sm text-foreground"
                  onClick={() => setQuery("")}
                >
                  Clear
                </button>
              )}
            </div>
          </Field>
        </div>
        )}
        {agenda.error && !open && <Notice title={agenda.error} />}
        {grouped.total === 0 ? (
          <EmptyState
            title={query ? "No matches" : "Nothing scheduled"}
            body={query ? "Try another word." : "Add an alarm, a countdown, or a reminder."}
            actionLabel={query ? "Clear search" : "New reminder"}
            onAction={query ? () => setQuery("") : () => openNew("reminder")}
          />
        ) : (
          <div className="space-y-6">
            {grouped.sections.map((section) => (
              <section key={section.name}>
                <h2 className="mb-2 px-1 text-sm text-muted-foreground">{section.name}</h2>
                <div className="divide-y divide-border border-y border-border">
                  {section.items.map((item) => (
                    <div key={item.id} className="flex items-start gap-3 px-1 py-3.5">
                      <button
                        type="button"
                        aria-label={`Mark ${item.title} done`}
                        className="grid size-12 shrink-0 place-items-center"
                        onClick={() => finish(item.id)}
                      >
                        <span className="size-5 rounded-full border border-foreground" />
                      </button>
                      <button type="button" className="flex min-w-0 flex-1 items-start gap-4 text-left" onClick={() => openEdit(item)}>
                        <div className="w-16 shrink-0 pt-0.5 text-right">
                          <p className="text-sm leading-5 tabular-nums text-foreground">{formatClock(item.atMs)}</p>
                          {section.name === "Later" && (
                            <p className="text-xs leading-4 text-muted-foreground">
                              {new Date(item.atMs).toLocaleDateString(undefined, { month: "short", day: "numeric" })}
                            </p>
                          )}
                        </div>
                        <div className="min-w-0 flex-1 border-l border-border pl-4">
                          <p className="text-xs text-muted-foreground">
                            {[item.kind, patternLabel(item)].filter(Boolean).join(" · ")}
                          </p>
                          <p className="truncate text-base leading-6 text-foreground">{item.title}</p>
                          {item.note ? (
                            <p className="mt-1 truncate text-sm leading-5 text-muted-foreground">{item.note}</p>
                          ) : null}
                        </div>
                      </button>
                    </div>
                  ))}
                </div>
              </section>
            ))}
          </div>
        )}
      </PageChrome>

      <Sheet
        open={open}
        title={editing ? `Edit ${kind}` : `New ${kind}`}
        onClose={close}
        footer={
          <div className="flex flex-wrap items-center gap-2">
            <ShareButton title={title} text={[kind, repeating ? `${clock} ${date}`.trim() : kind === "timer" ? `${minutes} min` : at, note].filter(Boolean).join("\n")} />
            {editing && (
              <Button variant="ghost" disabled={agenda.busy} onClick={() => remove(editing.id)}>
                <Trash2 /> Delete
              </Button>
            )}
            <div className="flex-1" />
            <Button variant="ghost" onClick={close}>
              Cancel
            </Button>
            <Button disabled={agenda.busy || !title.trim()} onClick={save}>
              Save
            </Button>
          </div>
        }
      >
        <div className="space-y-5">
          <div className="grid grid-cols-3 gap-2">
            {KINDS.map((item) => (
              <button
                key={item}
                type="button"
                onClick={() => setKind(item)}
                className={
                  item === kind
                    ? "h-12 border border-foreground text-sm text-foreground"
                    : "h-12 border border-border text-sm text-muted-foreground"
                }
              >
                {item}
              </button>
            ))}
          </div>
          <Field label="Title" error={open ? agenda.error : null}>
            <Input
              autoFocus
              value={title}
              aria-invalid={Boolean(agenda.error) || undefined}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Call the clinic"
            />
          </Field>
          {kind === "reminder" && !linked && (
            <RepeatFields
              repeat={repeat === "once" ? "none" : repeat}
              weekdays={weekdays}
              until={until}
              onRepeat={setRepeat}
              onWeekdays={setWeekdays}
              onUntil={setUntil}
            />
          )}
          {kind === "reminder" && linked && (
            <div>
              <p className="mb-2 text-sm text-muted-foreground">{t("Repeats")}</p>
              <div className="grid grid-cols-2 gap-2">
                {(["yearly", "once"] as const).map((item) => (
                  <button
                    key={item}
                    type="button"
                    aria-pressed={repeat === item}
                    onClick={() => setRepeat(item)}
                    className={
                      repeat === item
                        ? "h-14 border-2 border-foreground text-sm font-medium text-foreground"
                        : "h-14 border border-border text-sm text-muted-foreground"
                    }
                  >
                    {item === "once" ? t("Just once") : t("Every year")}
                  </button>
                ))}
              </div>
              <p className="mt-2 text-sm leading-5 text-muted-foreground">This date belongs to someone in People.</p>
            </div>
          )}
          {kind === "timer" ? (
            <Field label="Length" hint="Counts from the moment you save.">
              <div className="mb-3 flex gap-2">
                {[5, 10, 25].map((value) => (
                  <button
                    key={value}
                    type="button"
                    onClick={() => setMinutes(value)}
                    className={
                      minutes === value
                        ? "h-12 flex-1 border border-foreground text-sm text-foreground"
                        : "h-12 flex-1 border border-border text-sm text-muted-foreground"
                    }
                  >
                    {value} min
                  </button>
                ))}
              </div>
              <Input
                type="number"
                min={1}
                inputMode="numeric"
                value={minutes}
                onChange={(e) => setMinutes(Math.max(1, Number(e.target.value) || 1))}
              />
            </Field>
          ) : repeating ? (
            <div className="space-y-4">
              {(linked || repeat === "yearly" || repeat === "once" || repeat === "monthly") && (
                <Field label={t("Date")}>
                  <Input type="date" aria-label={t("Date")} value={asDateInput(date)} onChange={(e) => setDate(e.target.value)} />
                </Field>
              )}
              <Field label="Time" hint="Next notification uses this clock.">
                <Input type="time" value={clock} onChange={(e) => setClock(e.target.value)} />
              </Field>
              {(linked || repeat === "yearly") && (
                <Field label="Early">
                  <div className="flex gap-2">
                    {[0, 1, 3, 7].map((days) => (
                      <button
                        key={days}
                        type="button"
                        onClick={() => setLeadDays(days)}
                        className={
                          leadDays === days
                            ? "h-12 flex-1 border border-foreground text-sm text-foreground"
                            : "h-12 flex-1 border border-border text-sm text-muted-foreground"
                        }
                      >
                        {days === 0 ? "Day of" : `${days}d`}
                      </button>
                    ))}
                  </div>
                </Field>
              )}
            </div>
          ) : (
            <Field label="When">
              <div className="mb-3 flex flex-col gap-2">
                {quickTimes().map((choice) => (
                  <button
                    key={choice.label}
                    type="button"
                    onClick={() => setAt(toLocalInput(choice.ms))}
                    className={
                      at === toLocalInput(choice.ms)
                        ? "h-12 border border-foreground px-3 text-left text-sm text-foreground"
                        : "h-12 border border-border px-3 text-left text-sm text-muted-foreground"
                    }
                  >
                    {choice.label}
                  </button>
                ))}
              </div>
              <Input type="datetime-local" value={at} onChange={(e) => setAt(e.target.value)} />
            </Field>
          )}
          <Field label="Note">
            <Textarea value={note} onChange={(e) => setNote(e.target.value)} placeholder="Bring the referral" className="min-h-28" />
          </Field>
        </div>
      </Sheet>
    </>
  );
}

function quickTimes(): Array<{ label: string; ms: number }> {
  const now = new Date();
  const tonight = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 20, 0, 0, 0);
  if (tonight.getTime() <= now.getTime()) tonight.setDate(tonight.getDate() + 1);
  const morning = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 8, 0, 0, 0);
  return [
    { label: "In 1 hour", ms: now.getTime() + 3_600_000 },
    { label: "Tonight", ms: tonight.getTime() },
    { label: "Tomorrow morning", ms: morning.getTime() },
  ];
}

function groupItems(items: ScheduledItem[], query: string) {
  const needle = query.trim().toLowerCase();
  const now = Date.now();
  const endToday = new Date();
  endToday.setHours(24, 0, 0, 0);
  const filtered = items
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

function asDateInput(value: string): string {
  if (/^\d{4}-\d{2}-\d{2}/.test(value)) return value.slice(0, 10);
  const md = /^(\d{2})-(\d{2})$/.exec(value);
  if (!md) return "";
  return `${new Date().getFullYear()}-${md[1]}-${md[2]}`;
}

function clockFrom(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function patternLabel(item: ScheduledItem): string {
  const days = (item.weekdays ?? []).map((day) => WEEKDAYS.find((item) => item.id === day)?.label).filter(Boolean).join(" ");
  if (item.recurrence === "daily") return "Daily";
  if (item.recurrence === "weekly") return days ? `Weekly ${days}` : "Weekly";
  if (item.recurrence === "monthly") return "Monthly";
  if (item.recurrence === "yearly") return item.date ? `Yearly ${item.date}` : "Yearly";
  if (item.recurrence === "once" && item.date) return `Once ${item.date}`;
  return "";
}

function waitingLine(grouped: ReturnType<typeof groupItems>): string {
  const parts: string[] = [];
  if (grouped.overdue) parts.push(`${grouped.overdue} overdue`);
  if (grouped.today) parts.push(`${grouped.today} today`);
  if (!grouped.overdue && !grouped.today && grouped.later) parts.push(`${grouped.later} later`);
  return parts.length ? parts.join(" · ") : "None waiting";
}
