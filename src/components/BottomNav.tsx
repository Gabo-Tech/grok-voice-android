import { AlarmClock, CalendarDays, Mic, NotebookPen, Settings } from "lucide-react";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

export type AppTab = "talk" | "notes" | "calendar" | "reminders" | "more";

const ITEMS: Array<{ id: AppTab; label: string; icon: typeof Mic }> = [
  { id: "talk", label: "Talk", icon: Mic },
  { id: "notes", label: "Notes", icon: NotebookPen },
  { id: "calendar", label: "Calendar", icon: CalendarDays },
  { id: "reminders", label: "Time", icon: AlarmClock },
  { id: "more", label: "Settings", icon: Settings },
];

export function BottomNav({ tab, onChange }: { tab: AppTab; onChange: (t: AppTab) => void }) {
  const t = useT();
  return (
    <nav className="bottom-nav">
      <ul className="grid grid-cols-5">
        {ITEMS.map(({ id, label, icon: Icon }) => {
          const active = tab === id;
          return (
            <li key={id}>
              <button
                type="button"
                onClick={() => onChange(id)}
                aria-current={active ? "page" : undefined}
                className={cn(
                  "flex min-h-12 w-full flex-col items-center justify-center px-1 py-2 text-xs leading-4",
                  active ? "text-foreground" : "text-muted-foreground",
                )}
              >
                <Icon className="size-5" strokeWidth={1.5} />
                <span className="mt-1">{t(label)}</span>
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
