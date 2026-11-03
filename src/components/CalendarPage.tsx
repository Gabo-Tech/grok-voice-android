import { useMemo, useState } from "react";
import { Trash2 } from "lucide-react";
import { EmptyState, PageChrome, RowButton } from "@/components/PageChrome";
import { Sheet } from "@/components/Sheet";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { AgendaState } from "@/hooks/useAgenda";
import { formatClock, formatDay, formatSpan, fromLocalInput, toLocalInput } from "@/hooks/useAgenda";
import { api, type CalEvent } from "@/lib/api";

export function CalendarPage({ agenda }: { agenda: AgendaState }) {
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

  const openNew = () => {
    setEditing(null);
    setTitle("");
    setStart(toLocalInput(Date.now() + 3_600_000));
    setEnd(toLocalInput(Date.now() + 7_200_000));
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
      <PageChrome title="Calendar" subtitle={formatDay()} onAdd={openNew} addLabel="New event">
        {agenda.agenda.calendar !== "granted" && (
          <Button
            variant="outline"
            className="mb-4 w-full"
            disabled={agenda.busy || !agenda.inApp}
            onClick={() => void agenda.requestCalendar()}
          >
            Allow calendar access
          </Button>
        )}
        {agenda.error && <p className="mb-3 px-1 text-xs text-muted-foreground">{agenda.error}</p>}
        {events.length === 0 ? (
          <EmptyState title="Nothing today" body="Tap + to add an event, or ask Grok to schedule one." />
        ) : (
          <div className="divide-y divide-border border-y border-border">
            {events.map((event, i) => (
              <RowButton
                key={`${event.id || event.startMs}-${i}`}
                onClick={() => openEdit(event)}
                className="gap-4"
              >
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
        )}
      </PageChrome>

      <Sheet
        open={open}
        title={editing ? "Edit event" : "New event"}
        onClose={close}
        footer={
          <div className="flex items-center gap-2">
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
          <Field label="Title">
            <Input autoFocus value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Event" />
          </Field>
          <Field label="Starts">
            <Input type="datetime-local" value={start} onChange={(e) => setStart(e.target.value)} />
          </Field>
          <Field label="Ends">
            <Input type="datetime-local" value={end} onChange={(e) => setEnd(e.target.value)} />
          </Field>
          <Field label="Location">
            <Input value={location} onChange={(e) => setLocation(e.target.value)} placeholder="Optional" />
          </Field>
        </div>
      </Sheet>
    </>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block space-y-1.5">
      <span className="text-[11px] tracking-[0.16em] text-muted-foreground uppercase">{label}</span>
      {children}
    </label>
  );
}
