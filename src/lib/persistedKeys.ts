/** localStorage keys a backup must round-trip. Keep this list equal to `WEB_KEYS` in persist.rs. */
export const PERSISTED_WEB_KEYS = [
  "grok.appearance",
  "grok.customTheme",
  "grok-dashboard-layout",
  "grok-finance-allocation",
  "grok-flow-options",
  "grok-flow-colors",
  "grok-chart-period",
  "grok-chart-hidden",
  "grok-mic-open",
  "grok-listen-hint",
] as const;

export type PersistedWebKey = (typeof PERSISTED_WEB_KEYS)[number];

const persisted = new Set<string>(PERSISTED_WEB_KEYS);

export function readPersistedWeb(): Record<string, string> {
  const out: Record<string, string> = {};
  if (typeof localStorage === "undefined") return out;
  for (const key of PERSISTED_WEB_KEYS) {
    const value = localStorage.getItem(key);
    if (value != null) out[key] = value;
  }
  return out;
}

/** Writes the backed-up keys and clears any listed key the file does not have. */
export function writePersistedWeb(data: Record<string, string> | undefined, present: boolean) {
  if (!present || typeof localStorage === "undefined") return;
  const saved = data ?? {};
  for (const key of PERSISTED_WEB_KEYS) {
    const value = saved[key];
    if (value == null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  }
}

export function isPersistedWebKey(key: string): boolean {
  return persisted.has(key);
}
