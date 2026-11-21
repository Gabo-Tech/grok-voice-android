import { useEffect, useMemo, useState } from "react";
import type { FinanceAccount, FinanceFx, FinancePrices } from "@/lib/api";
import {
  ALLOCATION_PRESETS,
  allocation,
  assetClassOf,
  persistedColor,
  positionOf,
  type AllocGroup,
  type AllocSlice,
} from "@/lib/financeLedger";
import { bookFromStatus } from "@/lib/financeMath";
import { formatMoney } from "@/lib/money";
import { fill, useT } from "@/lib/i18n";

const PREF_KEY = "grok-finance-allocation";

function readPref(): { group: AllocGroup; cash: boolean; preset: string; expanded: boolean } {
  const fallback = { group: "item" as AllocGroup, cash: false, preset: "okabe", expanded: false };
  try {
    const raw = localStorage.getItem(PREF_KEY);
    if (!raw) return fallback;
    const parsed = JSON.parse(raw) as Partial<typeof fallback>;
    return {
      group: parsed.group === "class" ? "class" : "item",
      cash: Boolean(parsed.cash),
      preset: parsed.preset && ALLOCATION_PRESETS[parsed.preset] ? parsed.preset : "okabe",
      expanded: Boolean(parsed.expanded),
    };
  } catch {
    return fallback;
  }
}

