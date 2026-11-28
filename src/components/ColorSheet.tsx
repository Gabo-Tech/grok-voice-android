import { useState, type ReactNode } from "react";
import { SERIES_HEX, SERIES_TOKEN, tokenToHex } from "@/lib/entityColor";
import { fill, useT } from "@/lib/i18n";
import { formatMoney } from "@/lib/money";

export interface ColorSeries {
  id: string;
  label: string;
  color: string;
  amountMinor?: number | null;
  group?: string;
}

export function ColorEditor({
  name,
  color,
  onChange,
}: {
  name: string;
  color: string;
  onChange: (color: string) => void;
}) {
  const t = useT();
  const current = tokenToHex(color);
  return (
    <div className="color-field" data-swipe-ignore="">
      <p className="text-sm leading-5 text-muted-foreground">{t("Color")}</p>
      <div className="color-swatches" role="group" aria-label={t("Color")}>
        {SERIES_TOKEN.map((swatch, index) => (
          <button
            key={swatch}
            type="button"
            className="size-12 shrink-0"
            style={{ background: swatch, borderRadius: "var(--radius-xs)", boxShadow: current === SERIES_HEX[index] ? "inset 0 0 0 2px var(--surface-0), 0 0 0 1px var(--brand)" : undefined }}
            aria-label={fill("Preset {n} for {name}", { n: String(index + 1), name })}
            aria-pressed={current === SERIES_HEX[index]}
            onClick={() => onChange(SERIES_HEX[index])}
          />
        ))}
        <label className="relative grid size-12 shrink-0 place-items-center" style={{ borderRadius: "var(--radius-xs)" }}>
          <span className="size-6" style={{ background: color || current, borderRadius: "var(--radius-xs)" }} aria-hidden="true" />
          <input
            type="color"
            aria-label={fill("{name} color", { name })}
            value={current}
            onChange={(event) => onChange(event.target.value)}
            className="absolute inset-0 size-12 cursor-pointer opacity-0"
          />
        </label>
      </div>
      <button type="button" className="add-row" onClick={() => onChange("")}>{t("Reset color")}</button>
    </div>
  );
}

export function ColorSheet({
  series,
  currency,
  groups,
  onPaint,
  onResetAll,
  onClose,
  initialId = "",
}: {
  series: ColorSeries[];
  currency: string;
  groups?: Array<{ id: string; label: string }>;
  onPaint: (id: string, color: string) => void;
  onResetAll?: () => void;
  onClose: () => void;
  initialId?: string;
}) {
  const t = useT();
  const [confirmAll, setConfirmAll] = useState(false);
  const [openId, setOpenId] = useState(initialId);
  const sections = groups && groups.length ? groups : [{ id: "", label: "" }];
  return (
    <div className="flow-sheet mt-2" role="dialog" aria-label={t("Chart colors")} data-swipe-ignore="">
      <div className="flex items-center justify-between gap-3 border-b border-[var(--line-soft)] pb-2">
        <h2 className="text-[15px] leading-5">{t("Chart colors")}</h2>
        <div className="flex items-center">
          {onResetAll ? (
            confirmAll ? (
              <button type="button" className="h-11 bg-[var(--text-primary)] px-3 text-[15px] text-[var(--surface-0)]" onClick={() => { onResetAll(); setConfirmAll(false); }}>
                {t("Yes, reset all colors")}
              </button>
            ) : (
              <button type="button" className="fin-chip min-h-11" onClick={() => setConfirmAll(true)}>{t("Reset all colors")}</button>
            )
          ) : null}
          <button type="button" className="fin-chip min-h-11" onClick={onClose}>{t("Close")}</button>
        </div>
      </div>
      {sections.map((group) => {
        const members = series.filter((row) => (group.id ? row.group === group.id : true));
        if (!members.length) return null;
        return (
          <section key={group.id || "all"} className="mt-3">
            {group.label ? <h3 className="text-[13px] leading-4 tracking-wide uppercase text-[var(--text-tertiary)]">{group.label}</h3> : null}
            <ul className="mt-1">
              {members.map((row) => {
                const open = openId === row.id;
                return (
                  <li key={row.id} className="border-b border-[var(--line-soft)]">
                    <div className="flex min-h-11 items-center gap-2">
                      <button type="button" className="flex min-h-11 min-w-0 flex-1 items-center gap-2 text-left" aria-expanded={open} onClick={() => setOpenId(open ? "" : row.id)}>
                        <span className="h-6 w-6 shrink-0" style={{ background: row.color, borderRadius: "var(--radius)" }} aria-hidden="true" />
                        <span className="min-w-0 flex-1 truncate text-[15px] leading-5">{row.label}</span>
                        {row.amountMinor != null && currency ? <span className="fin-muted shrink-0 text-[13px] leading-4">{formatMoney(row.amountMinor, currency)}</span> : null}
                      </button>
                      <button type="button" className="fin-chip min-h-11 shrink-0" onClick={() => onPaint(row.id, "")}>{t("Reset")}</button>
                    </div>
                    {open ? <ColorEditor name={row.label} color={row.color} onChange={(next) => onPaint(row.id, next)} /> : null}
                  </li>
                );
              })}
            </ul>
          </section>
        );
      })}
    </div>
  );
}

export function ChartColorMenu({
  series,
  currency,
  onPaint,
  startOpen = false,
  extra,
  leading,
  tools,
}: {
  series: ColorSeries[];
  currency: string;
  onPaint: (id: string, color: string) => void;
  startOpen?: boolean;
  extra?: ReactNode;
  leading?: ReactNode;
  tools?: ReactNode;
}) {
  const t = useT();
  const [panel, setPanel] = useState(false);
  const [open, setOpen] = useState(startOpen);
  return (
    <div className="chart-color-host" data-swipe-ignore="">
      <div className="filter-row">
        {leading ?? <span className="min-w-0 flex-1" />}
        <div className="filter-tools">
          {tools}
          <button type="button" className="icon-hit" aria-expanded={panel} aria-label={t("Options")} onClick={() => setPanel((value) => !value)}>
            <OptionsIcon />
          </button>
        </div>
      </div>
      {panel ? (
        <div className="mt-1 flex flex-wrap gap-2">
          <button type="button" className="fin-chip min-h-11" onClick={() => { setOpen(true); setPanel(false); }}>{t("Chart colors")}</button>
          {extra}
        </div>
      ) : null}
      {open ? (
        <ColorSheet
          series={series}
          currency={currency}
          onPaint={onPaint}
          onResetAll={() => series.forEach((row) => onPaint(row.id, ""))}
          onClose={() => setOpen(false)}
        />
      ) : null}
    </div>
  );
}

function OptionsIcon() {
  return (
    <svg width="22" height="22" viewBox="0 0 22 22" aria-hidden="true">
      <path d="M4 6h14M4 11h14M4 16h14" fill="none" stroke="currentColor" strokeWidth="1.5" />
      <circle cx="8" cy="6" r="2" fill="var(--surface-0)" stroke="currentColor" strokeWidth="1.5" />
      <circle cx="14" cy="11" r="2" fill="var(--surface-0)" stroke="currentColor" strokeWidth="1.5" />
      <circle cx="9" cy="16" r="2" fill="var(--surface-0)" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  );
}
