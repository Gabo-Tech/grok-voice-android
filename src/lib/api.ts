import { invoke } from "@tauri-apps/api/core";

export type Phase =
  | "idle"
  | "wake"
  | "recording"
  | "connecting"
  | "listening"
  | "thinking"
  | "speaking";

export interface VoiceAccount {
  id: string;
  provider: string;
  hint: string;
}

export interface AuthStatus {
  hasKey: boolean;
  keyHint: string | null;
  securityLevel: string | null;
  accounts: VoiceAccount[];
}

export interface PermissionStatus {
  microphone: string;
  notifications: string;
  calendar?: string;
  contacts?: string;
}

export type PreferredChannel = "" | "call" | "sms" | "whatsapp" | "signal" | "email";

export interface PersonHandles {
  phone: string;
  email: string;
  whatsapp: string;
  signal: string;
}

export interface Person {
  id: string;
  androidContactId: string | null;
  displayName: string;
  nickname: string;
  aliases: string[];
  preferredChannel: PreferredChannel;
  handles: PersonHandles;
  favorite: boolean;
  note: string;
  /** Reserved for later. Always empty in this version. */
  reminders: unknown[];
}

export interface PersonDraft {
  id?: string;
  androidContactId: string | null;
  displayName: string;
  nickname: string;
  aliases: string[];
  preferredChannel: PreferredChannel;
  handles: PersonHandles;
  favorite: boolean;
  note: string;
}

export interface PhoneContact {
  id: string;
  name: string;
  phone: string;
  email: string;
}

export interface Note {
  id: string;
  title?: string;
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
  id: string;
  title: string;
  startMs: number;
  endMs: number;
  location: string;
  allDay: boolean;
  source?: string;
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
  addVoiceAccount: (provider: string, secret: string) =>
    invoke<AuthStatus>("add_voice_account", { provider, secret }),
  removeVoiceAccount: (id: string) => invoke<AuthStatus>("remove_voice_account", { id }),
  moveVoiceAccount: (id: string, up: boolean) => invoke<AuthStatus>("move_voice_account", { id, up }),
  clearUserToken: () => invoke<AuthStatus>("clear_user_token"),
  authStatus: () => invoke<AuthStatus>("auth_status"),
  permissionsStatus: () => invoke<PermissionStatus>("permissions_status"),
  requestPermissions: () => invoke<PermissionStatus>("request_permissions"),
  setWakeEnabled: (enabled: boolean) => invoke<boolean>("set_wake_enabled", { enabled }),
  startSession: () => invoke<void>("start_session"),
  askText: (text: string) => invoke<string>("ask_text", { text }),
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
  monthEvents: (startMs: number, endMs: number) =>
    invoke<{ events: CalEvent[]; calendar: string }>("month_events", { startMs, endMs }),
  saveNote: (text: string, id?: string, title?: string) =>
    invoke<Agenda>("save_note", { text, id: id ?? null, title: title ?? null }),
  deleteNote: (id: string) => invoke<Agenda>("delete_note", { id }),
  saveScheduled: (args: {
    title: string;
    atMs: number;
    note?: string;
    kind?: string;
    id?: number;
  }) =>
    invoke<Agenda>("save_scheduled", {
      title: args.title,
      atMs: args.atMs,
      note: args.note ?? null,
      kind: args.kind ?? null,
      id: args.id ?? null,
    }),
  cancelScheduled: (id: number) => invoke<Agenda>("cancel_scheduled", { id }),
  saveEvent: (args: {
    title: string;
    startMs: number;
    endMs: number;
    location?: string;
    id?: string;
  }) =>
    invoke<Agenda>("save_event", {
      title: args.title,
      startMs: args.startMs,
      endMs: args.endMs,
      location: args.location ?? null,
      id: args.id ?? null,
    }),
  deleteEvent: (id: string) => invoke<Agenda>("delete_event", { id }),
  requestCalendar: () => invoke<PermissionStatus>("request_calendar"),
  listPeople: () => invoke<Person[]>("list_people"),
  savePerson: (person: PersonDraft) => invoke<Person>("save_person", { person }),
  deletePerson: (id: string) => invoke<Person[]>("delete_person", { id }),
  requestContacts: () => invoke<PermissionStatus>("request_contacts"),
  listPhoneContacts: (query?: string) => invoke<PhoneContact[]>("list_phone_contacts", { query: query ?? null }),
  phoneContact: (id: string) => invoke<PhoneContact>("phone_contact", { id }),
  openPhoneContact: (id: string) => invoke<void>("open_phone_contact", { id }),
  recentMemory: () => invoke<Array<{ atMs: number; role: "user" | "assistant"; text: string }>>("recent_memory"),
  clearMemory: () => invoke<void>("clear_memory"),
};

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}
