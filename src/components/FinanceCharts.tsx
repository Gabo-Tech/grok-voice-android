import { useMemo, useRef, useState, type ReactElement } from "react";
import type { FinanceAccount, FinanceCategory, FinanceFx, FinancePrices, FinanceRule, FinanceTransaction } from "@/lib/api";
import { todayIso } from "@/components/FinanceDashboard";
import { ChartCard, Donut, Legend, LineChart, monthName, useChartVisibility, type LegendRow } from "@/components/ChartKit";
import { Loader } from "@/components/Loader";
import { ChartColorMenu } from "@/components/ColorSheet";
import { resolveColor, type EntityColor } from "@/lib/entityColor";
import { ChartPeriodBar, useChartPeriod } from "@/components/ChartPeriodBar";
import { inBounds, monthsIn } from "@/lib/chartPeriod";
import { selectDebt } from "@/lib/financeDebt";
import { debtRatio, netOfVisible, worthBuckets } from "@/lib/financeLedger";
import { plannedHits } from "@/lib/financeMath";
import { bookFromStatus, headlineOf, type MoneyPart, type RateBook } from "@/lib/financeMath";
import { formatMoney } from "@/lib/money";
import { isDebt } from "@/lib/financeView";
import { activeLocale, fill, useT } from "@/lib/i18n";

export interface ChartPick {
  accountId?: string;
  category?: string;
  currency?: string;
  month?: string;
  group?: "money" | "debt" | "investment";
}

