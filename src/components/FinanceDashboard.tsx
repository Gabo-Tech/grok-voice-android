import { useEffect, useMemo, useRef } from "react";
import type { FinanceState } from "@/hooks/useFinance";
import type { FinanceAccount, FinanceRule, FinanceTransaction } from "@/lib/api";
import { CombinedHeadline, RatePanel, StaticHeadline } from "@/components/CombinedTotal";
import {
  addDays,
  addMonths,
  bookFromStatus,
  cashFlow,
  comingRows,
  headlineOf,
  monthBounds,
  netWorth,
  netWorthChange,
  plannedHits,
  projectWindow,
  subtractHeadlines,
  type ComingRow,
  type Headline,
  type MoneyPart,
  type RateBook,
} from "@/lib/financeMath";
import { formatMoney } from "@/lib/money";
import { fill, useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

export type WorthFocus = "owned" | "debt" | "invested" | "cash";

export function todayIso(now = new Date()): string {
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

function listed(rows: ComingRow[]): MoneyPart[] {
  return rows.flatMap((row) => (row.amountMinor == null || !row.currency ? [] : [{ amountMinor: row.amountMinor, currency: row.currency }]));
}

function gapsOf(heads: Headline[]): string[] {
  return [...new Set(heads.flatMap((head) => head.missing))].sort();
}

export function FinanceDashboard({
  finance,
  accounts,
  onProgram,
  onIncome,
  onFocus,
}: {
  finance: FinanceState;
  accounts: FinanceAccount[];
  onProgram: () => void;
  onIncome: () => void;
  onFocus: (focus: WorthFocus) => void;
}) {
  const t = useT();
  const today = todayIso();
  const display = (finance.snapshot.defaultCurrency || "").toUpperCase();
  const book = useMemo(() => bookFromStatus(finance.snapshot.fx), [finance.snapshot.fx]);
  const nowMs = useMemo(() => Date.now(), [finance.snapshot]);
  const worth = useMemo(
    () => netWorth(accounts.map((account) => ({ kind: account.kind, currency: account.currency, shownBalanceMinor: account.shownBalanceMinor }))),
    [accounts],
  );
  const bounds = monthBounds(today);
  const flow = useMemo(() => cashFlow(finance.snapshot.transactions, bounds.start, bounds.end), [finance.snapshot.transactions, bounds.start, bounds.end]);
  const previous = useMemo(
    () => cashFlow(finance.snapshot.transactions, bounds.prevStart, bounds.prevEnd),
    [finance.snapshot.transactions, bounds.prevStart, bounds.prevEnd],
  );
  const rules = finance.snapshot.rules;
  const txs = finance.snapshot.transactions;
  const expenses = useMemo(() => comingRows(rules, txs, today, "out"), [rules, txs, today]);
  const income = useMemo(() => comingRows(rules, txs, today, "in"), [rules, txs, today]);
  const planned = useMemo(() => plannedHits(rules, txs, today), [rules, txs, today]);
  const monthPlan = useMemo(() => projectWindow(flow, planned, today, bounds.end, today, true), [flow, planned, today, bounds.end]);
  const horizon = addDays(today, 30);
  const yesterday = addDays(today, -1);
  const nextPlan = useMemo(() => projectWindow([], planned, today, horizon, today, false), [planned, today, horizon]);
  const overduePlan = useMemo(() => projectWindow([], planned, "1970-01-01", yesterday, today, false), [planned, today, yesterday]);
  const worthParts = useMemo(() => worth.map((row) => ({ amountMinor: row.net, currency: row.currency })), [worth]);
  const flowParts = useMemo(() => flow.map((row) => ({ amountMinor: row.left, currency: row.currency })), [flow]);
  const expenseParts = useMemo(() => listed(expenses), [expenses]);
  const incomeParts = useMemo(() => listed(income), [income]);
  const planParts = useMemo(() => monthPlan.map((row) => ({ amountMinor: row.projected, currency: row.currency })), [monthPlan]);
  const moves = useMemo(
    () =>
      netWorthChange(
        txs.map((tx) => ({
          currency: tx.currency,
          direction: tx.direction,
          amountMinor: tx.amountMinor,
          date: tx.date,
          accountKind: accounts.find((account) => account.id === tx.accountId)?.kind || "",
          fromKind: accounts.find((account) => account.id === tx.fromAccountId)?.kind,
        })),
        bounds.start,
        bounds.end,
      ),
    [txs, accounts, bounds.start, bounds.end],
  );
  const heads = useMemo(() => {
    const nextIn = headlineOf(nextPlan.map((row) => ({ amountMinor: row.comingIn, currency: row.currency })), display, book, nowMs);
    const nextOut = headlineOf(nextPlan.map((row) => ({ amountMinor: row.comingOut, currency: row.currency })), display, book, nowMs);
    return {
      worth: headlineOf(worthParts, display, book, nowMs),
      month: headlineOf(flowParts, display, book, nowMs),
      expenses: headlineOf(expenseParts, display, book, nowMs),
      income: headlineOf(incomeParts, display, book, nowMs),
      plan: headlineOf(planParts, display, book, nowMs),
      next: subtractHeadlines(nextIn, nextOut, display),
      nextIn,
      nextOut,
      overdueIn: headlineOf(overduePlan.map((row) => ({ amountMinor: row.comingIn, currency: row.currency })), display, book, nowMs),
      overdueOut: headlineOf(overduePlan.map((row) => ({ amountMinor: row.comingOut, currency: row.currency })), display, book, nowMs),
      moves: headlineOf(moves.map((row) => ({ amountMinor: row.change, currency: row.currency })), display, book, nowMs),
      previous: headlineOf(previous.map((row) => ({ amountMinor: row.left, currency: row.currency })), display, book, nowMs),
    };
  }, [worthParts, flowParts, expenseParts, incomeParts, planParts, nextPlan, overduePlan, moves, previous, display, book, nowMs]);
  const gapCodes = useMemo(
    () => gapsOf([heads.worth, heads.month, heads.expenses, heads.income, heads.plan, heads.next, heads.overdueIn, heads.overdueOut, heads.moves]),
    [heads],
  );
  const gapKey = gapCodes.join(",");
  const tried = useRef("");
  useEffect(() => {
    if (!gapKey || !finance.inApp) return;
    if (tried.current === gapKey) return;
    tried.current = gapKey;
    finance.refreshRates();
  }, [gapKey, finance.inApp, finance.refreshRates]);

  return (
    <div className="fin-stack space-y-6">
      {gapCodes.length > 0 ? (
        <RatePanel codes={gapCodes} display={display} busy={finance.busy} onRefresh={() => finance.refreshRates()} onSave={finance.setManualRate} />
      ) : null}

      <section className="fin-panel">
        <h2 className="fin-kicker">{t("Net worth")}</h2>
        <p className="fin-muted mt-2 text-sm leading-5">{t("Right now. What you own minus what you owe.")}</p>
        {worth.length === 0 ? (
          <p className="mt-4 text-base">{t("No balances stated yet")}</p>
        ) : (
          <>
            <CombinedHeadline parts={worthParts} display={display} book={book} nowMs={nowMs} busy={finance.busy} size="text-5xl" />
            <div className="mt-4 grid grid-cols-2 gap-2">
              <FocusChip label={t("You own")} onClick={() => onFocus("owned")} />
              <FocusChip label={t("You owe")} onClick={() => onFocus("debt")} />
              <FocusChip label={t("Invested")} onClick={() => onFocus("invested")} />
              <FocusChip label={t("Cash & savings")} onClick={() => onFocus("cash")} />
            </div>
          </>
        )}
      </section>

      <section className="fin-panel">
        <h2 className="fin-kicker">{t("This month")}</h2>
        <p className="fin-muted mt-2 text-sm leading-5">{t("Income received minus expenses paid.")}</p>
        {flow.length === 0 ? (
          <p className="mt-4 text-base">{t("Nothing recorded this month")}</p>
        ) : (
          <>
            <CombinedHeadline parts={flowParts} display={display} book={book} nowMs={nowMs} busy={finance.busy} size="text-5xl" tone="auto" />
            {heads.previous.state === "ready" ? (
              <p className="fin-muted mt-2 text-sm">{fill("Last month left {amount}", { amount: formatMoney(heads.previous.amountMinor, heads.previous.currency) })}</p>
            ) : null}
          </>
        )}
        {heads.moves.state !== "empty" ? (
          <div className="mt-4 border-t border-border pt-3">
            <p className="text-sm font-medium">{t("Change this month")}</p>
            <p className="fin-muted mt-1 text-sm leading-5">{t("From what you recorded. A holding stays at the price you entered.")}</p>
            <CombinedHeadline
              parts={moves.map((row) => ({ amountMinor: row.change, currency: row.currency }))}
              display={display}
              book={book}
              nowMs={nowMs}
              busy={finance.busy}
              size="text-2xl"
              tone="auto"
            />
          </div>
        ) : null}
      </section>

      <Coming
        title={t("Coming expenses")}
        body={t("Bills, rent, subscriptions, and fixed runs.")}
        rows={expenses}
        parts={expenseParts}
        emptyTitle={t("Program a bill")}
        emptyBody={t("Rent, a subscription, or a course with an end.")}
        onEmpty={onProgram}
        onPaid={(id, date) => finance.markPaid(id, date)}
        onSkip={(id, date) => finance.scheduleAction(id, "skip", date)}
        paidLabel={t("Mark paid")}
        book={book}
        display={display}
        nowMs={nowMs}
        busy={finance.busy}
      />
      <Coming
        title={t("Coming income")}
        body={t("Paydays, planned income, and payouts.")}
        rows={income}
        parts={incomeParts}
        emptyTitle={t("Program income")}
        emptyBody={t("A payday, money you will get, or a payout.")}
        onEmpty={onIncome}
        onPaid={(id, date) => finance.markPaid(id, date)}
        onSkip={(id, date) => finance.scheduleAction(id, "skip", date)}
        paidLabel={t("Mark received")}
        book={book}
        display={display}
        nowMs={nowMs}
        busy={finance.busy}
      />

      <section className="fin-panel fin-planned">
        <div className="flex items-center justify-between gap-3">
          <h2 className="fin-kicker">{t("If these land")}</h2>
          <span className="stamp">{t("Not actual")}</span>
        </div>
        <p className="fin-muted mt-2 text-sm leading-5">{t("This month if the plan happens. Not a lifetime total.")}</p>
        {monthPlan.length === 0 ? <p className="mt-4 text-base">{t("Nothing planned in this window")}</p> : <StaticHeadline head={heads.plan} size="text-4xl" />}
      </section>
      <NextThirty nextIn={heads.nextIn} nextOut={heads.nextOut} net={heads.next} overdueIn={heads.overdueIn} overdueOut={heads.overdueOut} />
    </div>
  );
}

function NextThirty({
  nextIn,
  nextOut,
  net,
  overdueIn,
  overdueOut,
}: {
  nextIn: Headline;
  nextOut: Headline;
  net: Headline;
  overdueIn: Headline;
  overdueOut: Headline;
}) {
  const t = useT();
  const quiet = [net, nextIn, nextOut, overdueIn, overdueOut].every((head) => head.state === "empty");
  if (quiet) return null;
  return (
    <section className="fin-window">
      <h2 className="fin-kicker">{t("Next 30 days")}</h2>
      <p className="fin-muted mt-2 text-sm leading-5">{t("Planned in and out for the next 30 days, plus anything overdue. Not this month's leftover.")}</p>
      <StaticHeadline head={net} size="text-4xl" />
      {nextIn.state === "ready" || nextOut.state === "ready" ? (
        <div className="mt-3 grid grid-cols-2 gap-2">
          {nextIn.state === "ready" ? <Mini label={t("Incoming")} value={formatMoney(nextIn.amountMinor, nextIn.currency)} tone="in" /> : null}
          {nextOut.state === "ready" ? <Mini label={t("Outgoing")} value={formatMoney(nextOut.amountMinor, nextOut.currency)} tone="out" /> : null}
        </div>
      ) : null}
      {overdueIn.state === "ready" || overdueOut.state === "ready" ? (
        <p className="fin-muted mt-2 text-sm leading-5 break-words">
          {t("Overdue still open")}
          {overdueIn.state === "ready" && overdueIn.amountMinor ? ` · ${t("Incoming")} ${formatMoney(overdueIn.amountMinor, overdueIn.currency)}` : ""}
          {overdueOut.state === "ready" && overdueOut.amountMinor ? ` · ${t("Outgoing")} ${formatMoney(overdueOut.amountMinor, overdueOut.currency)}` : ""}
        </p>
      ) : null}
    </section>
  );
}

function Coming({
  title,
  body,
  rows,
  parts,
  emptyTitle,
  emptyBody,
  onEmpty,
  onPaid,
  onSkip,
  paidLabel,
  book,
  display,
  nowMs,
  busy,
}: {
  title: string;
  body: string;
  rows: ComingRow[];
  parts: MoneyPart[];
  emptyTitle: string;
  emptyBody: string;
  onEmpty: () => void;
  onPaid: (id: string, date: string) => void;
  onSkip: (id: string, date: string) => void;
  paidLabel: string;
  book: RateBook | null;
  display: string;
  nowMs: number;
  busy: boolean;
}) {
  const t = useT();
  const groups = ["overdue", "week", "month", "later"] as const;
  const label = { overdue: t("Overdue"), week: t("This week"), month: t("This month"), later: t("Later") };
  return (
    <section className="fin-panel">
      <h2 className="fin-kicker">{title}</h2>
      <p className="fin-muted mt-2 text-sm leading-5">{body}</p>
      {rows.length === 0 ? (
        <button type="button" className="mt-4 min-h-16 w-full text-left" onClick={onEmpty}>
          <p className="text-base font-extrabold tracking-wide uppercase">{emptyTitle}</p>
          <p className="fin-muted mt-1 text-sm leading-5">{emptyBody}</p>
        </button>
      ) : (
        <>
          <CombinedHeadline parts={parts} display={display} book={book} nowMs={nowMs} busy={busy} size="text-4xl" />
          {groups.map((bucket) => {
            const items = rows.filter((row) => row.bucket === bucket);
            if (items.length === 0) return null;
            return (
              <div key={bucket} className="mt-4">
                <p className="stamp">{label[bucket]}</p>
                <div className="mt-2 space-y-3">
                  {items.map((row) => (
                    <ComingLine key={`${row.id}-${row.date}`} row={row} paidLabel={paidLabel} onPaid={onPaid} onSkip={onSkip} book={book} display={display} nowMs={nowMs} />
                  ))}
                </div>
              </div>
            );
          })}
        </>
      )}
    </section>
  );
}

function ComingLine({
  row,
  paidLabel,
  onPaid,
  onSkip,
  book,
  display,
  nowMs,
}: {
  row: ComingRow;
  paidLabel: string;
  onPaid: (id: string, date: string) => void;
  onSkip: (id: string, date: string) => void;
  book: RateBook | null;
  display: string;
  nowMs: number;
}) {
  const t = useT();
  const converted = row.amountMinor == null ? null : headlineOf([{ amountMinor: row.amountMinor, currency: row.currency }], display, book, nowMs);
  const showConverted = converted && converted.state === "ready" && converted.source !== "identity";
  const progress = row.progress;
  const cadence =
    row.progress.mode === "open"
      ? row.purpose === "investment_payout"
        ? t("Investment payout")
        : t("No end date")
      : "";
  return (
    <div className={cn("border px-3 py-3", row.bucket === "overdue" ? "border-border" : "fin-planned")}>
      <div className="flex items-baseline justify-between gap-3">
        <div className="min-w-0">
          <p className="truncate text-base">{row.name}</p>
          <p className="fin-muted text-sm">
            {row.date}
            {cadence ? ` · ${cadence}` : ""}
          </p>
        </div>
        <div className="text-right">
          <p className="fin-num text-lg">{row.amountMinor != null ? formatMoney(row.amountMinor, row.currency) : ""}</p>
          {showConverted ? (
            <p className="fin-muted text-xs">
              {formatMoney(converted.amountMinor, converted.currency)}
              {converted.stale ? ` · ${t("Rates are stale")}` : ""}
            </p>
          ) : null}
        </div>
      </div>
      {progress.mode === "fixed" && progress.totalCount != null && progress.remainingCount != null ? (
        <p className="mt-2 text-sm">
          {fill("{paid} of {total}", { paid: progress.paidCount, total: progress.totalCount })}
          {" · "}
          {fill("{left} left", { left: progress.remainingCount })}
          {progress.remainingMinor != null ? ` · ${fill("{amount} remaining", { amount: formatMoney(progress.remainingMinor, progress.currency) })}` : ""}
        </p>
      ) : null}
      {progress.mode === "open" && progress.perPeriodMinor != null ? (
        <p className="fin-muted mt-2 text-sm">
          {formatMoney(progress.perPeriodMinor, progress.currency)} · {t("No end date")}
        </p>
      ) : null}
      {row.track ? (
        <p className="fin-muted mt-2 text-sm leading-5">
          {row.track.expectedMinor != null
            ? fill("Expected {amount}", { amount: formatMoney(row.track.expectedMinor, row.track.expectedCurrency) })
            : t("Expected")}
          {" · "}
          {row.track.received.length === 0
            ? t("None received yet")
            : row.track.received.map((item) => formatMoney(item.amountMinor, item.currency)).join(", ")}
          {row.amountMode === "quote" && row.payoutCurrency ? ` · ${fill("Worth of {code}", { code: row.payoutCurrency })}` : ""}
        </p>
      ) : null}
      <div className="mt-3 flex flex-wrap gap-2">
        <button type="button" className="stamp min-h-12 max-w-full px-3 whitespace-normal text-center leading-4" onClick={() => onPaid(row.id, row.date)}>
          {paidLabel}
        </button>
        <button type="button" className="stamp min-h-12 px-3" onClick={() => onSkip(row.id, row.date)}>
          {t("Skip")}
        </button>
      </div>
    </div>
  );
}

function FocusChip({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button type="button" className="fin-chip w-full" onClick={onClick}>
      {label}
    </button>
  );
}

function Mini({ label, value, tone }: { label: string; value: string; tone?: "in" | "out" }) {
  return (
    <div className="min-w-0">
      <p className="text-xs leading-4 tracking-wide break-words text-muted-foreground uppercase">{label}</p>
      <p className={cn("fin-num mt-1 text-base font-medium break-words", tone === "in" && "fin-in", tone === "out" && "fin-out")}>{value}</p>
    </div>
  );
}

export function plannedYearBars(rules: FinanceRule[], txs: FinanceTransaction[], today = todayIso()) {
  const horizon = addMonths(today, 12);
  const hits = plannedHits(rules, txs, today).filter((hit) => hit.date >= today && hit.date < horizon && hit.amountMinor != null);
  const by = new Map<string, { income: number; expense: number }>();
  for (const hit of hits) {
    const row = by.get(hit.currency) ?? { income: 0, expense: 0 };
    if (hit.direction === "income") row.income += hit.amountMinor || 0;
    else row.expense += hit.amountMinor || 0;
    by.set(hit.currency, row);
  }
  return [...by.entries()].map(([currency, row]) => ({ currency, ...row }));
}
