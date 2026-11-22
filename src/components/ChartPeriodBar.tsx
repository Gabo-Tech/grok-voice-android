import { useEffect, useMemo, useState } from "react";
import { boundsOf, emptyPeriod, periodFromVoice, readPeriod, writePeriod, type ChartPeriod, type PeriodPreset } from "@/lib/chartPeriod";
import { useT } from "@/lib/i18n";

const PRESETS: Array<{ id: PeriodPreset; label: string }> = [
  { id: "this", label: "This month" },
  { id: "last", label: "Last month" },
  { id: "3m", label: "3M" },
  { id: "6m", label: "6M" },
  { id: "ytd", label: "YTD" },
  { id: "1y", label: "1Y" },
  { id: "all", label: "All" },
  { id: "custom", label: "Custom" },
];

export function useChartPeriod(chartId: string, fallback: PeriodPreset = "this") {
  const [period, setPeriod] = useState<ChartPeriod>(() => readPeriod(chartId, fallback));
  useEffect(() => {
    writePeriod(chartId, period);
  }, [chartId, period]);
  useEffect(() => {
    const onVoice = (event: Event) => {
      const detail = (event as CustomEvent<{ chart?: string; period?: string; start?: string; end?: string }>).detail || {};
      const chart = (detail.chart || "").trim().toLowerCase();
      if (chart && chart !== chartId && chart !== "all") return;
      const preset = periodFromVoice(detail.period || "");
      if (!preset && !detail.start && !detail.end) return;
      setPeriod({
        preset: preset || "custom",
        start: detail.start || "",
        end: detail.end || "",
      });
    };
    window.addEventListener("grok-finance-voice", onVoice);
    return () => window.removeEventListener("grok-finance-voice", onVoice);
  }, [chartId]);
  const bounds = useMemo(() => boundsOf(period), [period]);
  return { period, setPeriod, bounds };
}

export function ChartPeriodBar({
  period,
  onChange,
}: {
  period: ChartPeriod;
  onChange: (period: ChartPeriod) => void;
}) {
  const t = useT();
  return (
    <div className="mt-2" data-swipe-ignore="">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1" role="group" aria-label={t("Period")}>
        {PRESETS.map((item) => (
          <button key={item.id} type="button" className="fin-chip" aria-pressed={period.preset === item.id} onClick={() => onChange(item.id === "custom" ? { ...period, preset: "custom" } : emptyPeriod(item.id))}>
            {t(item.label)}
          </button>
        ))}
      </div>
      {period.preset === "custom" ? (
        <div className="mt-2 flex flex-wrap gap-2">
          <label className="text-[13px] leading-4">
            {t("From")}
            <input type="date" className="mt-1 block h-11 min-w-0 border border-[var(--fin-line)] bg-transparent px-2 text-[15px]" value={period.start} onChange={(event) => onChange({ ...period, preset: "custom", start: event.target.value })} />
          </label>
          <label className="text-[13px] leading-4">
            {t("To")}
            <input type="date" className="mt-1 block h-11 min-w-0 border border-[var(--fin-line)] bg-transparent px-2 text-[15px]" value={period.end} onChange={(event) => onChange({ ...period, preset: "custom", end: event.target.value })} />
          </label>
        </div>
      ) : null}
    </div>
  );
}