export function FinanceCarousel({
  accounts,
  transactions,
  rules = [],
  display = "",
  fx,
  prices,
  busy = false,
  onOpenSettings,
  onPick,
  categories = [],
  entityColors = [],
  onPaint,
}: {
  accounts: FinanceAccount[];
  transactions: FinanceTransaction[];
  rules?: FinanceRule[];
  display?: string;
  fx?: FinanceFx | null;
  prices?: FinancePrices | null;
  busy?: boolean;
  onOpenSettings?: () => void;
  onPick: (pick: ChartPick) => void;
  categories?: FinanceCategory[];
  entityColors?: EntityColor[];
  onPaint?: (kind: string, id: string, color: string) => void;
}) {
  const t = useT();
  const allocationPeriod = useChartPeriod("allocation");
  const categoryPeriod = useChartPeriod("category");
  const debtPeriod = useChartPeriod("debts");
  const trendPeriod = useChartPeriod("trend", "6m");
  const scroller = useRef<HTMLDivElement>(null);
  const [page, setPage] = useState(0);
  const book = useMemo(() => bookFromStatus(fx), [fx]);
  const nowMs = useMemo(() => Date.now(), [fx, transactions, accounts, display]);
  const code = display.trim().toUpperCase();
  const quotes = prices?.quotes || [];
  const buckets = useMemo(() => worthBuckets(accounts, code, book, quotes, nowMs), [accounts, code, book, quotes, nowMs]);
  const today = todayIso();
  const debt = useMemo(() => {
    const hits = plannedHits(rules, transactions, today);
    return selectDebt(
      accounts.map((account) => ({
        id: account.id,
        name: account.name,
        kind: account.kind,
        currency: account.currency,
        shownBalanceMinor: account.shownBalanceMinor,
      })),
      hits.map((hit) => ({
        id: `${hit.scheduleId}-${hit.date}`,
        name: hit.name,
        amountMinor: hit.amountMinor,
        currency: hit.currency,
        date: hit.date,
        direction: hit.direction,
        accountId: hit.accountId,
      })),
      code,
      book,
      nowMs,
      today,
    );
  }, [accounts, rules, transactions, code, book, nowMs, today]);
  const ringRows = useMemo(() => {
    const labels = { money: t("Money"), debt: t("Debts"), investment: t("Investments"), credit: t("Card credit") };
    return buckets.buckets
      .filter((bucket) => bucket.id !== "debt" && bucket.amountMinor > 0)
      .map((bucket) => ({
        id: bucket.id,
        label: labels[bucket.id],
        color: resolveColor(entityColors, "series", `allocation:${bucket.id}`, ""),
        amountMinor: bucket.amountMinor,
        currency: code,
        className: bucket.id,
        original: bucket.natives.map((row) => formatMoney(row.amountMinor, row.currency)).join(" · "),
        note: bucket.unpriced.length ? fill("No price for {name}", { name: bucket.unpriced.join(", ") }) : "",
      }));
  }, [buckets, code, t, entityColors]);
  const categoryRows = useMemo(
    () => categoryLegend(transactions, code, book, nowMs, categoryPeriod.bounds.start, categoryPeriod.bounds.end, categories, entityColors),
    [transactions, code, book, nowMs, categoryPeriod.bounds.start, categoryPeriod.bounds.end, categories, entityColors],
  );
  const debtRows = useMemo(() => debtLegend(accounts, code, book, nowMs, entityColors), [accounts, code, book, nowMs, entityColors]);
  const trend = useMemo(
    () => trendPoints(transactions, code, book, nowMs, trendPeriod.bounds.start, trendPeriod.bounds.end),
    [transactions, code, book, nowMs, trendPeriod.bounds.start, trendPeriod.bounds.end],
  );
  const ringVis = useChartVisibility("allocation", ringRows);
  const categoryVis = useChartVisibility("category", categoryRows);
  const debtVis = useChartVisibility("debts", debtRows);
  const trendVis = useChartVisibility("trend", [
    { id: "income", label: t("Income"), className: "income" },
    { id: "expense", label: t("Expenses"), className: "expense" },
    { id: "net", label: t("Result"), className: "result" },
  ]);
  const cards = [
    ringRows.length
      ? {
          key: "allocation",
          body: (
            <ChartCard title={t("Money, debts, and investments")}>
              <p className="fin-muted text-[13px] leading-4">{t("Latest balances. This period does not rewrite them.")}</p>
              <ChartPeriodBar period={allocationPeriod.period} onChange={allocationPeriod.setPeriod} />
              <Donut
                rows={ringRows}
                hidden={ringVis.hidden}
                caption={t("Net worth")}
                center={formatMoney(netOfVisible(buckets.buckets, ringVis.hidden).assets - debt.amountMinor, code)}
                debtRatio={debtRatio(netOfVisible(buckets.buckets, ringVis.hidden).assets, debt.amountMinor)}
              />
              <DebtAgainstAssets owed={debt.amountMinor} assets={netOfVisible(buckets.buckets, ringVis.hidden).assets} currency={code} original={debt.natives.map((row) => formatMoney(row.amountMinor, row.currency)).join(" · ") || ""} />
              <Legend rows={ringRows} hidden={ringVis.hidden} currency={code} onToggle={ringVis.toggle} onShowAll={ringVis.showAll} anyHidden={ringVis.anyHidden} />
              {onPaint ? <ChartColorMenu series={ringRows} currency={code} onPaint={(id, color) => onPaint("series", `allocation:${id}`, color)} /> : null}
            </ChartCard>
          ),
        }
      : null,
    categoryRows.length
      ? {
          key: "category",
          body: (
            <ChartCard title={t("By category")}>
              <ChartPeriodBar period={categoryPeriod.period} onChange={categoryPeriod.setPeriod} />
              <Legend rows={categoryRows} hidden={categoryVis.hidden} currency={code} onToggle={categoryVis.toggle} onShowAll={categoryVis.showAll} anyHidden={categoryVis.anyHidden} />
              {onPaint ? <ChartColorMenu series={categoryRows} currency={code} onPaint={(id, color) => onPaint(categories.find((row) => row.name === id)?.kind === "income" ? "income" : "category", id, color)} /> : null}
            </ChartCard>
          ),
        }
      : null,
    debtRows.length
      ? {
          key: "debts",
          body: (
            <ChartCard title={t("Debts")}>
              <p className="fin-muted text-[13px] leading-4">{t("Latest balances. This period does not rewrite them.")}</p>
              <ChartPeriodBar period={debtPeriod.period} onChange={debtPeriod.setPeriod} />
              <Donut rows={debtRows} hidden={debtVis.hidden} caption={t("Owed")} center={formatMoney(visibleTotal(debtRows, debtVis.hidden), code)} />
              <Legend rows={debtRows} hidden={debtVis.hidden} currency={code} onToggle={debtVis.toggle} onShowAll={debtVis.showAll} anyHidden={debtVis.anyHidden} />
              {onPaint ? <ChartColorMenu series={debtRows} currency={code} onPaint={(id, color) => onPaint("account", id, color)} /> : null}
            </ChartCard>
          ),
        }
      : null,
    trend.points.length
      ? {
          key: "trend",
          body: (
            <TrendCard
              title={t("Income vs expenses")}
              points={trend.points}
              currency={code}
              missing={trend.missing}
              busy={busy}
              onOpenSettings={onOpenSettings}
              hidden={trendVis.hidden}
              onToggle={trendVis.toggle}
              onShowAll={trendVis.showAll}
              anyHidden={trendVis.anyHidden}
              onPick={(month) => onPick({ month })}
              period={<ChartPeriodBar period={trendPeriod.period} onChange={trendPeriod.setPeriod} />}
              entityColors={entityColors}
              onPaint={onPaint}
            />
          ),
        }
      : null,
  ].filter((card): card is { key: string; body: ReactElement } => Boolean(card));
  if (cards.length === 0) return null;
  const go = (index: number) => {
    const node = scroller.current;
    if (!node) return;
    const child = node.children[index] as HTMLElement | undefined;
    child?.scrollIntoView({ behavior: "smooth", inline: "start", block: "nearest" });
    setPage(index);
  };
  return (
    <section className="w-full min-w-0 max-w-full">
      <p className="fin-kicker mb-2">{t("Charts")}</p>
      <div
        ref={scroller}
        data-swipe-ignore=""
        className="fin-carousel flex w-full min-w-0 max-w-full snap-x snap-mandatory items-start overflow-x-auto"
        onScroll={(event) => {
          const node = event.currentTarget;
          const width = node.clientWidth || 1;
          setPage(Math.min(cards.length - 1, Math.max(0, Math.round(node.scrollLeft / width))));
        }}
      >
        {cards.map((card) => (
          <div key={card.key} className="w-full max-w-full shrink-0 snap-start">
            {card.body}
          </div>
        ))}
      </div>
      <div className="mt-2 flex justify-center gap-1">
        {cards.map((card, index) => (
          <button key={card.key} type="button" className="fin-icon" aria-label={`${index + 1}`} aria-current={index === page} onClick={() => go(index)}>
            <span className={index === page ? "fin-dot on" : "fin-dot"} />
          </button>
        ))}
      </div>
    </section>
  );
}

