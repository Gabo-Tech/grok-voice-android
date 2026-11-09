type Step = { id: number; label: string; tag: string; undo: () => void };
type Overlay = { id: number; dismiss: () => void };
type BarAction = { label: string; run: () => void };

let steps: Step[] = [];
let overlays: Overlay[] = [];
let seq = 0;
let applying = 0;
let installed = false;

type Snap = { canBack: boolean; label: string };
let snap: Snap = { canBack: false, label: "Back" };
let action: BarAction | null = null;
const listeners = new Set<() => void>();
const actionListeners = new Set<() => void>();

type NavWindow = Window & {
  __grokBack?: () => boolean;
  GrokNav?: { setCanGoBack: (can: boolean) => void };
};

function publish() {
  const top = steps[steps.length - 1];
  const canBack = overlays.length > 0 || steps.length > 0;
  const label = overlays.length > 0 ? "Back" : (top?.label ?? "Back");
  snap = { canBack, label };
  listeners.forEach((fn) => fn());
  actionListeners.forEach((fn) => fn());
  try {
    (window as NavWindow).GrokNav?.setCanGoBack(canBack);
  } catch {
    /* The Android bridge is absent in a desktop preview. */
  }
}

export function installHistory() {
  if (installed || typeof window === "undefined") return;
  installed = true;
  const state = history.state as { grok?: number } | null;
  if (state == null || typeof state.grok !== "number") {
    history.replaceState({ grok: 0, id: 0 }, "");
  }
  window.addEventListener("popstate", onPop);
  (window as NavWindow).__grokBack = requestBack;
  publish();
}

function onPop() {
  const target = Number((history.state as { grok?: number } | null)?.grok ?? 0);
  applying += 1;
  try {
    while (steps.length > target) {
      steps.pop()?.undo();
    }
  } finally {
    applying -= 1;
    publish();
  }
}

export function isApplyingHistory(): boolean {
  return applying > 0;
}

export function pushStep(label: string, undo: () => void, tag = "screen"): number {
  if (applying > 0) return -1;
  const id = ++seq;
  steps.push({ id, label, tag, undo });
  history.pushState({ grok: steps.length, id }, "");
  publish();
  return id;
}

export function releaseStep(id: number) {
  if (id < 0 || applying > 0) return;
  const idx = steps.findIndex((step) => step.id === id);
  if (idx < 0) return;
  if (idx === steps.length - 1) {
    history.back();
    return;
  }
  steps[idx] = { ...steps[idx], undo: () => {} };
}

/** Dialogs and sheets close before screen history. They are not history entries. */
export function pushOverlay(dismiss: () => void): () => void {
  const id = ++seq;
  overlays.push({ id, dismiss });
  publish();
  return () => {
    const had = overlays.some((item) => item.id === id);
    overlays = overlays.filter((item) => item.id !== id);
    if (had) publish();
  };
}

export function requestBack(): boolean {
  const overlay = overlays[overlays.length - 1];
  if (overlay) {
    overlays = overlays.slice(0, -1);
    overlay.dismiss();
    publish();
    return true;
  }
  if (steps.length === 0) return false;
  history.back();
  return true;
}

export function popTags(tags: string[]) {
  if (applying > 0) return;
  let count = 0;
  for (let i = steps.length - 1; i >= 0; i -= 1) {
    if (!tags.includes(steps[i].tag)) break;
    count += 1;
  }
  if (count > 0) history.go(-count);
}

export function popUntil(tag: string) {
  if (applying > 0) return;
  let count = 0;
  for (let i = steps.length - 1; i >= 0; i -= 1) {
    if (steps[i].tag === tag) break;
    count += 1;
  }
  if (count > 0) history.go(-count);
}

export function hasTag(tag: string): boolean {
  return steps.some((step) => step.tag === tag);
}

export function subscribeNav(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function getNav(): Snap {
  return snap;
}

export function setBarAction(next: BarAction | null): () => void {
  action = next;
  actionListeners.forEach((fn) => fn());
  return () => {
    if (action === next) {
      action = null;
      actionListeners.forEach((fn) => fn());
    }
  };
}

export function subscribeAction(fn: () => void): () => void {
  actionListeners.add(fn);
  return () => actionListeners.delete(fn);
}

export function getAction(): BarAction | null {
  return action;
}
