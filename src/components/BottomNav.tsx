import { useSyncExternalStore } from "react";
import { AlarmClock, CalendarDays, ChevronLeft, Mic, NotebookPen, Plus, Settings, Wallet } from "lucide-react";
import { getAction, getNav, requestBack, subscribeAction, subscribeNav } from "@/lib/history";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

export type AppTab = "talk" | "notes" | "calendar" | "reminders" | "finance" | "more";

const ITEMS: Array<{ id: AppTab; label: string; icon: typeof Mic }> = [
  { id: "talk", label: "Talk", icon: Mic },
  { id: "notes", label: "Notes", icon: NotebookPen },
  { id: "calendar", label: "Calendar", icon: CalendarDays },
  { id: "reminders", label: "Time", icon: AlarmClock },
  { id: "finance", label: "Finance", icon: Wallet },
  { id: "more", label: "Settings", icon: Settings },
];

export function BottomNav({ tab, onChange }: { tab: AppTab; onChange: (t: AppTab) => void }) {
  const t = useT();
  const nav = useSyncExternalStore(subscribeNav, getNav, getNav);
  const action = useSyncExternalStore(subscribeAction, getAction, getAction);
  return (
    <nav className="bottom-nav">
      {(nav.canBack || action) && (
        <div className="flex items-center gap-1 border-b border-border px-1">
          {nav.canBack ? (
            <button type="button" onClick={() => requestBack()} className="flex min-h-12 min-w-0 flex-1 items-center gap-1 px-2 text-left text-base text-foreground">
              <ChevronLeft className="size-5 shrink-0" aria-hidden />
              <span className="truncate">{t(nav.label)}</span>
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
      <ul className="grid grid-cols-6">
        {ITEMS.map(({ id, label, icon: Icon }) => {
          const active = tab === id;
          return (
            <li key={id} className="min-w-0">
              <button
                type="button"
                onClick={() => onChange(id)}
                aria-current={active ? "page" : undefined}
                className={cn(
                  "flex min-h-14 w-full flex-col items-center justify-center px-0.5 py-1.5",
                  active ? "text-foreground" : "text-muted-foreground",
                )}
              >
                <Icon className="size-5 shrink-0" strokeWidth={1.5} />
                <span className="mt-0.5 line-clamp-2 w-full hyphens-auto text-center text-[10px] leading-3 break-words">{t(label)}</span>
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}

export function useThumbBar(): boolean {
  const nav = useSyncExternalStore(subscribeNav, getNav, getNav);
  const action = useSyncExternalStore(subscribeAction, getAction, getAction);
  return nav.canBack || action != null;
}
