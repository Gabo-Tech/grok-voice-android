import { useCallback, useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Pencil, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api, errorText, type Agenda, type CalEvent, type Note, type ScheduledItem } from "@/lib/api";

const EMPTY: Agenda = { notes: [], reminders: [], events: [], calendar: "prompt" };

function when(ms: number): string {
  return new Date(ms).toLocaleString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function span(startMs: number, endMs: number, allDay: boolean): string {
  if (allDay) return "All day";
  const start = new Date(startMs);
  const end = new Date(endMs);
  const clock: Intl.DateTimeFormatOptions = { hour: "numeric", minute: "2-digit" };
  return `${start.toLocaleTimeString(undefined, clock)}–${end.toLocaleTimeString(undefined, clock)}`;
}

function toLocalInput(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function fromLocalInput(value: string): number {
  const t = Date.parse(value);
  return Number.isFinite(t) ? t : Date.now() + 60 * 60 * 1000;
}

export function AgendaView() {
  const inApp = isTauri();
  const [agenda, setAgenda] = useState<Agenda>(EMPTY);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [noteDraft, setNoteDraft] = useState("");
  const [editingNote, setEditingNote] = useState<Note | null>(null);

  const [itemTitle, setItemTitle] = useState("");
  const [itemAt, setItemAt] = useState(() => toLocalInput(Date.now() + 60 * 60 * 1000));
  const [itemNote, setItemNote] = useState("");
  const [editingItem, setEditingItem] = useState<ScheduledItem | null>(null);

  const [eventTitle, setEventTitle] = useState("");
  const [eventStart, setEventStart] = useState(() => toLocalInput(Date.now() + 60 * 60 * 1000));
  const [eventEnd, setEventEnd] = useState(() => toLocalInput(Date.now() + 2 * 60 * 60 * 1000));
  const [eventLocation, setEventLocation] = useState("");
  const [editingEvent, setEditingEvent] = useState<CalEvent | null>(null);

  const load = useCallback(() => {
    if (!inApp) return;
    api
      .agendaSnapshot()
      .then((next) => {
        setAgenda(next);
        setError(null);
      })
      .catch((e) => setError(errorText(e)));
  }, [inApp]);

  useEffect(() => {
    load();
    if (!inApp) return;
    let unlisten: (() => void) | undefined;
    void listen("voice://agenda", () => load()).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [inApp, load]);

  const run = async (work: () => Promise<Agenda>) => {
    setBusy(true);
    try {
      setAgenda(await work());
      setError(null);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const allowCalendar = async () => {
    setBusy(true);
    try {
      await api.requestCalendar();
      load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const saveNote = () => {
    const text = (editingNote ? noteDraft : noteDraft).trim();
    if (!text) return;
    void run(async () => {
      const next = await api.saveNote(text, editingNote?.id);
      setNoteDraft("");
      setEditingNote(null);
      return next;
    });
  };

  const saveItem = () => {
    const title = itemTitle.trim();
    if (!title) return;
    void run(async () => {
      const next = await api.saveScheduled({
        title,
        atMs: fromLocalInput(itemAt),
        note: itemNote.trim(),
        kind: editingItem?.kind ?? "reminder",
        id: editingItem?.id,
      });
      setItemTitle("");
      setItemNote("");
      setItemAt(toLocalInput(Date.now() + 60 * 60 * 1000));
      setEditingItem(null);
      return next;
    });
  };

  const saveEvent = () => {
    const title = eventTitle.trim();
    if (!title) return;
    const startMs = fromLocalInput(eventStart);
    const endMs = Math.max(fromLocalInput(eventEnd), startMs + 60_000);
    void run(async () => {
      const next = await api.saveEvent({
        title,
        startMs,
        endMs,
        location: eventLocation.trim(),
        id: editingEvent?.id,
      });
      setEventTitle("");
      setEventLocation("");
      setEventStart(toLocalInput(Date.now() + 60 * 60 * 1000));
      setEventEnd(toLocalInput(Date.now() + 2 * 60 * 60 * 1000));
      setEditingEvent(null);
      return next;
    });
  };

  return (
    <div className="space-y-6 pb-6">
      <p className="text-xs text-muted-foreground">
        Say it out loud, or edit here. Create, change, and delete notes, reminders, and calendar events.
      </p>
      {error && <p className="border border-border px-3 py-2 text-xs">{error}</p>}

      <section className="space-y-2">
        <h2 className="text-[11px] tracking-widest text-muted-foreground uppercase">Today</h2>
        {agenda.calendar !== "granted" && (
          <Button variant="outline" disabled={busy || !inApp} onClick={() => void allowCalendar()}>
            Allow calendar
          </Button>
        )}
        <div className="space-y-2 border border-border bg-card p-3">
          <Input
            placeholder={editingEvent ? "Edit event title" : "New event title"}
            value={eventTitle}
            onChange={(e) => setEventTitle(e.target.value)}
          />
          <Input type="datetime-local" value={eventStart} onChange={(e) => setEventStart(e.target.value)} />
          <Input type="datetime-local" value={eventEnd} onChange={(e) => setEventEnd(e.target.value)} />
          <Input
            placeholder="Location (optional)"
            value={eventLocation}
            onChange={(e) => setEventLocation(e.target.value)}
          />
          <div className="flex gap-2">
            <Button disabled={busy || !eventTitle.trim()} onClick={saveEvent}>
              <Plus /> {editingEvent ? "Save event" : "Add event"}
            </Button>
            {editingEvent && (
              <Button
                variant="ghost"
                onClick={() => {
                  setEditingEvent(null);
                  setEventTitle("");
                  setEventLocation("");
                }}
              >
                Cancel
              </Button>
            )}
          </div>
        </div>
        {agenda.events.length === 0 ? (
          <p className="text-sm text-muted-foreground">Nothing on the calendar today.</p>
        ) : (
          agenda.events.map((event, i) => (
            <div key={`${event.id || event.startMs}-${i}`} className="flex items-start justify-between gap-3 border border-border bg-card px-3 py-2">
              <div>
                <p className="text-sm">{event.title || "(untitled)"}</p>
                <p className="text-[11px] text-muted-foreground">
                  {span(event.startMs, event.endMs, event.allDay)}
                  {event.location ? ` · ${event.location}` : ""}
                  {event.source === "local" ? " · in app" : ""}
                </p>
              </div>
              <div className="flex shrink-0">
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    setEditingEvent(event);
                    setEventTitle(event.title);
                    setEventStart(toLocalInput(event.startMs));
                    setEventEnd(toLocalInput(event.endMs || event.startMs + 3_600_000));
                    setEventLocation(event.location || "");
                  }}
                >
                  <Pencil />
                </Button>
                {event.id && (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => void run(() => api.deleteEvent(event.id))}
                  >
                    <Trash2 />
                  </Button>
                )}
              </div>
            </div>
          ))
        )}
      </section>

      <section className="space-y-2">
        <h2 className="text-[11px] tracking-widest text-muted-foreground uppercase">Scheduled</h2>
        <div className="space-y-2 border border-border bg-card p-3">
          <Input
            placeholder={editingItem ? "Edit reminder title" : "New reminder title"}
            value={itemTitle}
            onChange={(e) => setItemTitle(e.target.value)}
          />
          <Input type="datetime-local" value={itemAt} onChange={(e) => setItemAt(e.target.value)} />
          <Input placeholder="Note (optional)" value={itemNote} onChange={(e) => setItemNote(e.target.value)} />
          <div className="flex gap-2">
            <Button disabled={busy || !itemTitle.trim()} onClick={saveItem}>
              <Plus /> {editingItem ? "Save reminder" : "Add reminder"}
            </Button>
            {editingItem && (
              <Button
                variant="ghost"
                onClick={() => {
                  setEditingItem(null);
                  setItemTitle("");
                  setItemNote("");
                }}
              >
                Cancel
              </Button>
            )}
          </div>
        </div>
        {agenda.reminders.length === 0 ? (
          <p className="text-sm text-muted-foreground">No reminders, alarms, or timers from this app.</p>
        ) : (
          agenda.reminders.map((item) => (
            <div key={item.id} className="flex items-start justify-between gap-3 border border-border bg-card px-3 py-2">
              <div>
                <p className="text-[10px] tracking-widest text-muted-foreground uppercase">{item.kind}</p>
                <p className="text-sm">{item.title}</p>
                <p className="text-[11px] text-muted-foreground">{when(item.atMs)}</p>
              </div>
              <div className="flex shrink-0">
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    setEditingItem(item);
                    setItemTitle(item.title);
                    setItemAt(toLocalInput(item.atMs));
                    setItemNote(item.note || "");
                  }}
                >
                  <Pencil />
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => void run(() => api.cancelScheduled(item.id))}
                >
                  <Trash2 />
                </Button>
              </div>
            </div>
          ))
        )}
      </section>

      <section className="space-y-2">
        <h2 className="text-[11px] tracking-widest text-muted-foreground uppercase">Notes</h2>
        <div className="space-y-2 border border-border bg-card p-3">
          <Input
            placeholder={editingNote ? "Edit note" : "New note"}
            value={noteDraft}
            onChange={(e) => setNoteDraft(e.target.value)}
          />
          <div className="flex gap-2">
            <Button disabled={busy || !noteDraft.trim()} onClick={saveNote}>
              <Plus /> {editingNote ? "Save note" : "Add note"}
            </Button>
            {editingNote && (
              <Button
                variant="ghost"
                onClick={() => {
                  setEditingNote(null);
                  setNoteDraft("");
                }}
              >
                Cancel
              </Button>
            )}
          </div>
        </div>
        {agenda.notes.length === 0 ? (
          <p className="text-sm text-muted-foreground">No notes yet.</p>
        ) : (
          agenda.notes.map((note) => (
            <div key={note.id} className="flex items-start justify-between gap-3 border border-border bg-card px-3 py-2">
              <div>
                <p className="text-sm whitespace-pre-wrap">{note.text}</p>
                <p className="mt-1 text-[11px] text-muted-foreground">{when(note.createdMs)}</p>
              </div>
              <div className="flex shrink-0">
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    setEditingNote(note);
                    setNoteDraft(note.text);
                  }}
                >
                  <Pencil />
                </Button>
                <Button size="sm" variant="ghost" onClick={() => void run(() => api.deleteNote(note.id))}>
                  <Trash2 />
                </Button>
              </div>
            </div>
          ))
        )}
      </section>
    </div>
  );
}