function visibleTotal(rows: LegendRow[], hidden: Set<string>): number {
  return rows.reduce((sum, row) => (hidden.has(row.id) || row.omitShare || row.amountMinor == null ? sum : sum + row.amountMinor), 0);
}

function DebtAgainstAssets({ owed, assets, currency, original }: { owed: number; assets: number; currency: string; original: string }) {
  const t = useT();
  if (owed <= 0 && !original) return null;
  const ratio = debtRatio(assets, owed);
  const percent = ratio > 0 && ratio < 0.001 ? "<0.1" : String(Math.round(ratio * 1000) / 10);
  const width = `${Math.min(100, ratio * 100)}%`;
  return (
    <div className="mt-3">
      <p className="text-[13px] leading-4 font-extrabold tracking-wide uppercase">{t("Debts vs assets")}</p>
      <span className="fin-track mt-2 block">
        <span className="block h-full" style={{ width, background: "var(--negative)" }} />
      </span>
      <p className="mt-2 text-[13px] leading-4">
        <span className="fin-num">{formatMoney(owed, currency)}</span>
        <span className="fin-muted"> · {fill("{percent}% of assets", { percent })}</span>
      </p>
      {original ? <p className="fin-muted text-[13px] leading-4">{original}</p> : null}
    </div>
  );
}

