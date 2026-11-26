import { useMemo, useState } from "react";
import type { FinanceAccount, FinanceFx, FinancePrices } from "@/lib/api";
import { ChartCard, Donut, Legend, LineChart, useChartVisibility, type LegendRow } from "@/components/ChartKit";
import { ChartColorMenu, ColorSheet } from "@/components/ColorSheet";
import { resolveColor, type EntityColor } from "@/lib/entityColor";
import {
  ALLOCATION_PRESETS,
  allocation,
  assetClassOf,
  originalHolding,
  positionOf,
  type AllocGroup,
} from "@/lib/financeLedger";
import { bookFromStatus } from "@/lib/financeMath";
import { formatMoney } from "@/lib/money";
import { ChartPeriodBar, useChartPeriod } from "@/components/ChartPeriodBar";
import { inBounds } from "@/lib/chartPeriod";
import { changeOf, completeSeries, seriesFor, type ValuePoint } from "@/lib/portfolio";
import { fill, useT } from "@/lib/i18n";

const PREF_KEY = "grok-finance-allocation";

function readPref(): { group: AllocGroup; cash: boolean; preset: string } {
  const fallback = { group: "item" as AllocGroup, cash: false, preset: "okabe" };
  try {
    const raw = localStorage.getItem(PREF_KEY);
    if (!raw) return fallback;
    const parsed = JSON.parse(raw) as Partial<typeof fallback>;
    return {
      group: parsed.group === "class" ? "class" : "item",
      cash: Boolean(parsed.cash),
      preset: parsed.preset && ALLOCATION_PRESETS[parsed.preset] ? parsed.preset : "okabe",
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
  history = [],
  onRecolor,
  onPaint,
  entityColors = [],
  openSheet = "",
  onUnpriced,
}: {
  accounts: FinanceAccount[];
  display: string;
  fx?: FinanceFx | null;
  prices?: FinancePrices | null;
  history?: ValuePoint[];
  onRecolor: (account: FinanceAccount, color: string) => void;
  onPaint?: (kind: string, id: string, color: string) => void;
  entityColors?: EntityColor[];
  openSheet?: "" | "donut" | "line" | "holding";
  onUnpriced?: (account: FinanceAccount) => void;
}) {
  const t = useT();
  const [pref, setPref] = useState(readPref);
  const [painting, setPainting] = useState("");
  const [sheet, setSheet] = useState<"" | "donut" | "line" | "holding">(openSheet);
  const portfolioPeriod = useChartPeriod("portfolio", "3m");
  const holdingsPeriod = useChartPeriod("holdings");
  const [focus, setFocus] = useState("");
  const book = useMemo(() => bookFromStatus(fx), [fx]);
  const nowMs = useMemo(() => Date.now(), [accounts, fx, prices, display]);
  const quotes = prices?.quotes || [];
  const code = display.trim().toUpperCase();
  const rows = useMemo(
    () =>
      accounts
        .filter((account) => account.kind !== "credit_card" && account.kind !== "loan" && account.kind !== "other_debt")
        .map((account) => {
          const position = positionOf(account, code, book, quotes, nowMs);
          const symbolKind = position.symbol === "XAG" || position.symbol === "XAU" || position.symbol === "XPT" || position.symbol === "XPD" ? "metal" : position.symbol ? "crypto" : "";
          const original = originalHolding(account, position, code);
          return {
            account,
            id: account.id,
            name: account.name,
            kind: account.kind,
            assetClass: assetClassOf(account.kind, account.investmentType || "", symbolKind),
            color: resolveColor(entityColors, "account", account.id, account.color),
            amountMinor: position.amountMinor,
            unpriced: position.unpriced,
            quantity: original.quantity,
            native: original.nativeMinor != null && original.nativeCurrency ? formatMoney(original.nativeMinor, original.nativeCurrency) : "",
            source: position.estimated ? "estimate" : position.source,
            fetchedMs: position.fetchedMs,
            stale: position.stale,
            estimated: position.estimated,
            cash: account.kind === "cash" || account.kind === "bank",
          };
        }),
    [accounts, code, book, quotes, nowMs],
  );
  const pie = useMemo(
    () => allocation(rows, { group: pref.group, includeCash: pref.cash, palette: ALLOCATION_PRESETS[pref.preset], expanded: true }),
    [rows, pref],
  );
  const legend: LegendRow[] = [
    ...pie.slices.map((slice) => ({
      id: slice.id,
      label: sliceLabel(slice.label, slice.other, t),
      color: pref.group === "class" ? resolveColor(entityColors, "class", slice.label, "") : resolveColor(entityColors, "account", slice.members[0]?.id || slice.id, slice.members[0]?.color || ""),
      amountMinor: slice.amountMinor,
      currency: code,
      original: [slice.quantity, slice.members.length === 1 ? rows.find((row) => row.id === slice.members[0].id)?.native || "" : ""].filter(Boolean).join(" · "),
      className: slice.members.length === 1 ? slice.members[0].assetClass : slice.label,
    })),
    ...pie.unpriced.map((row) => ({
      id: row.id,
      label: row.name,
      color: resolveColor(entityColors, "account", row.id, row.color),
      amountMinor: null,
      currency: code,
      original: row.quantity,
      className: row.assetClass,
      unpriced: true,
    })),
  ];
  const vis = useChartVisibility("holdings", legend.map((row) => ({ id: row.id, label: row.label, className: row.className })));
  const visibleTotal = legend.reduce((sum, row) => (vis.hidden.has(row.id) || row.unpriced || row.amountMinor == null ? sum : sum + row.amountMinor), 0);
  const save = (next: typeof pref) => {
    setPref(next);
    try {
      localStorage.setItem(PREF_KEY, JSON.stringify(next));
    } catch {
      /* The choice still applies for this visit. */
    }
  };
  const historyIn = history.filter((point) => inBounds(point.date, portfolioPeriod.bounds.start, portfolioPeriod.bounds.end));
  const from = "";
  const portfolioItems = useMemo(() => {
    const names = new Map<string, { name: string; className: string }>();
    for (const point of history) names.set(point.accountId, { name: point.name, className: point.className || "" });
    return [...names.entries()].map(([id, row]) => ({ id, label: row.name, className: row.className }));
  }, [history]);
  const portVis = useChartVisibility("portfolio", [{ id: "total", label: t("Portfolio value"), className: "total" }, ...portfolioItems]);
  const shownIds = portfolioItems.filter((item) => !portVis.hidden.has(item.id)).map((item) => item.id);
  const portfolioIds = portfolioItems.map((item) => item.id);
  const totalSeries = completeSeries(historyIn, portfolioIds, from);
  const change = changeOf(totalSeries);
  const lineSeries = [
    portVis.hidden.has("total") ? null : { id: "total", color: resolveColor(entityColors, "series", "portfolio:total", ""), points: totalSeries },
    ...shownIds.map((id) => ({
      id,
      color: resolveColor(entityColors, "account", id, accounts.find((account) => account.id === id)?.color || ""),
      points: completeSeries(historyIn, [id], from),
    })),
  ].filter((row): row is { id: string; color: string; points: { date: string; valueMinor: number }[] } => Boolean(row && row.points.length));
  const portRows: LegendRow[] = [
    { id: "total", label: t("Portfolio value"), color: resolveColor(entityColors, "series", "portfolio:total", ""), amountMinor: totalSeries.length ? totalSeries[totalSeries.length - 1].valueMinor : null, currency: code, className: "total" },
    ...portfolioItems.map((item) => {
      const points = seriesFor(historyIn, [item.id], from);
      const last = points.length ? points[points.length - 1] : undefined;
      return {
        id: item.id,
        label: item.label,
        color: resolveColor(entityColors, "account", item.id, accounts.find((account) => account.id === item.id)?.color || ""),
        amountMinor: last?.valueMinor ?? null,
        currency: code,
        className: item.className,
        original: history.find((point) => point.accountId === item.id && point.source)?.source || "",
      };
    }),
  ];
  const focused = focus ? totalSeries.find((point) => point.date === focus) : null;
  const paint = (kind: string, id: string, color: string) => {
    if (onPaint) onPaint(kind, id, color);
    if (kind === "account") {
      const account = accounts.find((row) => row.id === id);
      if (account) onRecolor(account, color || resolveColor(entityColors, "account", account.id, ""));
    }
    if (kind === "series" && id === "portfolio:total" && onPaint) return;
  };
  const donutSeries = legend.map((row) => ({
    id: row.id,
    label: row.label,
    color: row.color,
    amountMinor: row.amountMinor,
    group: pref.group,
  }));
  const lineSeriesRows = portRows.map((row) => ({ id: row.id, label: row.label, color: row.color, amountMinor: row.amountMinor }));

  return (
    <div className="space-y-4">
      <ChartCard title={t("Holdings chart")}>
        <p className="fin-muted text-[13px] leading-4">{fill("By value in {code}.", { code: code || "—" })}</p>
        <p className="fin-muted text-[13px] leading-4">{t("Latest balances. This period does not rewrite them.")}</p>
        <ChartPeriodBar period={holdingsPeriod.period} onChange={holdingsPeriod.setPeriod} />
        <div className="fin-seg mt-2" role="group" aria-label={t("Holdings chart")}>
          <button type="button" aria-pressed={pref.group === "item"} onClick={() => save({ ...pref, group: "item" })}>{t("By item")}</button>
          <button type="button" aria-pressed={pref.group === "class"} onClick={() => save({ ...pref, group: "class" })}>{t("By class")}</button>
          <button type="button" aria-pressed={pref.cash} onClick={() => save({ ...pref, cash: !pref.cash })}>{t("Include cash")}</button>
        </div>
        {pie.slices.length === 0 ? (
          <p className="mt-3 text-[15px] leading-5">{t("Nothing priced yet")}</p>
        ) : (
          <Donut rows={legend} hidden={vis.hidden} center={formatMoney(visibleTotal, code)} />
        )}
        <Legend
          rows={legend}
          hidden={vis.hidden}
          currency={code}
          onToggle={vis.toggle}
          onShowAll={vis.showAll}
          anyHidden={vis.anyHidden}
          onColor={() => undefined}
          painting={painting}
          setPainting={(id) => {
            setPainting(id);
            if (id) setSheet("holding");
          }}
          onUnpriced={(id) => {
            const account = accounts.find((row) => row.id === id);
            if (account && onUnpriced) onUnpriced(account);
          }}
        />
        <ChartColorMenu
          series={donutSeries}
          currency={code}
          startOpen={openSheet === "donut"}
          onPaint={(id, color) => paint(pref.group === "class" ? "class" : "account", id, color)}
        />
        {sheet === "holding" && painting ? (
          <ColorSheet
            series={donutSeries.filter((row) => row.id === painting)}
            currency={code}
            initialId={painting}
            onPaint={(id, color) => paint(pref.group === "class" ? "class" : "account", id, color)}
            onClose={() => { setSheet(""); setPainting(""); }}
          />
        ) : null}
      </ChartCard>

      <div data-shot="portfolio">
      <ChartCard title={t("Portfolio value")}>
        <p className="fin-muted text-[13px] leading-4">{fill("In {code}. History starts when a real price is saved.", { code: code || "—" })}</p>
        <ChartPeriodBar period={portfolioPeriod.period} onChange={portfolioPeriod.setPeriod} />
        {totalSeries.length === 0 ? (
          <p className="mt-3 text-[15px] leading-5">{t("History starts when a real price is saved.")}</p>
        ) : (
          <>
            {change ? (
              <p className="fin-num mt-2 text-2xl leading-none">
                {formatMoney(change.amountMinor, code)}
                <span className="fin-muted ml-2 text-[13px] font-normal">{`${change.percent >= 0 ? "+" : ""}${Math.round(change.percent * 10) / 10}%`}</span>
              </p>
            ) : null}
            <p className="fin-muted text-[13px] leading-4">{change ? fill("Since {date}", { date: change.from }) : null}</p>
            <LineChart series={lineSeries} currency={code} onPick={setFocus} />
            {focused ? (
              <p className="fin-muted text-[13px] leading-5">{focused.date} · {formatMoney(focused.valueMinor, code)}</p>
            ) : null}
          </>
        )}
        <Legend
          rows={portRows}
          hidden={portVis.hidden}
          currency={code}
          onToggle={portVis.toggle}
          onShowAll={portVis.showAll}
          anyHidden={portVis.anyHidden}
          onColor={() => undefined}
          setPainting={(id) => {
            setPainting(id);
            if (id) setSheet("line");
          }}
          painting={painting}
        />
        <ChartColorMenu
          series={lineSeriesRows}
          currency={code}
          startOpen={openSheet === "line"}
          onPaint={(id, color) => paint(id === "total" ? "series" : "account", id === "total" ? "portfolio:total" : id, color)}
        />
        {sheet === "line" && painting ? (
          <ColorSheet
            series={lineSeriesRows.filter((row) => row.id === painting)}
            currency={code}
            initialId={painting}
            onPaint={(id, color) => paint(id === "total" ? "series" : "account", id === "total" ? "portfolio:total" : id, color)}
            onClose={() => { setSheet(""); setPainting(""); }}
          />
        ) : null}
      </ChartCard>
      </div>
    </div>
  );
}

function sliceLabel(label: string, other: boolean, t: (english: string) => string): string {
  if (other) return t("Other");
  if (["Crypto", "Metals", "Stocks", "Manual", "Cash", "Other"].includes(label)) return t(label);
  return label;
}

