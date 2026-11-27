import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { CalendarDays, ChevronLeft, Mic, NotebookPen, Plus, Settings, Wallet } from "lucide-react";
import { getAction, getNav, requestBack, subscribeAction, subscribeNav } from "@/lib/history";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

export type AppTab = "talk" | "notes" | "agenda" | "finance" | "more";

const ITEMS: Array<{ id: AppTab; label: string; icon: typeof Mic }> = [
  { id: "talk", label: "Talk", icon: Mic },
  { id: "notes", label: "Notes", icon: NotebookPen },
  { id: "agenda", label: "Agenda", icon: CalendarDays },
  { id: "finance", label: "Finance", icon: Wallet },
  { id: "more", label: "Settings", icon: Settings },
];

export function BottomNav({ tab, onChange }: { tab: AppTab; onChange: (t: AppTab) => void }) {
  const t = useT();
  const nav = useSyncExternalStore(subscribeNav, getNav, getNav);
  const action = useSyncExternalStore(subscribeAction, getAction, getAction);
  const listRef = useRef<HTMLUListElement>(null);
  const [compact, setCompact] = useState(false);
  const [rail, setRail] = useState(false);
  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const fit = () => {
      const sample = list.querySelector("button");
      const probe = document.createElement("span");
      probe.style.position = "absolute";
      probe.style.visibility = "hidden";
      probe.style.whiteSpace = "nowrap";
      probe.style.font = sample ? getComputedStyle(sample).font : "13px system-ui";
      list.appendChild(probe);
      const widest = ITEMS.reduce((max, item) => {
        probe.textContent = t(item.label);
        return Math.max(max, probe.offsetWidth);
      }, 0);
      probe.remove();
      const column = getComputedStyle(list).flexDirection === "column";
      const cell = column ? list.clientWidth : list.clientWidth / ITEMS.length;
      setRail(column);
      setCompact(widest > cell - 8);
    };
    fit();
    const soon = window.setTimeout(fit, 160);
    const later = window.setTimeout(fit, 420);
    const observer = new ResizeObserver(fit);
    observer.observe(list);
    window.addEventListener("resize", fit);
    return () => {
      window.clearTimeout(soon);
      window.clearTimeout(later);
      observer.disconnect();
      window.removeEventListener("resize", fit);
    };
  }, [t]);
  useEffect(() => {
    const bar = listRef.current?.closest(".bottom-nav");
    if (!bar) return;
    const publish = () => {
      document.documentElement.style.setProperty("--nav-block", `${bar.getBoundingClientRect().height}px`);
    };
    publish();
    const observer = new ResizeObserver(publish);
    observer.observe(bar);
    return () => observer.disconnect();
  }, [compact, rail, tab]);
  const activeLabel = ITEMS.find((item) => item.id === tab)?.label ?? "Talk";
  return (
    <nav className="bottom-nav" data-compact={compact ? "1" : "0"} data-rail={rail ? "1" : "0"}>
      {(nav.showBack || action) && (
        <div className="flex items-center gap-1 border-b border-border px-1">
          {nav.showBack ? (
            <button
              type="button"
              onClick={() => requestBack()}
              aria-label={t(nav.label)}
              className={cn(
                "flex min-h-12 items-center justify-center gap-1 text-left text-base text-foreground",
                rail ? "size-12 shrink-0 px-0" : "min-w-0 flex-1 px-2",
              )}
            >
              <ChevronLeft className="size-5 shrink-0" aria-hidden />
              {rail ? null : <span className="truncate">{t(nav.label)}</span>}
            </button>
          ) : (
            <span className="flex-1" />
          )}
          {action ? (
            <button type="button" onClick={action.run} aria-label={t(action.label)} className="grid size-12 shrink-0 place-items-center text-foreground">
              <Plus className="size-5" aria-hidden />
            </button>
          ) : null}
        </div>
      )}
      <ul ref={listRef} className="grid grid-cols-5">
        {ITEMS.map(({ id, label, icon: Icon }) => {
          const active = tab === id;
          const showLabel = !compact;
          return (
            <li key={id} className="min-w-0">
              <button
                type="button"
                onClick={() => onChange(id)}
                aria-current={active ? "page" : undefined}
                aria-label={t(label)}
                className={cn(
                  "flex min-h-12 w-full min-w-0 max-w-full flex-col items-center justify-center gap-1 px-0.5 py-1",
                  active ? "text-foreground" : "text-muted-foreground",
                )}
              >
                <span className="nav-pill">
                  <Icon className="size-6 shrink-0" strokeWidth={1.5} />
                </span>
                {showLabel ? <span className={cn("nav-label", active && "font-medium")}>{t(label)}</span> : null}
              </button>
            </li>
          );
        })}
      </ul>
      {compact && !rail ? <p className="nav-active-label">{t(activeLabel)}</p> : null}
    </nav>
  );
}

export function useThumbBar(): boolean {
  const nav = useSyncExternalStore(subscribeNav, getNav, getNav);
  const action = useSyncExternalStore(subscribeAction, getAction, getAction);
  return nav.showBack || action != null;
}
