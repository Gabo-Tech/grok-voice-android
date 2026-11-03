import { invoke } from "@tauri-apps/api/core";

export type Phase =
  | "idle"
  | "wake"
  | "recording"
  | "connecting"
  | "listening"
  | "thinking"
  | "speaking";

export interface AuthStatus {
  hasKey: boolean;
  keyHint: string | null;
  securityLevel: string | null;
}

export interface PermissionStatus {
  microphone: string;
  notifications: string;
  calendar?: string;
}

export interface Note {
  id: string;
  text: string;
  createdMs: number;
}

export interface ScheduledItem {
  id: number;
  kind: string;
  title: string;
  note: string;
  atMs: number;
}

export interface CalEvent {
  title: string;
  startMs: number;
  endMs: number;
  location: string;
  allDay: boolean;
}

export interface Agenda {
  notes: Note[];
  reminders: ScheduledItem[];
  events: CalEvent[];
  calendar: string;
}

export interface WakewordStatus {
  trained: boolean;
  samples: boolean[];
}

export interface SampleInfo {
  index: number;
  durationMs: number;
  peak: number;
}

export interface Settings {
  model: string;
  voice: string;
  instructions: string;
  echoGuard: boolean;
  wakeThreshold: number;
  idleTimeoutSecs: number;
  onboarded: boolean;
}

export interface DebugInfo {
  phase: Phase;
  wakeEnabled: boolean;
  wakewordTrained: boolean;
  wsState: string;
  model: string | null;
  tokenMintedAtMs: number | null;
  tokenExpiresAt: number | null;
  framesSent: number;
  bytesSent: number;
  framesDropped: number;
  audioDeltas: number;
  bytesReceived: number;
  eventsReceived: number;
  lastEvent: string | null;
  lastError: string | null;
  micRate: number | null;
  speakerRate: number | null;
  lastWakeScore: number | null;
  sessions: number;
  vaultSecurityLevel: string | null;
  keyHint: string | null;
  platform: string;
}

export interface LogLine {
  tsMs: number;
  level: "info" | "warn" | "error";
  msg: string;
}

export interface TranscriptEvent {
  id: string;
  role: "user" | "assistant";
  text: string | null;
  delta: string | null;
  isFinal: boolean;
}

export interface Level {
  mic: number;
  out: number;
}

export const api = {
  saveUserToken: (token: string) => invoke<AuthStatus>("save_user_token", { token }),
  clearUserToken: () => invoke<AuthStatus>("clear_user_token"),
  authStatus: () => invoke<AuthStatus>("auth_status"),
  permissionsStatus: () => invoke<PermissionStatus>("permissions_status"),
  requestPermissions: () => invoke<PermissionStatus>("request_permissions"),
  setWakeEnabled: (enabled: boolean) => invoke<boolean>("set_wake_enabled", { enabled }),
  startSession: () => invoke<void>("start_session"),
  endSession: () => invoke<void>("end_session"),
  recordWakeSample: (index: number, seconds?: number) =>
    invoke<SampleInfo>("record_wake_sample", { index, seconds }),
  trainWakeword: () => invoke<WakewordStatus>("train_wakeword"),
  wakewordStatus: () => invoke<WakewordStatus>("wakeword_status"),
  deleteWakeword: () => invoke<WakewordStatus>("delete_wakeword"),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  debugSnapshot: () => invoke<DebugInfo>("debug_snapshot"),
  debugLogs: () => invoke<LogLine[]>("debug_logs"),
  agendaSnapshot: () => invoke<Agenda>("agenda_snapshot"),
  deleteNote: (id: string) => invoke<Agenda>("delete_note", { id }),
  cancelScheduled: (id: number) => invoke<Agenda>("cancel_scheduled", { id }),
  requestCalendar: () => invoke<PermissionStatus>("request_calendar"),
};

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}
