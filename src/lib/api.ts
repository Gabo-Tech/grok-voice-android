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

export interface PersonReminder {
  id: string;
  label: "birthday" | "anniversary" | "custom" | string;
  title: string;
  /** MM-DD, or YYYY-MM-DD when it happens once. */
  date: string;
  recurrence: "yearly" | "once" | string;
  leadDays: number;
  time: string;
  enabled: boolean;
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
  reminders: PersonReminder[];
  reachOutDays: number;
  lastContactedMs: number | null;
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
  reminders: PersonReminder[];
  reachOutDays?: number;
  lastContactedMs?: number | null;
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
  recurrence?: string;
  weekdays?: number[];
  time?: string;
  date?: string;
  leadDays?: number;
  personId?: string;
  personReminderId?: string;
  untilMs?: number;
}

export interface CalEvent {
  id: string;
  title: string;
  startMs: number;
  endMs: number;
  location: string;
  allDay: boolean;
  source?: string;
  recurrence?: string;
  weekdays?: number[];
  untilMs?: number;
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

export interface LocalPack {
  id: string;
  label: string;
  detail: string;
  downloadBytes: number;
  installedBytes: number;
  ready: boolean;
  /** Set when an older pack is installed and this one still needs a download. */
  note?: string;
}

export interface PackProgress {
  id: string;
  received: number;
  total: number;
}

export interface Settings {
  model: string;
  voice: string;
  instructions: string;
  echoGuard: boolean;
  wakeThreshold: number;
  idleTimeoutSecs: number;
  onboarded: boolean;
  /** Soft daily voice budget in minutes. 0 is off. */
  dailyBudgetMinutes: number;
  /** End the session at the budget. Off unless the user opts in. */
  dailyBudgetHardStop: boolean;
  /** Wake sessions try a saved on-device account first when packs are ready. */
  preferLocalOnWake: boolean;
  /** Soft monthly voice budget in minutes. 0 is off. */
  monthlyBudgetMinutes: number;
  monthlyBudgetHardStop: boolean;
  /** `en` or `de` for on-device speech. */
  localSpeech: string;
  /** Empty follows the phone. */
  locale: string;
  /** Hands-free Listen. Saved on the phone and restored after the app is cleared. */
  listenArmed?: boolean;
}

export interface UsageStatus {
  day: string;
  minutes: number;
  sessions: number;
  budgetMinutes: number;
  hardStop: boolean;
  xaiMinutes: number;
  openaiMinutes: number;
  localMinutes: number;
  month: string;
  monthMinutes: number;
  monthXaiMinutes: number;
  monthOpenaiMinutes: number;
  monthLocalMinutes: number;
  monthlyBudgetMinutes: number;
  monthlyHardStop: boolean;
  openSinceMs: number | null;
  openProvider: string;
  message: string | null;
}

export interface BackupPreview {
  notes: number;
  people: number;
  reminders: number;
  keys: number;
  facts: number;
  sessions: number;
  hasMemory: boolean;
}

export interface LocalFact {
  id: string;
  text: string;
  updatedMs: number;
}

export interface LocalSessionNote {
  id: string;
  atMs: number;
  text: string;
}

export interface LocalMemoryBundle {
  facts: LocalFact[];
  sessions: LocalSessionNote[];
}

export interface HandoffDraft {
  id: string;
  summary: string;
  kind: string;
  phone: string;
  text: string;
}

export interface AccessStatus {
  notificationListener: string;
  dnd: string;
  callLog: string;
  sms: string;
}

export interface ExportedBackup {
  fileName: string;
  dataB64: string;
  shared: boolean;
}

export interface SpendCue {
  message: string;
  minutes: number;
  budgetMinutes: number;
  blocked: boolean;
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
  provider?: string | null;
  foregroundService?: boolean;
  micSource?: string;
  activityResumed?: boolean;
  screenInteractive?: boolean;
  lastStopReason?: string | null;
  listenPreference?: boolean;
  serviceArmed?: boolean;
  listenerHistory?: string[];
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

export interface UpdateStatus {
  installed: string;
  version: string;
  tag: string;
  url: string;
}

export const api = {
  saveUserToken: (token: string) => invoke<AuthStatus>("save_user_token", { token }),
  addVoiceAccount: (provider: string, secret: string) =>
    invoke<AuthStatus>("add_voice_account", { provider, secret }),
  updateVoiceAccount: (id: string, provider: string, secret: string) =>
    invoke<AuthStatus>("update_voice_account", { id, provider, secret }),
  removeVoiceAccount: (id: string) => invoke<AuthStatus>("remove_voice_account", { id }),
  reorderVoiceAccounts: (ids: string[]) => invoke<AuthStatus>("reorder_voice_accounts", { ids }),
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
    recurrence?: string;
    weekdays?: number[];
    time?: string;
    date?: string;
    leadDays?: number;
    untilMs?: number;
  }) =>
    invoke<Agenda>("save_scheduled", {
      title: args.title,
      atMs: args.atMs,
      note: args.note ?? null,
      kind: args.kind ?? null,
      id: args.id ?? null,
      recurrence: args.recurrence ?? null,
      weekdays: args.weekdays ?? null,
      time: args.time ?? null,
      date: args.date ?? null,
      leadDays: args.leadDays ?? null,
      untilMs: args.untilMs ?? null,
    }),
  cancelScheduled: (id: number) => invoke<Agenda>("cancel_scheduled", { id }),
  completeScheduled: (id: number) => invoke<Agenda>("complete_scheduled", { id }),
  usageStatus: () => invoke<UsageStatus>("usage_status"),
  exportBackup: (passphrase: string, includeKeys = false) =>
    invoke<ExportedBackup>("export_backup", { passphrase, includeKeys }),
  previewBackup: (dataB64: string, passphrase: string) =>
    invoke<BackupPreview>("preview_backup", { dataB64, passphrase }),
  restoreBackup: (dataB64: string, passphrase: string, confirm: boolean, restoreKeys = false) =>
    invoke<BackupPreview>("restore_backup", { dataB64, passphrase, confirm, restoreKeys }),
  saveEvent: (args: {
    title: string;
    startMs: number;
    endMs: number;
    location?: string;
    id?: string;
    recurrence?: string;
    weekdays?: number[];
    untilMs?: number;
  }) =>
    invoke<Agenda>("save_event", {
      title: args.title,
      startMs: args.startMs,
      endMs: args.endMs,
      location: args.location ?? null,
      id: args.id ?? null,
      recurrence: args.recurrence ?? null,
      weekdays: args.weekdays ?? null,
      untilMs: args.untilMs ?? null,
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
  localMemory: () => invoke<LocalMemoryBundle>("local_memory"),
  pinLocalFact: (text: string, id?: string) =>
    invoke<LocalMemoryBundle>("pin_local_fact", { text, id: id ?? null }),
  forgetLocalFact: (id?: string, text?: string) =>
    invoke<LocalMemoryBundle>("forget_local_fact", { id: id ?? null, text: text ?? null }),
  wipeLocalMemory: () => invoke<LocalMemoryBundle>("wipe_local_memory"),
  confirmHandoff: () => invoke<unknown>("confirm_handoff"),
  dismissHandoff: () => invoke<void>("dismiss_handoff"),
  accessStatus: () => invoke<AccessStatus>("access_status"),
  openAccessSettings: (kind: "notifications" | "dnd") => invoke<unknown>("open_access_settings", { kind }),
  requestHistoryAccess: () => invoke<PermissionStatus>("request_history_access"),
  refreshLastContact: (id: string) => invoke<Person>("refresh_last_contact", { id }),
  importContactBirthday: (id: string) => invoke<Person>("import_contact_birthday", { id }),
  importContactDates: (id: string) => invoke<Person>("import_contact_dates", { id }),
  localPacks: () => invoke<LocalPack[]>("local_pack_status"),
  downloadLocalPack: (id: string) => invoke<LocalPack[]>("download_local_pack", { id }),
  deleteLocalPack: (id: string) => invoke<LocalPack[]>("delete_local_pack", { id }),
  updateStatus: () => invoke<UpdateStatus>("update_status"),
  dismissUpdate: () => invoke<UpdateStatus>("dismiss_update"),
};

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return JSON.stringify(e);
}