function categoryLegend(txs: FinanceTransaction[], display: string, book: RateBook | null, nowMs: number, start: string, end: string, categories: FinanceCategory[], entityColors: EntityColor[]): LegendRow[] {
  const by = new Map<string, { name: string; parts: MoneyPart[] }>();
  for (const tx of txs) {
    if (tx.direction !== "expense" || !inBounds(tx.date, start, end) || !tx.currency || !tx.amountMinor) continue;
    const name = tx.category || "Expense";
    const row = by.get(name) ?? { name, parts: [] };
    row.parts.push({ amountMinor: tx.amountMinor, currency: tx.currency });
    by.set(name, row);
  }
  return [...by.values()].flatMap((row) => {
    const head = headlineOf(row.parts, display, book, nowMs);
    if (head.state !== "ready" || head.amountMinor === 0) return [];
    const natives = new Map<string, number>();
    for (const part of row.parts) {
      const code = part.currency.toUpperCase();
      if (code === display) continue;
      natives.set(code, (natives.get(code) || 0) + part.amountMinor);
    }
    return [{
      id: row.name,
      label: row.name,
      color: resolveColor(entityColors, categories.find((category) => category.name === row.name)?.kind === "income" ? "income" : "category", row.name, categories.find((category) => category.name === row.name)?.color || ""),
      amountMinor: head.amountMinor,
      currency: display,
      className: row.name,
      original: [...natives.entries()].map(([currency, amountMinor]) => formatMoney(amountMinor, currency)).join(" · "),
    }];
  });
}

function debtLegend(accounts: FinanceAccount[], display: string, book: RateBook | null, nowMs: number, entityColors: EntityColor[]): LegendRow[] {
  return accounts.flatMap((account) => {
    if (!isDebt(account.kind) || account.shownBalanceMinor == null || account.shownBalanceMinor <= 0) return [];
    const head = headlineOf([{ amountMinor: account.shownBalanceMinor, currency: account.currency }], display, book, nowMs);
    if (head.state !== "ready") return [];
    const original = account.currency.toUpperCase() !== display ? formatMoney(account.shownBalanceMinor, account.currency) : "";
    return [{
      id: account.id,
      label: account.name,
      color: resolveColor(entityColors, "account", account.id, account.color),
      amountMinor: head.amountMinor,
      currency: display,
      className: account.kind,
      original,
    }];
  });
}

function trendPoints(txs: FinanceTransaction[], display: string, book: RateBook | null, nowMs: number, start: string, end: string) {
  const missing = new Set<string>();
  const points = monthsIn(start, end).map((key) => {
    const incomeParts: MoneyPart[] = [];
    const expenseParts: MoneyPart[] = [];
    for (const tx of txs) {
      if (!tx.date.startsWith(key) || !tx.currency) continue;
      if (tx.direction === "income") incomeParts.push({ amountMinor: tx.amountMinor, currency: tx.currency });
      if (tx.direction === "expense") expenseParts.push({ amountMinor: tx.amountMinor, currency: tx.currency });
    }
    const income = headlineOf(incomeParts, display, book, nowMs);
    const expense = headlineOf(expenseParts, display, book, nowMs);
    income.missing.forEach((code) => missing.add(code));
    expense.missing.forEach((code) => missing.add(code));
    const incomeMinor = income.state === "ready" ? income.amountMinor : 0;
    const expenseMinor = expense.state === "ready" ? expense.amountMinor : 0;
    return { key, income: incomeMinor, expense: expenseMinor, net: incomeMinor - expenseMinor };
  });
  if (!points.some((point) => point.income || point.expense)) return { points: [] as typeof points, missing: [] as string[] };
  return { points, missing: [...missing] };
}

