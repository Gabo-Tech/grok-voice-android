import { useEffect, useMemo, useState, type ReactNode } from "react";
import type { FinanceState } from "@/hooks/useFinance";
import type { FinanceAccount } from "@/lib/api";
import { Donut } from "@/components/ChartKit";
import { CombinedHeadline } from "@/components/CombinedTotal";
import { MoneyFlowPanel } from "@/components/MoneyFlowChart";
import { todayIso } from "@/components/FinanceDashboard";
import { periodFromVoice, type ChartPeriod } from "@/lib/chartPeriod";
import {
  changeRatio,
  comparisonKey,
  dashboardRange,
  isDashPreset,
  moveSection,
  normalizeLayout,
  previousRange,
  savingsRate,
  sectionFromVoice,
  spendingPace,
  topCategories,
  totalInRange,
  type DashId,
  type DashPreset,
} from "@/lib/dashboardMath";
import { selectDebt } from "@/lib/financeDebt";
import { isHoldingAccount, positionOf, worthBuckets } from "@/lib/financeLedger";
import { bookFromStatus, comingCashRows, headlineOf, plannedHits, utilization, type RateBook } from "@/lib/financeMath";
import { formatMoney } from "@/lib/money";
import { ChartColorMenu } from "@/components/ColorSheet";
import { resolveColor } from "@/lib/entityColor";
import { fill, useT } from "@/lib/i18n";

const LAYOUT_KEY = "grok-dashboard-layout";
const PRESETS: Array<{ id: DashPreset; label: string }> = [
  { id: "this", label: "This month" },
  { id: "last", label: "Last month" },
  { id: "3m", label: "3M" },
  { id: "ytd", label: "YTD" },
  { id: "1y", label: "1Y" },
  { id: "custom", label: "Custom" },
];

const LABELS: Record<DashId, string> = {
  hero: "Net worth",
  result: "Net result",
  change: "Change",
  next: "Next 30 days",
  debt: "Debt",
  categories: "Top categories",
  flow: "Where my money went",
  pace: "Spending pace",
  attention: "Attention",
};

interface Layout {
  order: DashId[];
  hidden: DashId[];
  preset: DashPreset;
  start: string;
  end: string;
}

function readLayout(): Layout {
  const base = normalizeLayout([], []);
  try {
    const raw = localStorage.getItem(LAYOUT_KEY);
    const parsed = raw ? (JSON.parse(raw) as Partial<Layout>) : {};
    const layout = normalizeLayout(parsed.order || [], parsed.hidden || []);
    const preset = isDashPreset(parsed.preset || "") ? parsed.preset! : "this";
    return { ...layout, preset, start: parsed.start || "", end: parsed.end || "" };
  } catch {
    return { ...base, preset: "this", start: "", end: "" };
  }
}

function formatChange(ratio: number): string {
  const pct = Math.round(Math.abs(ratio) * 1000) / 10;
  const sign = ratio > 0 ? "+" : ratio < 0 ? "−" : "";
  return `${sign}${pct}%`;
}

