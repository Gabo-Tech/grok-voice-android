import { useEffect, useState, type ReactNode } from "react";
import { nextHidden, readHidden, writeHidden, type ChartItem } from "@/lib/chartPrefs";
import { legendShares } from "@/lib/legendShare";
import { compactMoney, formatMoney } from "@/lib/money";
import { activeLocale, fill, useT } from "@/lib/i18n";

export interface LegendRow {
  id: string;
  label: string;
  color: string;
  amountMinor: number | null;
  currency: string;
  original?: string;
  note?: string;
  className?: string;
  unpriced?: boolean;
  /** Shown in the list, left out of the percentage. Debts must not take a share of a net-worth total. */
  omitShare?: boolean;
  /** The total row. It shows 100% and is not added into the denominator. */
  whole?: boolean;
}

export function useChartVisibility(chartId: string, items: ChartItem[]) {
  const [hidden, setHidden] = useState<string[]>(() => readHidden(chartId));
  const [picked, setPicked] = useState("");
  useEffect(() => {
    writeHidden(chartId, hidden);
  }, [chartId, hidden]);
  useEffect(() => {
    const onVoice = (event: Event) => {
      const detail = (event as CustomEvent<{ chart?: string; action?: string; name?: string }>).detail || {};
      if (!detail.action || detail.chart !== chartId) return;
      setHidden((current) => nextHidden(items, current, detail.action || "", detail.name || "").hidden);
    };
    window.addEventListener("grok-finance-voice", onVoice);
    return () => window.removeEventListener("grok-finance-voice", onVoice);
  }, [chartId, items]);
  const hiddenSet = new Set(hidden);
  return {
    hidden: hiddenSet,
    anyHidden: hidden.length > 0,
    picked,
    setPicked,
    toggle: (id: string) => setHidden((current) => (current.includes(id) ? current.filter((row) => row !== id) : [...current, id])),
    showAll: () => setHidden([]),
  };
}

export function ChartCard({
  title,
  kicker,
  action,
  children,
}: {
  title: string;
  kicker?: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="fin-panel fin-card">
      <div>
        {kicker ? <p className="fin-kicker">{kicker}</p> : null}
        <div className="title-line">
          <h3 className={kicker ? "text-base leading-5" : "fin-kicker"}>{title}</h3>
          {action}
        </div>
      </div>
      <div className="mt-2 min-w-0">{children}</div>
    </section>
  );
}

