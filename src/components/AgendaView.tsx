import { useCallback, useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { api, errorText, type Agenda } from "@/lib/api";

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

export function AgendaView() {
  const inApp = isTauri();
  const [agenda, setAgenda] = useState<Agenda>(EMPTY);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

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

  return (
    <div className="space-y-6 pb-6">
      <p className="text-xs text-muted-foreground">
        Say “make a note”, “set a timer for 10 minutes”, “remind me tomorrow at 9”, or “what’s on my calendar”.
      </p>
      {error && <p className="border border-border px-3 py-2 text-xs">{error}</p>}

      <section className="space-y-2">
        <h2 className="text-[11px] tracking-widest text-muted-foreground uppercase">Today</h2>
        {agenda.calendar !== "granted" ? (
          <Button variant="outline" disabled={busy || !inApp} onClick={() => void allowCalendar()}>
            Allow calendar
          </Button>
        ) : agenda.events.length === 0 ? (
          <p className="text-sm text-muted-foreground">Nothing on the calendar today.</p>
        ) : (
          agenda.events.map((event, i) => (
            <div key={`${event.startMs}-${i}`} className="border border-border bg-card px-3 py-2">
              <p className="text-sm">{event.title || "(untitled)"}</p>
              <p className="text-[11px] text-muted-foreground">
                {span(event.startMs, event.endMs, event.allDay)}
                {event.location ? ` · ${event.location}` : ""}
              </p>
            </div>
          ))
        )}
      </section>

      <section className="space-y-2">
        <h2 className="text-[11px] tracking-widest text-muted-foreground uppercase">Scheduled</h2>
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
              <Button
                size="sm"
                variant="ghost"
                onClick={() => void api.cancelScheduled(item.id).then(setAgenda).catch((e) => setError(errorText(e)))}
              >
                <Trash2 />
              </Button>
            </div>
          ))
        )}
      </section>

      <section className="space-y-2">
        <h2 className="text-[11px] tracking-widest text-muted-foreground uppercase">Notes</h2>
        {agenda.notes.length === 0 ? (
          <p className="text-sm text-muted-foreground">No notes yet.</p>
        ) : (
          agenda.notes.map((note) => (
            <div key={note.id} className="flex items-start justify-between gap-3 border border-border bg-card px-3 py-2">
              <div>
                <p className="text-sm whitespace-pre-wrap">{note.text}</p>
                <p className="mt-1 text-[11px] text-muted-foreground">{when(note.createdMs)}</p>
              </div>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => void api.deleteNote(note.id).then(setAgenda).catch((e) => setError(errorText(e)))}
              >
                <Trash2 />
              </Button>
            </div>
          ))
        )}
      </section>
    </div>
  );
}
