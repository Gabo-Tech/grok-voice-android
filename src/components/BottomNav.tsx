import { cn } from "@/lib/utils";

export type AppTab = "talk" | "notes" | "calendar" | "reminders" | "more";

const ITEMS: Array<{ id: AppTab; label: string }> = [
  { id: "talk", label: "Talk" },
  { id: "notes", label: "Notes" },
  { id: "calendar", label: "Calendar" },
  { id: "reminders", label: "Remind" },
  { id: "more", label: "More" },
];

export function BottomNav({ tab, onChange }: { tab: AppTab; onChange: (t: AppTab) => void }) {
  return (
    <nav className="bottom-nav">
      <ul className="grid grid-cols-5">
        {ITEMS.map(({ id, label }) => {
          const active = tab === id;
          return (
            <li key={id}>
              <button
                type="button"
                onClick={() => onChange(id)}
                className={cn(
                  "flex w-full flex-col items-center px-1 py-3 text-[11px] leading-4 tracking-[0.14em] uppercase",
                  active ? "text-foreground" : "text-muted-foreground",
                )}
              >
                {label}
                <span className={cn("mt-1 h-px w-5", active ? "bg-foreground" : "bg-transparent")} />
              </button>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
