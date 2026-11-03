import { useCallback, useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { api, errorText, type Agenda } from "@/lib/api";

const EMPTY: Agenda = { notes: [], reminders: [], events: [], calendar: "prompt" };

export function useAgenda() {
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

  const run = useCallback(async (work: () => Promise<Agenda>) => {
    setBusy(true);
    try {
      setAgenda(await work());
      setError(null);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const requestCalendar = useCallback(async () => {
    setBusy(true);
    try {
      await api.requestCalendar();
      load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  }, [load]);

  return { agenda, error, busy, inApp, load, run, requestCalendar, setError };
}

export type AgendaState = ReturnType<typeof useAgenda>;

export function toLocalInput(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function fromLocalInput(value: string): number {
  const t = Date.parse(value);
  return Number.isFinite(t) ? t : Date.now() + 60 * 60 * 1000;
}

export function formatWhen(ms: number): string {
  return new Date(ms).toLocaleString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

export function formatDay(ms = Date.now()): string {
  return new Date(ms).toLocaleDateString(undefined, {
    weekday: "long",
    month: "long",
    day: "numeric",
  });
}

export function formatSpan(startMs: number, endMs: number, allDay: boolean): string {
  if (allDay) return "All day";
  const clock: Intl.DateTimeFormatOptions = { hour: "numeric", minute: "2-digit" };
  return `${new Date(startMs).toLocaleTimeString(undefined, clock)}–${new Date(endMs).toLocaleTimeString(undefined, clock)}`;
}

export function formatClock(ms: number): string {
  return new Date(ms).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
}