export function PaletteMenu({ value, onChange }: { value: string; onChange: (id: string) => void }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const options = [
    { id: "okabe", label: t("Clear palette") },
    { id: "dusk", label: t("Dusk palette") },
    { id: "ink", label: t("Ink palette") },
  ];
  return (
    <div className="relative">
      <button type="button" className="fin-icon" aria-label={t("Palette")} aria-expanded={open} onClick={() => setOpen((current) => !current)}>
        <span className="fin-swatch" style={{ background: "var(--fin-gold)" }} />
      </button>
      {open ? (
        <div className="fin-menu" role="menu">
          {options.map((option) => (
            <button
              key={option.id}
              type="button"
              role="menuitemradio"
              aria-checked={value === option.id}
              className="fin-menu-item"
              onClick={() => {
                onChange(option.id);
                setOpen(false);
              }}
            >
              {option.label}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

export function Legend({
  rows,
  hidden,
  currency,
  onToggle,
  onShowAll,
  anyHidden,
  onColor,
  colors,
  painting,
  setPainting,
  onUnpriced,
}: {
  rows: LegendRow[];
  hidden: Set<string>;
  currency: string;
  onToggle: (id: string) => void;
  onShowAll: () => void;
  anyHidden: boolean;
  onColor?: (id: string, color: string) => void;
  colors?: string[];
  painting?: string;
  setPainting?: (id: string) => void;
  onUnpriced?: (id: string) => void;
}) {
  const t = useT();
  const shares = legendShares(
    rows.map((row) => ({
      id: row.id,
      amount: row.amountMinor,
      whole: row.whole,
      omit: row.omitShare,
      hidden: hidden.has(row.id),
      unpriced: row.unpriced,
    })),
  );
  return (
    <div className="mt-2">
      <ul className="space-y-1">
        {rows.map((row) => {
          const off = hidden.has(row.id);
          const rawShare = shares.get(row.id);
          const share = rawShare == null ? null : rawShare > 0 && rawShare < 0.1 ? "<0.1" : rawShare;
          return (
            <li key={row.id} className={off ? "opacity-40 transition-opacity" : "transition-opacity"}>
              <div className="flex min-h-11 items-center gap-1">
                {onColor && setPainting ? (
                  <button
                    type="button"
                    className="fin-icon shrink-0"
                    aria-label={fill("Color for {name}", { name: row.label })}
                    onClick={() => setPainting(painting === row.id ? "" : row.id)}
                  >
                    <span className="fin-swatch" style={{ background: row.color }} />
                  </button>
                ) : (
                  <span className="fin-icon shrink-0" aria-hidden>
                    <span className="fin-swatch" style={{ background: row.color }} />
                  </span>
                )}
                <div className="min-w-0 flex-1">
                <button type="button" className="flex min-h-11 w-full min-w-0 items-center gap-2 text-left" aria-pressed={!off} onClick={() => onToggle(row.id)}>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[15px] leading-5">{row.label}</span>
                    {row.original ? <span className="fin-muted block text-[13px] leading-4">{row.original}</span> : null}
                    {row.note && !row.unpriced ? <span className="fin-muted block text-[13px] leading-4">{row.note}</span> : null}
                  </span>
                  <span className="shrink-0 text-right">
                    {row.unpriced || row.amountMinor == null ? null : (
                      <>
                        <span className="fin-num block text-[15px] leading-5">{formatMoney(row.amountMinor, row.currency || currency)}</span>
                        {share != null ? <span className="fin-muted block text-[13px] leading-4">{share}%</span> : null}
                      </>
                    )}
                  </span>
                </button>
                {row.unpriced ? (
                  <button
                    type="button"
                    className="fin-muted block min-h-11 text-left text-[13px] leading-4 underline"
                    onClick={() => (onUnpriced ? onUnpriced(row.id) : onToggle(row.id))}
                  >
                    {t("No price")}
                  </button>
                ) : null}
                </div>
              </div>
              {painting === row.id && colors && onColor ? (
                <div className="mb-1 flex flex-wrap gap-1 pl-1">
                  {colors.map((swatch) => (
                    <button key={swatch} type="button" className="fin-icon" aria-label={swatch} onClick={() => onColor(row.id, swatch)}>
                      <span className="fin-swatch" style={{ background: swatch }} />
                    </button>
                  ))}
                </div>
              ) : null}
            </li>
          );
        })}
      </ul>
      {anyHidden ? (
        <button type="button" className="fin-textbtn mt-1" onClick={onShowAll}>
          {t("Show all")}
        </button>
      ) : null}
    </div>
  );
}

export function Donut({
  rows,
  hidden,
  center,
  caption,
  debtRatio,
}: {
  rows: Array<{ id: string; label: string; color: string; amountMinor: number | null }>;
  hidden: Set<string>;
  center: string;
  caption?: string;
  /** Share of assets that debts represent. Drawn as a rose ring outside the asset donut. */
  debtRatio?: number;
}) {
  const slices = rows.filter((row) => !hidden.has(row.id) && (row.amountMinor || 0) > 0);
  const total = slices.reduce((sum, row) => sum + (row.amountMinor || 0), 0) || 1;
  let angle = -90;
  const paths = slices.map((slice) => {
    const sweep = ((slice.amountMinor || 0) / total) * 360;
    const path = arc(100, 100, 72, 46, angle, angle + Math.max(sweep, 0.8));
    angle += sweep;
    return { ...slice, path };
  });
  const ratio = debtRatio != null && debtRatio > 0 ? Math.min(debtRatio, 1) : 0;
  const label = [caption, center].filter(Boolean).join(" ") || rows.map((row) => row.label).filter(Boolean).join(", ");
  const hole = 76;
  const textWidth = (text: string, size: number) => [...text].length * size * 0.56;
  const captionSize = 11;
  const showCaption = caption != null && caption.length > 0 && textWidth(caption, captionSize) <= hole;
  const centerSize = Math.min(22, hole / Math.max([...center].length * 0.56, 1));
  return (
    <div className="relative mx-auto aspect-square w-full max-w-[220px]">
      <svg viewBox="0 0 200 200" className="block h-full w-full" role="img" aria-label={label} data-fit="amount">
        {ratio > 0 ? (
          <>
            <path d={arc(100, 100, 94, 82, -90, 269.2)} fill="var(--fin-line)" />
            <path d={arc(100, 100, 94, 82, -90, -90 + Math.max(ratio * 360, 1.2))} fill="var(--rose)" />
          </>
        ) : null}
        {paths.map((slice) => (
          <path key={slice.id} d={slice.path} fill={slice.color} className="fin-slice">
            <title>{slice.label}</title>
          </path>
        ))}
        {showCaption ? (
          <text x="100" y="86" textAnchor="middle" dominantBaseline="middle" fontSize={captionSize} fill="currentColor">
            {caption}
          </text>
        ) : null}
        {center ? (
          <text x="100" y={showCaption ? 112 : 100} textAnchor="middle" dominantBaseline="middle" fontSize={centerSize} fontWeight="500" fill="currentColor">
            {center}
          </text>
        ) : null}
      </svg>
    </div>
  );
}

export interface LineSeries {
  id: string;
  color: string;
  points: Array<{ date: string; valueMinor: number }>;
}

export function LineChart({
  series,
  currency,
  partial,
  onPick,
}: {
  series: LineSeries[];
  currency: string;
  partial?: string;
  onPick?: (date: string) => void;
}) {
  const t = useT();
  const locale = activeLocale();
  const dates = [...new Set(series.flatMap((row) => row.points.map((point) => point.date)))].sort();
  const values = series.flatMap((row) => row.points.map((point) => point.valueMinor));
  const max = Math.max(...values, 0);
  const min = Math.min(...values, 0);
  const span = max - min || 1;
  const left = 8;
  const right = 8;
  const top = 12;
  const bottom = 8;
  const width = 320;
  const height = 148;
  const innerW = width - left - right;
  const innerH = height - top - bottom;
  const xOf = (date: string) => {
    const index = Math.max(0, dates.indexOf(date));
    if (dates.length <= 1) return left + innerW / 2;
    return left + (index / (dates.length - 1)) * innerW;
  };
  const yOf = (value: number) => top + ((max - value) / span) * innerH;
  const ticks = [max, min + span / 2, min].filter((value, index, all) => all.findIndex((item) => Math.abs(item - value) < span * 0.05) === index);
  const labelStep = Math.max(1, Math.ceil(dates.length / 4));
  return (
    <div className="mt-2">
    <div className="flex items-stretch gap-2">
      <div className="flex w-11 shrink-0 flex-col justify-between py-1 text-right text-[13px] leading-4 tabular-nums">
        {ticks.map((tick) => (
          <span key={tick}>{compactMoney(Math.round(tick), currency)}</span>
        ))}
      </div>
      <svg viewBox={`0 0 ${width} ${height}`} className="block h-auto min-w-0 flex-1" role="img" aria-label={t("Portfolio value")}>
        {ticks.map((tick) => (
          <line key={tick} x1={left} x2={width - right} y1={yOf(tick)} y2={yOf(tick)} stroke="currentColor" strokeOpacity="0.18" />
        ))}
        {series.map((row) => {
          const ordered = [...row.points].sort((a, b) => a.date.localeCompare(b.date));
          if (ordered.length === 0) return null;
          const d = ordered.map((point, index) => `${index === 0 ? "M" : "L"} ${xOf(point.date)} ${yOf(point.valueMinor)}`).join(" ");
          return <path key={row.id} d={d} fill="none" stroke={row.color} strokeWidth="2.5" />;
        })}
      </svg>
    </div>
    <div className="relative ml-12 h-11">
      {dates.map((date, index) =>
        index % labelStep === 0 || index === dates.length - 1 ? (
          <button
            key={date}
            type="button"
            className="absolute min-h-11 -translate-x-1/2 px-1 text-[13px]"
            style={{ left: `${(xOf(date) / width) * 100}%` }}
            onClick={() => onPick?.(date)}
          >
            {monthName(date, locale)}
            {partial && date.startsWith(partial) ? " ·" : ""}
          </button>
        ) : null,
      )}
    </div>
    </div>
  );
}

export function monthName(date: string, locale: string): string {
  const [year, month] = date.split("-").map(Number);
  if (!year || !month) return date;
  try {
    return new Intl.DateTimeFormat(locale === "ca-ES-valencia" ? "ca-ES" : locale, { month: "short" }).format(new Date(year, month - 1, 1));
  } catch {
    return date.slice(5, 7);
  }
}

function arc(cx: number, cy: number, outer: number, inner: number, start: number, end: number): string {
  const sweep = Math.min(end - start, 359.8);
  const large = sweep > 180 ? 1 : 0;
  const point = (radius: number, degrees: number) => {
    const radians = (degrees * Math.PI) / 180;
    return { x: cx + radius * Math.cos(radians), y: cy + radius * Math.sin(radians) };
  };
  const a = point(outer, start);
  const b = point(outer, start + sweep);
  const c = point(inner, start + sweep);
  const d = point(inner, start);
  return `M ${a.x} ${a.y} A ${outer} ${outer} 0 ${large} 1 ${b.x} ${b.y} L ${c.x} ${c.y} A ${inner} ${inner} 0 ${large} 0 ${d.x} ${d.y} Z`;
}