function TrendCard({
  title,
  points,
  currency,
  missing,
  busy,
  onOpenSettings,
  hidden,
  onToggle,
  onShowAll,
  anyHidden,
  onPick,
  period,
  entityColors = [],
  onPaint,
}: {
  title: string;
  points: Array<{ key: string; income: number; expense: number; net: number }>;
  currency: string;
  missing: string[];
  busy: boolean;
  period?: ReactElement;
  onOpenSettings?: () => void;
  hidden: Set<string>;
  onToggle: (id: string) => void;
  onShowAll: () => void;
  anyHidden: boolean;
  onPick: (month: string) => void;
  entityColors?: EntityColor[];
  onPaint?: (kind: string, id: string, color: string) => void;
}) {
  const t = useT();
  const locale = activeLocale();
  const [focus, setFocus] = useState(points[points.length - 1]?.key || "");
  const current = points[points.length - 1]?.key || "";
  const chosen = points.find((point) => point.key === focus) || points[points.length - 1];
  const rows: LegendRow[] = [
    { id: "income", label: t("Income"), color: resolveColor(entityColors, "series", "trend:income", ""), amountMinor: chosen?.income || 0, currency, className: "income" },
    { id: "expense", label: t("Expenses"), color: resolveColor(entityColors, "series", "trend:expense", ""), amountMinor: chosen?.expense || 0, currency, className: "expense" },
    { id: "net", label: t("Result"), color: resolveColor(entityColors, "series", "trend:net", ""), amountMinor: chosen?.net || 0, currency, className: "result" },
  ];
  if (missing.length) {
    return (
      <ChartCard title={title}>
        {period}
        <div className="flex items-start gap-2">
          {busy ? <Loader size="inline" quiet label={fill("Checking the rate for {codes}.", { codes: missing.join(", ") })} /> : null}
          <p className="text-[15px] leading-5">{busy ? fill("Checking the rate for {codes}.", { codes: missing.join(", ") }) : fill("Combined total needs a rate for {codes}.", { codes: missing.join(", ") })}</p>
        </div>
        {onOpenSettings && !busy ? (
          <button type="button" className="fin-textbtn mt-2" onClick={onOpenSettings}>{t("Settings")}</button>
        ) : null}
      </ChartCard>
    );
  }
  const series = [
    { id: "income", color: resolveColor(entityColors, "series", "trend:income", ""), points: points.map((point) => ({ date: point.key, valueMinor: point.income })) },
    { id: "expense", color: resolveColor(entityColors, "series", "trend:expense", ""), points: points.map((point) => ({ date: point.key, valueMinor: point.expense })) },
    { id: "net", color: resolveColor(entityColors, "series", "trend:net", ""), points: points.map((point) => ({ date: point.key, valueMinor: point.net })) },
  ].filter((row) => !hidden.has(row.id));
  return (
    <ChartCard title={title}>
      <p className="fin-muted text-[13px] leading-4">{fill("In {code}. This month is still open.", { code: currency })}</p>
      {period}
      <LineChart series={series} currency={currency} partial={current} onPick={(date) => { setFocus(date); onPick(date); }} />
      {chosen ? (
        <p className="fin-muted mt-1 text-[13px] leading-5">
          {monthName(chosen.key, locale)}
          {chosen.key === current ? ` · ${t("Partial")}` : ""}
          {` · ${t("Income")} ${formatMoney(chosen.income, currency)} · ${t("Expenses")} ${formatMoney(chosen.expense, currency)} · ${t("Result")} ${formatMoney(chosen.net, currency)}`}
        </p>
      ) : null}
      <Legend rows={rows} hidden={hidden} currency={currency} onToggle={onToggle} onShowAll={onShowAll} anyHidden={anyHidden} />
      {onPaint ? <ChartColorMenu series={rows} currency={currency} onPaint={(id, color) => onPaint("series", `trend:${id}`, color)} /> : null}
    </ChartCard>
  );
}