export function HoldingsChart({
  accounts,
  display,
  fx,
  prices,
  onRecolor,
}: {
  accounts: FinanceAccount[];
  display: string;
  fx?: FinanceFx | null;
  prices?: FinancePrices | null;
  onRecolor: (account: FinanceAccount, color: string) => void;
}) {
  const t = useT();
  const [pref, setPref] = useState(readPref);
  const [selected, setSelected] = useState<string>("");
  const [painting, setPainting] = useState("");
  const reduced = useReducedMotion();
  const book = useMemo(() => bookFromStatus(fx), [fx]);
  const nowMs = useMemo(() => Date.now(), [accounts, fx, prices, display]);
  const quotes = prices?.quotes || [];
  const rows = useMemo(
    () =>
      accounts.filter((account) => account.kind !== "credit_card" && account.kind !== "loan" && account.kind !== "other_debt").map((account) => {
        const position = positionOf(account, display, book, quotes, nowMs);
        const symbolKind = position.symbol === "XAG" || position.symbol === "XAU" || position.symbol === "XPT" || position.symbol === "XPD" ? "metal" : position.symbol ? "crypto" : "";
        return {
          id: account.id,
          name: account.name,
          kind: account.kind,
          assetClass: assetClassOf(account.kind, account.investmentType || "", symbolKind),
          color: account.color,
          amountMinor: position.amountMinor,
          unpriced: position.unpriced,
          quantity: position.quantity,
          source: position.estimated ? "estimate" : position.source,
          fetchedMs: position.fetchedMs,
          stale: position.stale,
          estimated: position.estimated,
          cash: account.kind === "cash" || account.kind === "bank",
        };
      }),
    [accounts, display, book, quotes, nowMs],
  );
  const pie = useMemo(
    () =>
      allocation(rows, {
        group: pref.group,
        includeCash: pref.cash,
        palette: ALLOCATION_PRESETS[pref.preset],
        expanded: pref.expanded,
      }),
    [rows, pref],
  );
  const save = (next: typeof pref) => {
    setPref(next);
    try {
      localStorage.setItem(PREF_KEY, JSON.stringify(next));
    } catch {
      /* The choice still applies for this visit. */
    }
  };
  const chosen = pie.slices.find((slice) => slice.id === selected) || null;
  const totalLabel = formatMoney(pie.total, display);

  return (
    <section className="fin-panel" data-finance-chart="holdings">
      <h2 className="fin-kicker">{t("Holdings chart")}</h2>
      <p className="fin-muted mt-2 text-sm leading-5">{fill("By value in {code}.", { code: display || "—" })}</p>
      <div className="mt-3 flex flex-wrap gap-2">
        <button type="button" className="fin-chip" aria-pressed={pref.group === "item"} onClick={() => save({ ...pref, group: "item" })}>
          {t("By item")}
        </button>
        <button type="button" className="fin-chip" aria-pressed={pref.group === "class"} onClick={() => save({ ...pref, group: "class" })}>
          {t("By class")}
        </button>
        <button type="button" className="fin-chip" aria-pressed={pref.cash} onClick={() => save({ ...pref, cash: !pref.cash })}>
          {t("Include cash")}
        </button>
      </div>
      {pie.slices.length === 0 ? (
        <p className="mt-4 text-base">{t("Nothing priced yet")}</p>
      ) : (
        <AllocationDonut slices={pie.slices} center={totalLabel} reduced={reduced} onPick={setSelected} />
      )}
      {chosen ? (
        <div className="mt-4 border border-border px-3 py-3">
          <p className="text-base font-medium">{sliceName(chosen, t)}</p>
          <p className="fin-num mt-1 text-2xl">{formatMoney(chosen.amountMinor, display)}</p>
          <p className="fin-muted mt-1 text-sm leading-5">
            {Math.round(chosen.share * 1000) / 10}%
            {chosen.quantity ? ` · ${chosen.quantity}` : ""}
            {chosen.source ? ` · ${chosen.estimated ? t("Your estimate") : chosen.source}` : ""}
            {chosen.stale ? ` · ${t("Rates are stale")}` : ""}
          </p>
          {chosen.other ? (
            <ul className="mt-2 space-y-1">
              {chosen.members.map((member) => (
                <li key={member.id} className="text-sm">
                  {member.name} · {formatMoney(member.amountMinor, display)}
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}
      <ul className="mt-4 space-y-2">
        {pie.slices.map((slice) => (
          <li key={slice.id}>
            <button type="button" className="flex min-h-12 w-full items-center gap-3 text-left" onClick={() => setSelected(slice.id)}>
              <span className="fin-swatch shrink-0" style={{ background: slice.color }} />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-base">{sliceName(slice, t)}</span>
                <span className="fin-muted block text-sm">{formatMoney(slice.amountMinor, display)} · {Math.round(slice.share * 1000) / 10}%</span>
              </span>
            </button>
          </li>
        ))}
      </ul>
      {pie.slices.some((slice) => slice.other) || pref.expanded ? (
        <button type="button" className="fin-chip mt-3" onClick={() => save({ ...pref, expanded: !pref.expanded })}>
          {pref.expanded ? t("Group small slices") : t("Show small slices")}
        </button>
      ) : null}
      {pie.unpriced.length > 0 ? (
        <div className="mt-4">
          <p className="stamp">{t("Not priced")}</p>
          <ul className="mt-2 space-y-1">
            {pie.unpriced.map((row) => (
              <li key={row.id} className="text-sm">
                {row.name}
                {row.quantity ? ` · ${row.quantity}` : ""} · {t("Not priced")}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      <div className="mt-4">
        <p className="stamp">{t("Colors")}</p>
        <div className="mt-2 flex flex-wrap gap-2">
          {Object.keys(ALLOCATION_PRESETS).map((id) => (
            <button key={id} type="button" className="fin-chip" aria-pressed={pref.preset === id} onClick={() => save({ ...pref, preset: id })}>
              {t(presetLabel(id))}
            </button>
          ))}
        </div>
        <div className="mt-3 space-y-2">
          {accounts
            .filter((account) => account.kind === "investment" || account.kind === "other" || account.units)
            .map((account, index) => (
              <div key={account.id} className="flex min-h-12 items-center gap-3">
                <button
                  type="button"
                  className="fin-swatch h-11 w-11 shrink-0"
                  style={{ background: persistedColor(account.color, ALLOCATION_PRESETS[pref.preset][index % ALLOCATION_PRESETS[pref.preset].length]) }}
                  aria-label={fill("Color for {name}", { name: account.name })}
                  onClick={() => setPainting(painting === account.id ? "" : account.id)}
                />
                <span className="min-w-0 flex-1 truncate text-base">{account.name}</span>
                {painting === account.id ? (
                  <span className="flex flex-wrap gap-1">
                    {ALLOCATION_PRESETS[pref.preset].map((swatch) => (
                      <button
                        key={swatch}
                        type="button"
                        className="fin-swatch h-11 w-11"
                        style={{ background: swatch }}
                        aria-label={swatch}
                        onClick={() => {
                          onRecolor(account, swatch);
                          setPainting("");
                        }}
                      />
                    ))}
                  </span>
                ) : null}
              </div>
            ))}
        </div>
      </div>
    </section>
  );
}

function sliceName(slice: AllocSlice, t: (english: string) => string): string {
  if (slice.other) return t("Other");
  if (["Crypto", "Metals", "Stocks", "Manual", "Cash", "Other"].includes(slice.label)) return t(slice.label);
  return slice.label;
}

function presetLabel(id: string): string {
  if (id === "dusk") return "Dusk palette";
  if (id === "ink") return "Ink palette";
  return "Clear palette";
}

function AllocationDonut({
  slices,
  center,
  reduced,
  onPick,
}: {
  slices: AllocSlice[];
  center: string;
  reduced: boolean;
  onPick: (id: string) => void;
}) {
  const total = slices.reduce((sum, slice) => sum + slice.amountMinor, 0) || 1;
  let angle = -90;
  const paths = slices.map((slice) => {
    const sweep = (slice.amountMinor / total) * 360;
    const path = donutArc(120, 120, 86, 52, angle, angle + Math.max(sweep, 0.8));
    angle += sweep;
    return { ...slice, path };
  });
  return (
    <svg viewBox="0 0 240 240" className="mx-auto mt-4 h-56 w-56 max-w-full" role="img" aria-label={center}>
      {paths.map((slice) => (
        <path
          key={slice.id}
          d={slice.path}
          fill={slice.color}
          className={reduced ? undefined : "fin-slice"}
          onClick={() => onPick(slice.id)}
        >
          <title>{slice.label}</title>
        </path>
      ))}
      <text x="120" y="124" textAnchor="middle" fill="currentColor" fontSize={center.length > 14 ? 11 : 16} className="fin-num">
        {center}
      </text>
    </svg>
  );
}

function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(false);
  useEffect(() => {
    const query = window.matchMedia("(prefers-reduced-motion: reduce)");
    const apply = () => setReduced(query.matches);
    apply();
    query.addEventListener("change", apply);
    return () => query.removeEventListener("change", apply);
  }, []);
  return reduced;
}

function donutArc(cx: number, cy: number, outer: number, inner: number, start: number, end: number): string {
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
