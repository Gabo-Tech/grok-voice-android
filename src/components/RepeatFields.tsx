import { Field } from "@/components/Field";
import { Input } from "@/components/ui/input";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

export const WEEKDAYS = [
  { id: 1, label: "Mon" },
  { id: 2, label: "Tue" },
  { id: 3, label: "Wed" },
  { id: 4, label: "Thu" },
  { id: 5, label: "Fri" },
  { id: 6, label: "Sat" },
  { id: 7, label: "Sun" },
] as const;

export type RepeatChoice = "none" | "daily" | "weekly" | "monthly" | "yearly" | "custom";
export type RepeatUnit = "days" | "weeks" | "months" | "years";

const CHOICES: RepeatChoice[] = ["none", "daily", "weekly", "monthly", "yearly"];

function choiceLabel(choice: RepeatChoice, t: (text: string) => string) {
  if (choice === "none") return t("Once");
  if (choice === "daily") return t("Daily");
  if (choice === "weekly") return t("Weekly");
  if (choice === "monthly") return t("Monthly");
  return t("Yearly");
}

export function RepeatFields({
  repeat,
  weekdays,
  until,
  onRepeat,
  onWeekdays,
  onUntil,
  interval,
  unit,
  endCount,
  onInterval,
  onUnit,
  onEndCount,
}: {
  repeat: RepeatChoice;
  weekdays: number[];
  until: string;
  onRepeat: (repeat: RepeatChoice) => void;
  onWeekdays: (weekdays: number[]) => void;
  onUntil: (until: string) => void;
  interval?: number;
  unit?: RepeatUnit;
  endCount?: string;
  onInterval?: (interval: number) => void;
  onUnit?: (unit: RepeatUnit) => void;
  onEndCount?: (endCount: string) => void;
}) {
  const t = useT();
  const repeating = repeat !== "none";
  const counted = Boolean(endCount);
  const units: Array<{ id: RepeatUnit; label: string }> = [
    { id: "days", label: "Days" },
    { id: "weeks", label: "Weeks" },
    { id: "months", label: "Months" },
    { id: "years", label: "Years" },
  ];
  return (
    <div className="space-y-4">
      <div>
        <p className="mb-2 text-sm text-muted-foreground" id="repeat-label">
          {t("Repeats")}
        </p>
        <div className="grid grid-cols-2 gap-2" role="group" aria-labelledby="repeat-label">
          {CHOICES.map((choice) => {
            const selected = repeat === choice;
            return (
              <button
                key={choice}
                type="button"
                aria-pressed={selected}
                onClick={() => {
                  onRepeat(choice);
                  if (choice === "weekly" && weekdays.length === 0) {
                    const day = new Date().getDay();
                    onWeekdays([day === 0 ? 7 : day]);
                  }
                }}
                className={cn(
                  "h-14 border px-2 text-sm",
                  selected ? "border-2 border-foreground font-medium text-foreground" : "border-border text-muted-foreground",
                )}
              >
                {choiceLabel(choice, t)}
              </button>
            );
          })}
          {onInterval && (
            <button
              type="button"
              aria-pressed={repeat === "custom"}
              onClick={() => onRepeat("custom")}
              className={cn(
                "h-14 border px-2 text-sm",
                repeat === "custom" ? "border-2 border-foreground font-medium text-foreground" : "border-border text-muted-foreground",
              )}
            >
              {t("Custom")}
            </button>
          )}
        </div>
      </div>
      {repeat === "custom" && onInterval && onUnit && (
        <div>
          <p className="mb-2 text-sm text-muted-foreground">{t("Repeat every")}</p>
          <div className="mb-2 grid grid-cols-4 gap-2">
            {units.map((choice) => (
              <button
                key={choice.id}
                type="button"
                aria-pressed={unit === choice.id}
                onClick={() => {
                  onUnit(choice.id);
                  if (choice.id === "weeks" && weekdays.length === 0) {
                    const day = new Date().getDay();
                    onWeekdays([day === 0 ? 7 : day]);
                  }
                }}
                className={cn(
                  "h-12 border px-1 text-sm",
                  unit === choice.id ? "border-2 border-foreground font-medium text-foreground" : "border-border text-muted-foreground",
                )}
              >
                {t(choice.label)}
              </button>
            ))}
          </div>
          <Input
            type="number"
            min={1}
            inputMode="numeric"
            aria-label={t("Repeat every")}
            value={interval ?? 2}
            onChange={(event) => onInterval(Math.max(1, Number(event.target.value) || 1))}
          />
        </div>
      )}
      {(repeat === "weekly" || (repeat === "custom" && unit === "weeks")) && (
        <div className="grid grid-cols-7 gap-1" role="group" aria-label={t("Weekly")}>
          {WEEKDAYS.map((day) => {
            const on = weekdays.includes(day.id);
            return (
              <button
                key={day.id}
                type="button"
                aria-pressed={on}
                onClick={() => onWeekdays(on ? weekdays.filter((value) => value !== day.id) : [...weekdays, day.id].sort())}
                className={cn(
                  "h-11 min-w-0 border px-0 text-[13px]",
                  on ? "border-2 border-foreground font-medium text-foreground" : "border-border text-muted-foreground",
                )}
              >
                {t(day.label)}
              </button>
            );
          })}
        </div>
      )}
      {repeating && (
        <Field label={t("Ends")} hint={until ? undefined : t("Never")}>
          <div className="mb-2 grid grid-cols-2 gap-2">
            <button
              type="button"
              aria-pressed={!until && !counted}
              onClick={() => {
                onUntil("");
                onEndCount?.("");
              }}
              className={cn(
                "h-12 border text-sm",
                !until && !counted ? "border-2 border-foreground font-medium text-foreground" : "border-border text-muted-foreground",
              )}
            >
              {t("Never")}
            </button>
            <button
              type="button"
              aria-pressed={Boolean(until)}
              onClick={() => {
                onEndCount?.("");
                if (!until) {
                  const date = new Date();
                  const pad = (n: number) => String(n).padStart(2, "0");
                  onUntil(`${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`);
                }
              }}
              className={cn(
                "h-12 border text-sm",
                until ? "border-2 border-foreground font-medium text-foreground" : "border-border text-muted-foreground",
              )}
            >
              {t("End date")}
            </button>
            {onEndCount && (
              <button
                type="button"
                aria-pressed={counted}
                onClick={() => {
                  onUntil("");
                  if (!endCount) onEndCount("10");
                }}
                className={cn(
                  "col-span-2 h-12 border text-sm",
                  counted ? "border-2 border-foreground font-medium text-foreground" : "border-border text-muted-foreground",
                )}
              >
                {t("Stop after")}
              </button>
            )}
          </div>
          {until && <Input type="date" aria-label={t("End date")} value={until} onChange={(e) => onUntil(e.target.value)} />}
          {counted && onEndCount && (
            <Input
              type="number"
              min={1}
              inputMode="numeric"
              aria-label={t("How many times")}
              value={endCount}
              onChange={(event) => onEndCount(String(Math.max(1, Number(event.target.value) || 1)))}
            />
          )}
        </Field>
      )}
    </div>
  );
}

export function endOfDay(isoDate: string): number {
  const [year, month, day] = isoDate.split("-").map(Number);
  if (!year || !month || !day) return 0;
  return new Date(year, month - 1, day, 23, 59, 0, 0).getTime();
}

export function untilInput(untilMs?: number): string {
  if (!untilMs) return "";
  const date = new Date(untilMs);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}
