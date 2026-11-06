import { useEffect, useMemo, useState } from "react";
import { Trash2 } from "lucide-react";
import { EmptyState, Notice, PageChrome, RowButton } from "@/components/PageChrome";
import { ShareButton } from "@/components/ShareButton";
import { Sheet } from "@/components/Sheet";
import { Field } from "@/components/Field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { AgendaState } from "@/hooks/useAgenda";
import { formatClock, formatDay, formatSpan, fromLocalInput, toLocalInput } from "@/hooks/useAgenda";
import { api, type CalEvent } from "@/lib/api";
import { useT } from "@/lib/i18n";

export function CalendarPage({ agenda }: { agenda: AgendaState }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<CalEvent | null>(null);
  const [title, setTitle] = useState("");
  const [start, setStart] = useState(() => toLocalInput(Date.now() + 3_600_000));
  const [end, setEnd] = useState(() => toLocalInput(Date.now() + 7_200_000));
  const [location, setLocation] = useState("");

  const events = useMemo(
    () => [...agenda.agenda.events].sort((a, b) => a.startMs - b.startMs),
    [agenda.agenda.events],
  );
  const [cursor, setCursor] = useState(() => new Date());
  const [selected, setSelected] = useState(() => startOfDay(new Date()));
  const [monthEvents, setMonthEvents] = useState<CalEvent[]>([]);
  const [monthCalendar, setMonthCalendar] = useState(agenda.agenda.calendar);

  const year = cursor.getFullYear();
  const month = cursor.getMonth();
  const cells = useMemo(() => monthCells(year, month), [year, month]);

  useEffect(() => {
    if (!agenda.inApp) return;
    const from = new Date(year, month, 1).getTime();
    const to = new Date(year, month + 1, 1).getTime();
    let cancel = false;
    void api.monthEvents(from, to).then(
      (view) => {
        if (cancel) return;
        setMonthEvents(view.events);
        setMonthCalendar(view.calendar);
      },
      () => {
        if (!cancel) setMonthEvents([]);
      },
    );
    return () => {
      cancel = true;
    };
  }, [agenda.inApp, agenda.agenda, year, month]);

  const selectedEvents = useMemo(
    () => monthEvents.filter((event) => overlapsDay(event, selected)),
    [monthEvents, selected],
  );

  const openNew = () => {
    setEditing(null);
    setTitle("");
    const base = new Date(selected);
    base.setHours(new Date().getHours() + 1, 0, 0, 0);
    setStart(toLocalInput(base.getTime()));
    setEnd(toLocalInput(base.getTime() + 3_600_000));
    setLocation("");
    setOpen(true);
  };

  const openEdit = (event: CalEvent) => {
    setEditing(event);
    setTitle(event.title);
    setStart(toLocalInput(event.startMs));
    setEnd(toLocalInput(event.endMs || event.startMs + 3_600_000));
    setLocation(event.location || "");
    setOpen(true);
  };

  const close = () => {
    setOpen(false);
    setEditing(null);
  };

  const save = () => {
    const next = title.trim();
    if (!next) return;
    const startMs = fromLocalInput(start);
    const endMs = Math.max(fromLocalInput(end), startMs + 60_000);
    void agenda.run(async () => {
      const snap = await api.saveEvent({
        title: next,
        startMs,
        endMs,
        location: location.trim(),
        id: editing?.id,
      });
      close();
      return snap;
    });
  };

  const remove = () => {
    if (!editing?.id) return;
    void agenda.run(async () => {
      const snap = await api.deleteEvent(editing.id);
      close();
      return snap;
    });
  };

  return (
    <>
      <PageChrome title={t("Calendar")} subtitle={formatDay()} onAdd={openNew} addLabel={t("New event")}>
        {monthCalendar !== "granted" && (
          <Button
            variant="outline"
            className="mb-4 w-full"
            disabled={agenda.busy || !agenda.inApp}
            onClick={() => void agenda.requestCalendar()}
          >
            Allow calendar access
          </Button>
        )}
        {monthCalendar !== "granted" && (
          <p className="mb-4 px-1 text-xs leading-5 text-muted-foreground">
            The phone calendar is not connected. Days below still show events saved in the app.
          </p>
        )}
        {agenda.error && <Notice title={agenda.error} />}

        <p className="mb-2 px-1 text-sm text-muted-foreground">Today</p>
        {events.length === 0 ? (
          <EmptyState
            title={t("Nothing today")}
            body={t("Tap + to add an event, or ask Grok to schedule one.")}
            actionLabel={t("New event")}
            onAction={openNew}
          />
        ) : (
          <EventList events={events} onOpen={openEdit} />
        )}

        <div className="mt-8">
          <div className="mb-3 flex items-center justify-between px-1">
            <p className="text-[11px] tracking-[0.16em] text-muted-foreground uppercase">
              {cursor.toLocaleDateString(undefined, { month: "long", year: "numeric" })}
            </p>
            <div className="flex gap-2">
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  const next = new Date(year, month - 1, 1);
                  setCursor(next);
                  setSelected(next);
                }}
              >
                Previous
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  const next = new Date(year, month + 1, 1);
                  setCursor(next);
                  setSelected(next);
                }}
              >
                Next
              </Button>
            </div>
          </div>
          <div className="grid grid-cols-7 px-1 text-center text-xs text-muted-foreground">
            {["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].map((label) => (
              <span key={label}>{label}</span>
            ))}
          </div>
          <div className="mt-1 grid grid-cols-7">
            {cells.map((day, i) => {
              if (!day) return <span key={`pad-${i}`} />;
              const on = sameDay(day, selected);
              const count = monthEvents.filter((event) => overlapsDay(event, day)).length;
              const today = sameDay(day, new Date());
              return (
                <button
                  key={day.toISOString()}
                  type="button"
                  onClick={() => setSelected(day)}
                  aria-label={`${day.toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric" })}, ${count} ${count === 1 ? "event" : "events"}`}
                  aria-pressed={on}
                  className={`flex h-12 flex-col items-center justify-center text-sm tabular-nums ${
                    on ? "bg-foreground text-background" : "text-foreground"
                  } ${today && !on ? "underline decoration-2 underline-offset-2" : ""}`}
                >
                  {day.getDate()}
                  <span className="text-xs leading-4">{count > 0 ? count : ""}</span>
                </button>
              );
            })}
          </div>
          <p className="mt-4 mb-2 px-1 text-sm text-muted-foreground">
            {selected.toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric" })}
          </p>
          {selectedEvents.length === 0 ? (
            <EmptyState
              title={t("Nothing this day")}
              body={t("Choose another day, or tap + to add an event here.")}
              actionLabel={t("New event")}
              onAction={openNew}
            />
          ) : (
            <EventList events={selectedEvents} onOpen={openEdit} />
          )}
        </div>
      </PageChrome>

      <Sheet
        open={open}
        title={editing ? "Edit event" : "New event"}
        onClose={close}
        footer={
          <div className="flex flex-wrap items-center gap-2">
            <ShareButton title={title} text={[start, end, location].filter(Boolean).join("\n")} />
            {editing?.id && (
              <Button variant="ghost" disabled={agenda.busy} onClick={remove}>
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
        <div className="space-y-4">
          <Field label="Title" error={open ? agenda.error : null}>
            <Input
              autoFocus
              value={title}
              aria-invalid={Boolean(agenda.error) || undefined}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Zurich meetup"
            />
          </Field>
          <Field label="Starts">
            <Input type="datetime-local" value={start} onChange={(e) => setStart(e.target.value)} />
          </Field>
          <Field label="Ends">
            <Input type="datetime-local" value={end} onChange={(e) => setEnd(e.target.value)} />
          </Field>
          <Field label="Location">
            <Input value={location} onChange={(e) => setLocation(e.target.value)} placeholder="Cafe, or leave blank" />
          </Field>
        </div>
      </Sheet>
    </>
  );
}

function EventList({ events, onOpen }: { events: CalEvent[]; onOpen: (event: CalEvent) => void }) {
  return (
    <div className="divide-y divide-border border-y border-border">
      {events.map((event, i) => (
        <RowButton key={`${event.id || event.startMs}-${i}`} onClick={() => onOpen(event)} className="gap-4">
          <div className="w-14 shrink-0 pt-0.5 text-right">
            <p className="text-sm leading-5 tabular-nums text-foreground">
              {event.allDay ? "All day" : formatClock(event.startMs)}
            </p>
          </div>
          <div className="min-w-0 flex-1 border-l border-border pl-4">
            <p className="truncate text-base leading-6 text-foreground">{event.title || "Untitled"}</p>
            <p className="mt-1 truncate text-sm leading-5 text-muted-foreground">
              {formatSpan(event.startMs, event.endMs, event.allDay)}
              {event.location ? ` · ${event.location}` : ""}
              {event.source === "local" ? " · in app" : ""}
            </p>
          </div>
        </RowButton>
      ))}
    </div>
  );
}

function startOfDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

function sameDay(a: Date, b: Date): boolean {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

function overlapsDay(event: CalEvent, day: Date): boolean {
  const start = day.getTime();
  const end = start + 86_400_000;
  const eventEnd = event.endMs || event.startMs + 3_600_000;
  return event.startMs < end && eventEnd > start;
}

function monthCells(year: number, month: number): Array<Date | null> {
  const first = new Date(year, month, 1);
  const pad = (first.getDay() + 6) % 7;
  const count = new Date(year, month + 1, 0).getDate();
  const cells: Array<Date | null> = Array.from({ length: pad }, () => null);
  for (let day = 1; day <= count; day += 1) cells.push(new Date(year, month, day));
  return cells;
}

