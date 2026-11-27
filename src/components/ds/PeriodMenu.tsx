import { useState, type ReactNode } from "react";
import { ChevronDown } from "lucide-react";
import { Sheet } from "@/components/Sheet";
import { useT } from "@/lib/i18n";

/** One "This month ▾" chip. The range list opens in a sheet. */
export function PeriodMenu({
  value,
  options,
  onChange,
  label,
  custom,
}: {
  value: string;
  options: Array<{ id: string; label: string }>;
  onChange: (id: string) => void;
  label?: string;
  custom?: ReactNode;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const current = options.find((item) => item.id === value);
  const caption = label || (current ? t(current.label) : t("This month"));
  return (
    <>
      <button type="button" className="period-chip" aria-haspopup="dialog" aria-expanded={open} onClick={() => setOpen(true)}>
        <span>{caption}</span>
        <ChevronDown className="size-4 shrink-0" aria-hidden />
      </button>
      <Sheet open={open} title={t("Period")} onClose={() => setOpen(false)}>
        <div role="radiogroup" aria-label={t("Period")} className="tune-sheet">
          {options.map((item) => {
            const selected = item.id === value;
            return (
              <button
                key={item.id}
                type="button"
                role="radio"
                aria-checked={selected}
                className="tune-row"
                onClick={() => {
                  onChange(item.id);
                  if (item.id !== "custom") setOpen(false);
                }}
              >
                <span>{t(item.label)}</span>
                {selected ? <span aria-hidden>✓</span> : null}
              </button>
            );
          })}
          {value === "custom" ? custom : null}
        </div>
      </Sheet>
    </>
  );
}