export function FinanceOverview({
  finance,
  accounts,
  onFocus,
  onRefreshPrices,
  onOpenActivity,
  priceNote = "",
}: {
  finance: FinanceState;
  accounts: FinanceAccount[];
  onFocus: (focus: "owned" | "debt" | "invested" | "cash") => void;
  onRefreshPrices: () => void;
  onOpenActivity: () => void;
  priceNote?: string;
}) {
  const t = useT();
  const [layout, setLayout] = useState<Layout>(readLayout);
  const [customizing, setCustomizing] = useState(false);
  const [flowOwn, setFlowOwn] = useState<ChartPeriod | null>(null);
  const today = todayIso();
  const display = (finance.snapshot.defaultCurrency || "").trim().toUpperCase();
  const book = useMemo(() => bookFromStatus(finance.snapshot.fx), [finance.snapshot.fx]);
  const nowMs = useMemo(() => Date.now(), [finance.snapshot, display]);
  const quotes = finance.snapshot.prices?.quotes || [];
  const range = useMemo(() => dashboardRange(layout.preset, new Date(), { start: layout.start, end: layout.end }), [layout.preset, layout.start, layout.end]);
  const prior = useMemo(() => previousRange(layout.preset, new Date(), range), [layout.preset, range]);
  const txs = finance.snapshot.transactions;
  const moneyRows = useMemo(
    () => txs.map((tx) => ({ amountMinor: tx.amountMinor, currency: tx.currency, direction: tx.direction, date: tx.date, category: tx.category })),
    [txs],
  );
  const income = useMemo(() => totalInRange(moneyRows, "income", range, display, book, nowMs), [moneyRows, range, display, book, nowMs]);
  const expenses = useMemo(() => totalInRange(moneyRows, "expense", range, display, book, nowMs), [moneyRows, range, display, book, nowMs]);
  const priorIncome = useMemo(() => (prior ? totalInRange(moneyRows, "income", prior, display, book, nowMs) : null), [moneyRows, prior, display, book, nowMs]);
  const priorExpenses = useMemo(() => (prior ? totalInRange(moneyRows, "expense", prior, display, book, nowMs) : null), [moneyRows, prior, display, book, nowMs]);
  const categories = useMemo(() => topCategories(moneyRows, range, display, book, nowMs, 5), [moneyRows, range, display, book, nowMs]);
  const pace = useMemo(() => (layout.preset === "this" ? spendingPace(moneyRows, new Date(), display, book, nowMs) : null), [layout.preset, moneyRows, display, book, nowMs]);
  const planned = useMemo(() => plannedHits(finance.snapshot.rules, txs, today), [finance.snapshot.rules, txs, today]);
  const debt = useMemo(
    () =>
      selectDebt(
        accounts.map((account) => ({ id: account.id, name: account.name, kind: account.kind, currency: account.currency, shownBalanceMinor: account.shownBalanceMinor })),
        planned.map((hit) => ({ id: `${hit.scheduleId}-${hit.date}`, name: hit.name, amountMinor: hit.amountMinor, currency: hit.currency, date: hit.date, direction: hit.direction, accountId: hit.accountId })),
        display,
        book,
        nowMs,
        today,
      ),
    [accounts, planned, display, book, nowMs, today],
  );
  const buckets = useMemo(() => worthBuckets(accounts, display, book, quotes, nowMs), [accounts, display, book, quotes, nowMs]);
  const worthParts = useMemo(() => [{ amountMinor: buckets.assets - debt.amountMinor, currency: display }], [buckets.assets, debt.amountMinor, display]);
  const coming = useMemo(() => {
    const cards = accounts.map((account) => ({ id: account.id, name: account.name, kind: account.kind, currency: account.currency, shownBalanceMinor: account.shownBalanceMinor, paymentMinor: null, dueDate: "", dueDay: account.dueDay }));
    const out = comingCashRows(finance.snapshot.rules, txs, cards, today, "out");
    const inn = comingCashRows(finance.snapshot.rules, txs, cards, today, "in");
    return [...out, ...inn].sort((a, b) => a.date.localeCompare(b.date) || a.name.localeCompare(b.name));
  }, [accounts, finance.snapshot.rules, txs, today]);
  const dueParts = useMemo(
    () => coming.filter((row) => row.direction !== "income" && row.amountMinor).map((row) => ({ amountMinor: row.amountMinor || 0, currency: row.currency })),
    [coming],
  );
  const attention = useMemo(() => {
    const notes: string[] = [];
    for (const account of accounts) {
      if (isHoldingAccount(account) && positionOf(account, display, book, quotes, nowMs).unpriced) notes.push(fill("No price for {name}", { name: account.name }));
      if (account.kind === "credit_card") {
        const ratio = utilization(account.shownBalanceMinor, account.creditLimitMinor);
        if (ratio != null && ratio > 0.8) notes.push(fill("{name} is over 80% of its limit", { name: account.name }));
      }
    }
    const bills = debt.slices.find((slice) => slice.id === "bills");
    for (const item of bills?.items || []) notes.push(fill("{name} is overdue", { name: item.label }));
    return notes;
  }, [accounts, display, book, quotes, nowMs, debt]);

  useEffect(() => {
    try {
      localStorage.setItem(LAYOUT_KEY, JSON.stringify(layout));
    } catch {
      /* The layout still applies for this visit. */
    }
  }, [layout]);

  useEffect(() => {
    const onVoice = (event: Event) => {
      const detail = (event as CustomEvent<{ chart?: string; action?: string; name?: string; period?: string; start?: string; end?: string; view?: string }>).detail || {};
      const chart = (detail.chart || "").trim().toLowerCase();
      if (chart === "flow") return;
      const preset = periodFromVoice(detail.period || "");
      if (preset && isDashPreset(preset) && (!chart || chart === "dashboard" || chart === "overview")) {
        setLayout((current) => ({ ...current, preset, start: detail.start || current.start, end: detail.end || current.end }));
      }
      const section = sectionFromVoice(detail.name || "") || (chart === "category" ? "categories" : null);
      if (!section || (detail.action !== "hide" && detail.action !== "show")) return;
      setLayout((current) => {
        const hidden = new Set(current.hidden);
        if (detail.action === "hide") hidden.add(section);
        else hidden.delete(section);
        return { ...current, hidden: [...hidden] };
      });
    };
    window.addEventListener("grok-finance-voice", onVoice);
    return () => window.removeEventListener("grok-finance-voice", onVoice);
  }, []);

  const hidden = new Set(layout.hidden);
  const flowPeriod: ChartPeriod = flowOwn || { preset: layout.preset === "custom" ? "custom" : layout.preset, start: layout.start, end: layout.end };
  const rate = income.amountMinor != null && expenses.amountMinor != null ? savingsRate(income.amountMinor, expenses.amountMinor) : null;
  const net = income.amountMinor != null && expenses.amountMinor != null ? income.amountMinor - expenses.amountMinor : null;
  const maxCategory = categories.rows[0]?.amountMinor || 1;
  const ringRows = buckets.buckets
    .filter((bucket) => bucket.id !== "debt" && bucket.amountMinor > 0)
    .map((bucket) => ({
      id: bucket.id,
      label: bucket.id,
      color: resolveColor(finance.snapshot.entityColors, "series", `allocation:${bucket.id}`, ""),
      amountMinor: bucket.amountMinor,
    }));

  const blocks: Record<DashId, ReactNode> = {
    hero: (
      <section data-finance-panel="hero">
        <h2 className="fin-kicker">{t("Net worth")}</h2>
        <p className="fin-muted mt-1 text-sm leading-5">{t("Right now. What you own minus what you owe.")}</p>
        {worthParts.length ? <CombinedHeadline parts={worthParts} display={display} book={book} nowMs={nowMs} busy={finance.busy} size="text-5xl" /> : <p className="mt-3 text-base">{t("No balances stated yet")}</p>}
        <button type="button" className="mt-3 flex min-h-11 w-full items-center justify-between gap-3 text-left" onClick={() => onFocus("owned")}>
          <span className="text-[15px]">{t("Income")}</span>
          <Amount total={income} display={display} book={book} nowMs={nowMs} />
        </button>
        <button type="button" className="flex min-h-11 w-full items-center justify-between gap-3 border-t border-[var(--line-soft)] text-left" onClick={onOpenActivity}>
          <span className="text-[15px]">{t("Expenses")}</span>
          <Amount total={expenses} display={display} book={book} nowMs={nowMs} />
        </button>
        {ringRows.length ? (
          <>
            <Donut rows={ringRows} hidden={new Set()} caption={t("Net worth")} center={formatMoney(buckets.assets - debt.amountMinor, display)} debtRatio={buckets.assets > 0 ? debt.amountMinor / buckets.assets : debt.amountMinor > 0 ? 1 : 0} />
            <ChartColorMenu series={ringRows.map((row) => ({ ...row, label: t(row.label === "money" ? "Money" : row.label === "investment" ? "Investments" : "Card credit") }))} currency={display} onPaint={(id, color) => finance.paintColor("series", `allocation:${id}`, color)} />
          </>
        ) : null}
      </section>
    ),
    result: (
      <section className="border-t border-[var(--line-soft)] pt-3" data-finance-panel="result">
        <h2 className="fin-kicker">{t("Net result")}</h2>
        <p className="fin-num mt-2 text-[15px]">{net == null ? "—" : formatMoney(net, display)}</p>
        {rate != null ? <p className="fin-muted text-[13px] leading-5">{t("Savings rate")} {Math.round(rate * 1000) / 10}%</p> : null}
      </section>
    ),
    change: (
      <section className="border-t border-[var(--line-soft)] pt-3" data-finance-panel="change">
        <h2 className="fin-kicker">{t("Change")}</h2>
        <Compare label={t("Income")} current={income.amountMinor} previous={priorIncome?.amountMinor ?? null} preset={layout.preset} display={display} />
        <Compare label={t("Expenses")} current={expenses.amountMinor} previous={priorExpenses?.amountMinor ?? null} preset={layout.preset} display={display} />
      </section>
    ),
    next: (
      <section className="border-t border-[var(--line-soft)] pt-3" data-finance-panel="next">
        <h2 className="fin-kicker">{t("Next 30 days")}</h2>
        <p className="fin-muted mt-1 text-[13px] leading-5">{t("Bills from cash or a bank, and card payments that are due.")}</p>
        {dueParts.length ? <CombinedHeadline parts={dueParts} display={display} book={book} nowMs={nowMs} busy={finance.busy} size="text-3xl" tone="out" /> : <p className="mt-2 text-[15px]">{t("Nothing due in the next 30 days.")}</p>}
        <ul className="mt-2">
          {coming.slice(0, 6).map((row) => (
            <li key={`${row.id}-${row.date}`} className="flex min-h-11 items-center justify-between gap-3 border-t border-[var(--line-soft)]">
              <span className="min-w-0">
                <span className="block text-[15px]">{row.name}</span>
                <span className="fin-muted text-[13px] leading-4">{row.date}</span>
              </span>
              <span className="fin-num text-[15px]">{row.amountMinor ? formatMoney(row.amountMinor, row.currency) : t("Not set")}</span>
            </li>
          ))}
        </ul>
      </section>
    ),
    debt: (
      <section className="border-t border-[var(--line-soft)] pt-3" data-finance-panel="debt">
        <button type="button" className="flex min-h-11 w-full items-center justify-between gap-3 text-left" onClick={() => onFocus("debt")}>
          <span className="text-[15px]">{t("Debt")}</span>
          <span className="fin-num fin-out text-[15px]">{formatMoney(debt.amountMinor, display)}</span>
        </button>
        {debt.natives.length ? <p className="fin-muted text-[13px] leading-5">{debt.natives.map((row) => formatMoney(row.amountMinor, row.currency)).join(" · ")}</p> : null}
        {debt.slices.map((slice) => (
          <div key={slice.id} className="flex min-h-11 items-center justify-between gap-3">
            <span className="text-[15px]">{t(slice.id === "cards" ? "Cards" : slice.id === "loans" ? "Loans" : slice.id === "people" ? "Owed to people" : "Overdue bills")}</span>
            <span className="fin-num text-[15px]">{formatMoney(slice.amountMinor, display)}</span>
          </div>
        ))}
        {accounts.some((account) => isHoldingAccount(account) && positionOf(account, display, book, quotes, nowMs).unpriced) ? (
          <>
            <button type="button" className="fin-chip mt-1" onClick={onRefreshPrices}>{t("Refresh price now")}</button>
            {priceNote ? <p className="fin-muted mt-1 text-[13px] leading-5">{priceNote}</p> : null}
          </>
        ) : null}
      </section>
    ),
    categories: (
      <section className="border-t border-[var(--line-soft)] pt-3" data-finance-panel="categories">
        <h2 className="fin-kicker">{t("Top categories")}</h2>
        {categories.rows.length === 0 ? <p className="mt-2 text-[15px]">{t("No income or expenses in this period.")}</p> : null}
        <ul>
          {categories.rows.map((row) => (
            <li key={row.name} className="min-h-11 border-t border-[var(--line-soft)] py-2">
              <div className="flex items-center justify-between gap-3">
                <span className="text-[15px]">{row.name === "Uncategorized" ? t("Uncategorized") : row.name}</span>
                <span className="fin-num text-[15px]">{formatMoney(row.amountMinor, display)}</span>
              </div>
              <span className="mt-1 block h-0.5 w-full bg-[var(--line-soft)]">
                <span className="block h-full" style={{ width: `${Math.max(4, Math.round((row.amountMinor / maxCategory) * 100))}%`, background: resolveColor(finance.snapshot.entityColors, finance.snapshot.categories.find((category) => category.name === row.name)?.kind === "income" ? "income" : "category", row.name, finance.snapshot.categories.find((category) => category.name === row.name)?.color || "") }} />
              </span>
            </li>
          ))}
        </ul>
      </section>
    ),
    flow: (
      <div className="border-t border-[var(--line-soft)] pt-3" data-finance-panel="flow" data-shot="flow">
        <MoneyFlowPanel
          transactions={txs}
          accounts={accounts}
          categories={finance.snapshot.categories}
          display={display}
          fx={finance.snapshot.fx}
          externalPeriod={flowPeriod}
          onDetach={setFlowOwn}
          detached={flowOwn != null}
          onFollow={() => setFlowOwn(null)}
          onPaintCategory={(name, color) => finance.editCategory({ action: "color", name, color })}
          flowColors={finance.snapshot.flowColors || []}
          flowLayout={finance.snapshot.flowLayout || ""}
          onPaintNode={(id, color) => finance.editCategory({ action: color ? "paint" : "unpaint", name: id, color })}
          onLayout={(layout) => finance.editCategory({ action: "layout", name: layout })}
          onReassign={onOpenActivity}
          budgets={finance.snapshot.budgets || []}
        />
      </div>
    ),
    pace: (
      <section className="border-t border-[var(--line-soft)] pt-3" data-finance-panel="pace">
        <h2 className="fin-kicker">{t("Spending pace")}</h2>
        {layout.preset !== "this" ? <p className="fin-muted mt-2 text-[13px] leading-5">{t("Pace uses this month.")}</p> : null}
        {pace ? (
          <>
            <p className="mt-2 text-[15px]">{formatMoney(pace.spentMinor, display)}</p>
            <p className="fin-muted text-[13px] leading-5">{fill("Through {date}. Average of {count} earlier months, same day: {amount}", { date: pace.through, count: String(pace.months), amount: formatMoney(pace.averageMinor, display) })}</p>
          </>
        ) : layout.preset === "this" ? <p className="fin-muted mt-2 text-[13px] leading-5">{t("Not enough history")}</p> : null}
      </section>
    ),
    attention: (
      <section className="border-t border-[var(--line-soft)] pt-3" data-finance-panel="attention">
        <h2 className="fin-kicker">{t("Attention")}</h2>
        {attention.length === 0 ? <p className="fin-muted mt-2 text-[13px] leading-5">{t("Nothing needs attention.")}</p> : null}
        <ul>
          {attention.map((line) => (
            <li key={line} className="min-h-11 border-t border-[var(--line-soft)] py-2 text-[15px]">{line}</li>
          ))}
        </ul>
      </section>
    ),
  };

  return (
    <div className="fin-stack w-full min-w-0">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1" data-swipe-ignore="" role="group" aria-label={t("Period")}>
        {PRESETS.map((item) => (
          <button key={item.id} type="button" className="fin-chip" aria-pressed={layout.preset === item.id} onClick={() => setLayout((current) => ({ ...current, preset: item.id }))}>
            {t(item.label)}
          </button>
        ))}
        <button type="button" className="fin-chip" aria-pressed={customizing} onClick={() => setCustomizing((value) => !value)}>{t("Customize dashboard")}</button>
      </div>
      {layout.preset === "custom" ? (
        <div className="mt-2 flex flex-wrap gap-2" data-swipe-ignore="">
          <label className="text-[13px] leading-4">
            {t("From")}
            <input type="date" className="mt-1 block h-11 border border-[var(--line)] bg-transparent px-2 text-[15px]" value={layout.start} onChange={(event) => setLayout((current) => ({ ...current, start: event.target.value }))} />
          </label>
          <label className="text-[13px] leading-4">
            {t("To")}
            <input type="date" className="mt-1 block h-11 border border-[var(--line)] bg-transparent px-2 text-[15px]" value={layout.end} onChange={(event) => setLayout((current) => ({ ...current, end: event.target.value }))} />
          </label>
        </div>
      ) : null}
      {customizing ? (
        <ul className="mt-3" data-swipe-ignore="">
          {layout.order.map((id) => (
            <li key={id} className="flex min-h-11 items-center justify-between gap-2 border-t border-[var(--line-soft)]">
              <span className="text-[15px]">{t(LABELS[id])}</span>
              <span className="flex gap-2">
                <button type="button" className="fin-chip" onClick={() => setLayout((current) => ({ ...current, order: moveSection(current.order, id, -1) }))}>{t("Move up")}</button>
                <button type="button" className="fin-chip" onClick={() => setLayout((current) => ({ ...current, order: moveSection(current.order, id, 1) }))}>{t("Move down")}</button>
                <button
                  type="button"
                  className="fin-chip"
                  aria-pressed={!hidden.has(id)}
                  onClick={() =>
                    setLayout((current) => {
                      const next = new Set(current.hidden);
                      if (next.has(id)) next.delete(id);
                      else next.add(id);
                      return { ...current, hidden: [...next] };
                    })
                  }
                >
                  {hidden.has(id) ? t("Show") : t("Hide")}
                </button>
              </span>
            </li>
          ))}
        </ul>
      ) : null}
      <div className="mt-4 space-y-4">
        {layout.order.filter((id) => !hidden.has(id)).map((id) => (
          <div key={id}>{blocks[id]}</div>
        ))}
      </div>
    </div>
  );
}

function Amount({ total, display, book, nowMs }: { total: { amountMinor: number | null; missing: string[] }; display: string; book: RateBook | null; nowMs: number }) {
  if (total.amountMinor == null) {
    const head = headlineOf(total.missing.map((code) => ({ amountMinor: 1, currency: code })), display, book, nowMs);
    return <span className="fin-num text-[15px]">{head.state === "needs-rate" ? head.missing.join(", ") : "—"}</span>;
  }
  return <span className="fin-num text-[15px]">{formatMoney(total.amountMinor, display)}</span>;
}

function Compare({ label, current, previous, preset, display }: { label: string; current: number | null; previous: number | null; preset: DashPreset; display: string }) {
  const t = useT();
  if (current == null || previous == null) return <p className="mt-2 text-[15px]">{label}</p>;
  const ratio = changeRatio(current, previous);
  return (
    <p className="mt-2 text-[15px]">
      {label} {formatMoney(current, display)}
      {ratio == null ? <span className="fin-muted"> · {t("No earlier amount to compare")}</span> : <span className="fin-muted"> {formatChange(ratio)} {t(comparisonKey(preset))}</span>}
    </p>
  );
}
