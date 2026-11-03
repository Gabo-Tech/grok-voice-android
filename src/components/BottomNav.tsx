import { Bell, CalendarDays, Mic, MoreHorizontal, StickyNote } from "lucide-react";
import { cn } from "@/lib/utils";

export type AppTab = "talk" | "notes" | "calendar" | "reminders" | "more";

const ITEMS: Array<{ id: AppTab; label: string; Icon: typeof Mic }> = [
  { id: "talk", label: "Talk", Icon: Mic },
  { id: "notes", label: "Notes", Icon: StickyNote },
  { id: "calendar", label: "Calendar", Icon: CalendarDays },
  { id: "reminders", label: "Remind", Icon: Bell },
  { id: "more", label: "More", Icon: MoreHorizontal },
];

export function BottomNav({ tab, onChange }: { tab: AppTab; onChange: (t: AppTab) => void }) {
  return (
    <nav className="bottom-nav shrink-0 border-t border-border bg-[#0e0e0e]/95 backdrop-blur-sm">
      <ul className="grid grid-cols-5">
        {ITEMS.map(({ id, label, Icon }) => {
          const active = tab === id;
          return (
            <li key={id}>
              <button
                type="button"
                onClick={() => onChange(id)}
                className={cn(
                  "flex w-full flex-col items-center gap-1 px-1 py-2.5 text-[10px] tracking-[0.12em] uppercase transition-colors",
                  active ? "text-foreground" : "text-muted-foreground",
                )}
              >
                <Icon className={cn("size-5 transition-transform duration-200", active && "scale-110")} strokeWidth={active ? 2 : 1.5} />
                <span>{label}</span>
                <span
                  className={cn(
                    "mt-0.5 h-0.5 w-4 bg-foreground transition-opacity duration-200",
                    active ? "opacity-100" : "opacity-0",
                  )}
                />
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
