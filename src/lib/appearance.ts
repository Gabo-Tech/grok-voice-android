export type AppearanceChoice = "graphite" | "ink" | "stone" | "system";
export type ResolvedAppearance = "graphite" | "ink" | "stone";

const STORAGE_KEY = "grok.appearance";

const CANVAS: Record<ResolvedAppearance, string> = {
  graphite: "#0E0F11",
  ink: "#070708",
  stone: "#F4F2EE",
};

export function isAppearanceChoice(value: string): value is AppearanceChoice {
  return value === "graphite" || value === "ink" || value === "stone" || value === "system";
}

/** A missing or unknown value stays on Graphite. System is only an explicit pick. */
export function normalizeAppearance(value: string | undefined | null): AppearanceChoice {
  const cleaned = (value ?? "").trim().toLowerCase();
  return isAppearanceChoice(cleaned) ? cleaned : "graphite";
}

export function hasStoredAppearance(): boolean {
  try {
    return window.localStorage.getItem(STORAGE_KEY) !== null;
  } catch {
    return false;
  }
}

export function readStoredAppearance(): AppearanceChoice {
  try {
    return normalizeAppearance(window.localStorage.getItem(STORAGE_KEY));
  } catch {
    return "graphite";
  }
}

export function rememberAppearance(choice: AppearanceChoice) {
  try {
    window.localStorage.setItem(STORAGE_KEY, choice);
  } catch {
    /* private mode */
  }
}

export function resolvedAppearance(choice: AppearanceChoice, prefersLight: boolean): ResolvedAppearance {
  if (choice === "system") return prefersLight ? "stone" : "graphite";
  return choice;
}

export function applyAppearance(choice: AppearanceChoice, prefersLight: boolean) {
  if (typeof document === "undefined") return;
  const resolved = resolvedAppearance(choice, prefersLight);
  const root = document.documentElement;
  root.dataset.appearance = resolved;
  root.dataset.appearanceChoice = choice;
  root.classList.toggle("dark", resolved !== "stone");
  root.style.colorScheme = resolved === "stone" ? "light" : "dark";
  const canvas = CANVAS[resolved];
  const meta = document.querySelector('meta[name="theme-color"]');
  if (meta) meta.setAttribute("content", canvas);
  const bridge = (window as unknown as { GrokTheme?: { setCanvas?: (hex: string) => void } }).GrokTheme;
  try {
    bridge?.setCanvas?.(canvas);
  } catch {
    /* the page still paints from CSS */
  }
}

export function bootAppearance() {
  applyAppearance(readStoredAppearance(), prefersLight());
}

export function prefersLight(): boolean {
  return typeof window !== "undefined" && window.matchMedia("(prefers-color-scheme: light)").matches;
}

/** Server value wins when it is a real choice. Empty keeps whatever already painted. */
export function choiceFromSettings(value: string | undefined | null): AppearanceChoice | null {
  const cleaned = (value ?? "").trim().toLowerCase();
  return isAppearanceChoice(cleaned) ? cleaned : null;
}
