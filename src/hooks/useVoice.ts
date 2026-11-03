import { useCallback, useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { api, type Level, type LogLine, type Phase, type TranscriptEvent } from "@/lib/api";

export interface TranscriptRow {
  id: string;
  role: "user" | "assistant";
  text: string;
  isFinal: boolean;
}

const MAX_ROWS = 40;
const MAX_LOGS = 300;

export function useVoice() {
  const [phase, setPhase] = useState<Phase>("idle");
  const [wakeEnabled, setWakeEnabled] = useState(false);
  const [rows, setRows] = useState<TranscriptRow[]>([]);
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [lastError, setLastError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [busy, setBusy] = useState("");
  const levelRef = useRef<Level>({ mic: 0, out: 0 });
  const [level, setLevel] = useState<Level>({ mic: 0, out: 0 });
  const raf = useRef<number | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const unsubs: Array<() => void> = [];
    const sub = async <T>(event: string, cb: (payload: T) => void) => {
      const un = await listen<T>(event, (e) => cb(e.payload));
      if (disposed) un();
      else unsubs.push(un);
    };

    void sub<Phase>("voice://phase", setPhase);
    void sub<boolean>("voice://wake_enabled", setWakeEnabled);
    void sub<Level>("voice://level", (l) => {
      levelRef.current = l;
      if (raf.current == null) {
        raf.current = requestAnimationFrame(() => {
          raf.current = null;
          setLevel(levelRef.current);
        });
      }
    });
    void sub<LogLine>("voice://log", (line) => {
      setLogs((prev) => {
        const next = prev.length >= MAX_LOGS ? prev.slice(prev.length - MAX_LOGS + 1) : prev.slice();
        next.push(line);
        return next;
      });
      if (line.level === "error") setLastError(line.msg);
    });
    void sub<string>("voice://busy", setBusy);
    void sub<TranscriptEvent>("voice://transcript", (t) => {
      setRows((prev) => {
        const idx = prev.findIndex((r) => r.id === t.id && r.role === t.role);
        const next = prev.slice();
        if (idx === -1) {
          next.push({ id: t.id, role: t.role, text: t.text ?? t.delta ?? "", isFinal: t.isFinal });
        } else {
          const row = next[idx];
          next[idx] = {
            ...row,
            text: t.text != null ? t.text : row.text + (t.delta ?? ""),
            isFinal: t.isFinal || row.isFinal,
          };
        }
        return next.slice(-MAX_ROWS);
      });
    });

    void api.debugSnapshot().then((d) => {
      if (disposed) return;
      setPhase(d.phase);
      setWakeEnabled(d.wakeEnabled);
    });
    void api.debugLogs().then((l) => !disposed && setLogs(l));

    return () => {
      disposed = true;
      unsubs.forEach((u) => u());
      if (raf.current != null) cancelAnimationFrame(raf.current);
    };
  }, []);

  const toggleWake = useCallback(async (enabled: boolean) => {
    setLastError(null);
    try {
      setWakeEnabled(await api.setWakeEnabled(enabled));
    } catch (e) {
      setLastError(String(e));
      setWakeEnabled(false);
    }
  }, []);

  const sendText = useCallback(async (text: string) => {
    const trimmed = text.trim();
    if (!trimmed || sending) return;
    setLastError(null);
    setSending(true);
    try {
      await api.askText(trimmed);
    } catch (e) {
      setLastError(String(e));
    } finally {
      setSending(false);
    }
  }, [sending]);

  const toggleSession = useCallback(async () => {
    setLastError(null);
    try {
      if (["connecting", "listening", "thinking", "speaking"].includes(phase)) {
        await api.endSession();
      } else {
        await api.startSession();
      }
    } catch (e) {
      setLastError(String(e));
    }
  }, [phase]);

  return {
    phase,
    wakeEnabled,
    rows,
    logs,
    level,
    lastError,
    sending,
    busy,
    sendText,
    clearTranscript: () => setRows([]),
    clearLogs: () => setLogs([]),
    toggleWake,
    toggleSession,
  };
}

export type VoiceState = ReturnType<typeof useVoice>;
