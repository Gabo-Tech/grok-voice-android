import { useMemo, useState } from "react";
import { Trash2 } from "lucide-react";
import { EmptyState, PageChrome } from "@/components/PageChrome";
import { Sheet } from "@/components/Sheet";
import { Field } from "@/components/Field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import type { AgendaState } from "@/hooks/useAgenda";
import { formatClock, fromLocalInput, toLocalInput } from "@/hooks/useAgenda";
import { api, type ScheduledItem } from "@/lib/api";

const KINDS = ["reminder", "alarm", "timer"] as const;

export function RemindersPage({ agenda }: { agenda: AgendaState }) {
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<ScheduledItem | null>(null);
  const [title, setTitle] = useState("");
  const [at, setAt] = useState(() => toLocalInput(Date.now() + 3_600_000));
  const [minutes, setMinutes] = useState(10);
  const [note, setNote] = useState("");
  const [kind, setKind] = useState<(typeof KINDS)[number]>("reminder");
  const [kindFilter, setKindFilter] = useState<"all" | (typeof KINDS)[number]>("all");
  const [query, setQuery] = useState("");

  const items = agenda.agenda.reminders;

  const openNew = (next: (typeof KINDS)[number] = "reminder") => {
    setEditing(null);
    setTitle("");
    setKind(next);
    setAt(toLocalInput(Date.now() + 3_600_000));
    setMinutes(next === "timer" ? 10 : 60);
    setNote("");
    setOpen(true);
  };

  const openEdit = (item: ScheduledItem) => {
    setEditing(item);
    setTitle(item.title);
    setKind(KINDS.includes(item.kind as (typeof KINDS)[number]) ? (item.kind as (typeof KINDS)[number]) : "reminder");
    setAt(toLocalInput(item.atMs));
    setMinutes(Math.max(1, Math.round((item.atMs - Date.now()) / 60_000)));
    setNote(item.note || "");
    setOpen(true);
  };

  const close = () => {
    setOpen(false);
    setEditing(null);
  };

  const whenMs = () => (kind === "timer" ? Date.now() + Math.max(1, minutes) * 60_000 : fromLocalInput(at));

  const save = () => {
    const next = title.trim();
    if (!next) return;
    void agenda.run(async () => {
      const snap = await api.saveScheduled({
        title: next,
        atMs: whenMs(),
        note: note.trim(),
        kind,
        id: editing?.id,
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

  const grouped = useMemo(
    () => groupItems(items.filter((item) => kindFilter === "all" || item.kind === kindFilter), query),
    [items, query, kindFilter],
  );
  const subtitle = waitingLine(grouped);

  return (
    <>
      <PageChrome title="Time" subtitle={subtitle} onAdd={() => openNew("reminder")} addLabel="New reminder">
        <div className="mb-4 grid grid-cols-2 gap-2">
          <button type="button" className="h-12 border border-border text-sm" onClick={() => openNew("alarm")}>
            New alarm
          </button>
          <button type="button" className="h-12 border border-border text-sm" onClick={() => openNew("timer")}>
            New countdown
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
                  ? "h-10 shrink-0 border border-foreground px-3 text-xs tracking-[0.08em] text-foreground uppercase"
                  : "h-10 shrink-0 border border-border px-3 text-xs tracking-[0.08em] text-muted-foreground uppercase"
              }
            >
              {item === "timer" ? "countdown" : item}
            </button>
          ))}
        </div>
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
                  className="absolute top-0 right-0 h-12 px-3 text-xs tracking-[0.12em] text-foreground uppercase"
                  onClick={() => setQuery("")}
                >
                  Clear
                </button>
              )}
            </div>
          </Field>
        </div>
        {agenda.error && !open && <p className="mb-3 px-1 text-sm text-foreground">{agenda.error}</p>}
        {grouped.total === 0 ? (
          <EmptyState
            title={query ? "No matches" : "Nothing scheduled"}
            body={query ? "Try another word." : "Add an alarm, a countdown, or a reminder."}
          />
        ) : (
          <div className="space-y-6">
            {grouped.sections.map((section) => (
              <section key={section.name}>
                <h2 className="mb-2 px-1 text-[11px] tracking-[0.16em] text-muted-foreground uppercase">{section.name}</h2>
                <div className="divide-y divide-border border-y border-border">
                  {section.items.map((item) => (
                    <div key={item.id} className="flex items-start gap-3 px-1 py-3.5">
                      <button
                        type="button"
                        aria-label={`Mark ${item.title} done`}
                        className="mt-1 size-5 shrink-0 rounded-full border border-foreground/70"
                        onClick={() => remove(item.id)}
                      />
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
                          <p className="text-[10px] tracking-[0.16em] text-muted-foreground uppercase">{item.kind}</p>
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
                    ? "h-12 border border-foreground text-xs tracking-[0.12em] text-foreground uppercase"
                    : "h-12 border border-border text-xs tracking-[0.12em] text-muted-foreground uppercase"
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

function waitingLine(grouped: ReturnType<typeof groupItems>): string {
  const parts: string[] = [];
  if (grouped.overdue) parts.push(`${grouped.overdue} overdue`);
  if (grouped.today) parts.push(`${grouped.today} today`);
  if (!grouped.overdue && !grouped.today && grouped.later) parts.push(`${grouped.later} later`);
  return parts.length ? parts.join(" · ") : "None waiting";
}
