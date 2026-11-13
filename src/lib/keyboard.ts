export interface InsetPlan {
  open: boolean;
  /** Extra page padding. Zero when the WebView itself already shrank. */
  pad: number;
  visibleBottom: number;
}

/**
 * One keyboard height, never two.
 * A resized WebView (adjustResize or a native bottom pad) already ends at the
 * keyboard, so the page must not add that height again. When the layout stays
 * full screen, the page pads by the inset and the focused field sits on the
 * keyboard while the bottom bar stays underneath it.
 */
export function resolveInset(
  fullHeight: number,
  layoutHeight: number,
  vvHeight: number,
  vvOffset: number,
  nativeIme: number,
): InsetPlan {
  const native = Math.max(0, Math.round(nativeIme));
  const vvGap = Math.max(0, Math.round(layoutHeight - vvHeight - vvOffset));
  const open = native > 48 || vvGap > 48;
  const resized = fullHeight - layoutHeight > 48;
  const pad = !open || resized ? 0 : Math.max(vvGap, native);
  const visibleBottom = vvGap > 48 ? vvOffset + vvHeight : layoutHeight - pad;
  return { open, pad, visibleBottom };
}

/** How far to scroll so the field's bottom sits `margin` px above `visibleBottom`. */
export function scrollDelta(fieldBottom: number, visibleBottom: number, margin = 12): number {
  const limit = visibleBottom - margin;
  if (fieldBottom <= limit) return 0;
  return fieldBottom - limit;
}

function isField(node: EventTarget | null): node is HTMLElement {
  if (!(node instanceof HTMLElement)) return false;
  if (node.isContentEditable) return true;
  const tag = node.tagName;
  if (tag === "TEXTAREA" || tag === "SELECT") return true;
  if (tag !== "INPUT") return false;
  const type = (node as HTMLInputElement).type;
  return !["button", "checkbox", "color", "file", "hidden", "image", "radio", "range", "reset", "submit"].includes(type);
}

function nearestScroller(node: HTMLElement): HTMLElement | null {
  let current: HTMLElement | null = node.parentElement;
  while (current && current !== document.body) {
    const style = window.getComputedStyle(current);
    const scrollable = /(auto|scroll)/.test(style.overflowY);
    if (scrollable && current.scrollHeight > current.clientHeight + 4) return current;
    current = current.parentElement;
  }
  return null;
}

function pin(visibleBottom: number) {
  const el = document.activeElement;
  if (!isField(el)) return;
  const rect = el.getBoundingClientRect();
  const inSheet = Boolean(el.closest(".sheet-panel"));
  const delta = scrollDelta(rect.bottom, visibleBottom, inSheet ? 96 : 12);
  const topGap = rect.top < 8 ? rect.top - 8 : 0;
  const move = delta !== 0 ? delta : topGap;
  if (move === 0) return;
  const scroller = nearestScroller(el);
  if (scroller) scroller.scrollTop += move;
}

export function installKeyboardInset() {
  if (typeof window === "undefined") return;
  let fullHeight = window.innerHeight;
  let nativeIme = 0;
  let timer = 0;

  const measure = () => {
    const layout = window.innerHeight;
    const vv = window.visualViewport;
    const plan = resolveInset(fullHeight, layout, vv?.height ?? layout, vv?.offsetTop ?? 0, nativeIme);
    if (!plan.open) fullHeight = layout;
    document.documentElement.style.setProperty("--keyboard-pad", `${plan.pad}px`);
    document.documentElement.classList.toggle("keyboard-open", plan.open);
    if (plan.open) pin(plan.visibleBottom);
  };

  const soon = () => {
    measure();
    window.clearTimeout(timer);
    timer = window.setTimeout(measure, 60);
    window.setTimeout(measure, 240);
  };

  window.__grokIme = (px: number) => {
    nativeIme = Number.isFinite(px) ? px : 0;
    soon();
  };
  window.visualViewport?.addEventListener("resize", soon);
  window.visualViewport?.addEventListener("scroll", soon);
  window.addEventListener("resize", soon);
  document.addEventListener("focusin", (event) => {
    if (isField(event.target)) soon();
  });
  document.addEventListener("focusout", () => {
    window.setTimeout(measure, 80);
  });
}

declare global {
  interface Window {
    __grokIme?: (px: number) => void;
  }
}
