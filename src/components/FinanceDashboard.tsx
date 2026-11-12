import { useEffect, useMemo, useRef, type ReactNode } from "react";
import type { FinanceState } from "@/hooks/useFinance";
import type { FinanceAccount, FinanceRule, FinanceTransaction } from "@/lib/api";
import { CombinedHeadline, StaticHeadline } from "@/components/CombinedTotal";
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
import { financePanels, type FinancePlace } from "@/lib/financeTabs";
import { cn } from "@/lib/utils";

export type { FinancePlace };
export { financePanels };

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

function periodParts(rows: FinanceTransaction[], start: string, end: string, direction: string): MoneyPart[] {
  return rows.flatMap((tx) => {
    if (tx.direction !== direction || !tx.currency) return [];
    if (start && tx.date < start) return [];
    if (end && tx.date > end) return [];
    return [{ amountMinor: tx.amountMinor, currency: tx.currency }];
  });
}

/** Codes the combined headlines still cannot convert. Same book the Overview uses. Empty when every pair is known. */
export function missingRateKey(snapshot: FinanceState["snapshot"], nowMs: number): string {
  const accounts = snapshot.accounts;
  const today = todayIso();
  const display = (snapshot.defaultCurrency || "").toUpperCase();
  const book = bookFromStatus(snapshot.fx);
  const worth = netWorth(accounts.map((account) => ({ kind: account.kind, currency: account.currency, shownBalanceMinor: account.shownBalanceMinor })));
  const bounds = monthBounds(today);
  const flow = cashFlow(snapshot.transactions, bounds.start, bounds.end);
  const expenses = comingRows(snapshot.rules, snapshot.transactions, today, "out");
  const income = comingRows(snapshot.rules, snapshot.transactions, today, "in");
  const planned = plannedHits(snapshot.rules, snapshot.transactions, today);
  const monthPlan = projectWindow(flow, planned, today, bounds.end, today, true);
  const horizon = addDays(today, 30);
  const yesterday = addDays(today, -1);
  const nextPlan = projectWindow([], planned, today, horizon, today, false);
  const overduePlan = projectWindow([], planned, "1970-01-01", yesterday, today, false);
  const moves = netWorthChange(
    snapshot.transactions.map((tx) => ({
      currency: tx.currency,
      direction: tx.direction,
      amountMinor: tx.amountMinor,
      date: tx.date,
      accountKind: accounts.find((account) => account.id === tx.accountId)?.kind || "",
      fromKind: accounts.find((account) => account.id === tx.fromAccountId)?.kind,
    })),
    bounds.start,
    bounds.end,
  );
  const nextIn = headlineOf(nextPlan.map((row) => ({ amountMinor: row.comingIn, currency: row.currency })), display, book, nowMs);
  const nextOut = headlineOf(nextPlan.map((row) => ({ amountMinor: row.comingOut, currency: row.currency })), display, book, nowMs);
  return gapsOf([
    headlineOf(worth.map((row) => ({ amountMinor: row.net, currency: row.currency })), display, book, nowMs),
    headlineOf(flow.map((row) => ({ amountMinor: row.left, currency: row.currency })), display, book, nowMs),
    headlineOf(listed(expenses), display, book, nowMs),
    headlineOf(listed(income), display, book, nowMs),
    headlineOf(monthPlan.map((row) => ({ amountMinor: row.projected, currency: row.currency })), display, book, nowMs),
    subtractHeadlines(nextIn, nextOut, display),
    headlineOf(overduePlan.map((row) => ({ amountMinor: row.comingIn, currency: row.currency })), display, book, nowMs),
    headlineOf(overduePlan.map((row) => ({ amountMinor: row.comingOut, currency: row.currency })), display, book, nowMs),
    headlineOf(moves.map((row) => ({ amountMinor: row.change, currency: row.currency })), display, book, nowMs),
    headlineOf(periodParts(snapshot.transactions, bounds.start, bounds.end, "expense"), display, book, nowMs),
    headlineOf(periodParts(snapshot.transactions, bounds.start, bounds.end, "income"), display, book, nowMs),
  ]).join(",");
}

/** Fetch when a headline is blocked. Runs from FinancePage on every tab, above any early return. */
export function useFinanceRateWatch(finance: FinanceState) {
  const nowMs = useMemo(() => Date.now(), [finance.snapshot]);
  const gapKey = useMemo(() => missingRateKey(finance.snapshot, nowMs), [finance.snapshot, nowMs]);
  const tried = useRef("");
  useEffect(() => {
    if (!gapKey || !finance.inApp) return;
    if (tried.current === gapKey) return;
    tried.current = gapKey;
    finance.refreshRates();
  }, [gapKey, finance.inApp, finance.refreshRates]);
}

export function FinanceDashboard({
  finance,
  accounts,
  place,
  onProgram,
  onIncome,
  onPayout,
  onFocus,
  onOpenExpenses,
  onOpenIncome,
  children,
}: {
  finance: FinanceState;
  accounts: FinanceAccount[];
  place: FinancePlace;
  onProgram: () => void;
  onIncome: () => void;
  onPayout?: () => void;
  onFocus: (focus: WorthFocus) => void;
  onOpenExpenses?: () => void;
  onOpenIncome?: () => void;
  children?: ReactNode;
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
  const comingOut = useMemo(() => comingRows(rules, txs, today, "out"), [rules, txs, today]);
  const comingIn = useMemo(() => comingRows(rules, txs, today, "in"), [rules, txs, today]);
  const payoutRows = useMemo(() => comingIn.filter((row) => row.purpose === "investment_payout"), [comingIn]);
  const incomeRows = useMemo(() => comingIn.filter((row) => row.purpose !== "investment_payout"), [comingIn]);
  const planned = useMemo(() => plannedHits(rules, txs, today), [rules, txs, today]);
  const monthPlan = useMemo(() => projectWindow(flow, planned, today, bounds.end, today, true), [flow, planned, today, bounds.end]);
  const horizon = addDays(today, 30);
  const yesterday = addDays(today, -1);
  const nextPlan = useMemo(() => projectWindow([], planned, today, horizon, today, false), [planned, today, horizon]);
  const overduePlan = useMemo(() => projectWindow([], planned, "1970-01-01", yesterday, today, false), [planned, today, yesterday]);
  const worthParts = useMemo(() => worth.map((row) => ({ amountMinor: row.net, currency: row.currency })), [worth]);
  const flowParts = useMemo(() => flow.map((row) => ({ amountMinor: row.left, currency: row.currency })), [flow]);
  const spentParts = useMemo(() => periodParts(txs, bounds.start, bounds.end, "expense"), [txs, bounds.start, bounds.end]);
  const paidParts = useMemo(() => periodParts(txs, bounds.start, bounds.end, "payment"), [txs, bounds.start, bounds.end]);
  const earnedParts = useMemo(() => periodParts(txs, bounds.start, bounds.end, "income"), [txs, bounds.start, bounds.end]);
  const comingOutParts = useMemo(() => listed(comingOut), [comingOut]);
  const incomeParts = useMemo(() => listed(incomeRows), [incomeRows]);
  const payoutParts = useMemo(() => listed(payoutRows), [payoutRows]);
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
      plan: headlineOf(planParts, display, book, nowMs),
      next: subtractHeadlines(nextIn, nextOut, display),
      nextIn,
      nextOut,
      overdueIn: headlineOf(overduePlan.map((row) => ({ amountMinor: row.comingIn, currency: row.currency })), display, book, nowMs),
      overdueOut: headlineOf(overduePlan.map((row) => ({ amountMinor: row.comingOut, currency: row.currency })), display, book, nowMs),
      moves: headlineOf(moves.map((row) => ({ amountMinor: row.change, currency: row.currency })), display, book, nowMs),
      previous: headlineOf(previous.map((row) => ({ amountMinor: row.left, currency: row.currency })), display, book, nowMs),
    };
  }, [planParts, nextPlan, overduePlan, moves, previous, display, book, nowMs]);
  const panels = financePanels(place);
  const openExpenses = onOpenExpenses ?? onProgram;
  const openIncome = onOpenIncome ?? onIncome;
  const openPayout = onPayout ?? onIncome;

  return (
    <div className="fin-stack w-full min-w-0 space-y-6" data-finance-place={place}>
      {panels.includes("net-worth") ? (
        <section className="fin-panel" data-finance-panel="net-worth">
          <h2 className="fin-kicker">{t("Net worth")}</h2>
          <p className="fin-muted mt-2 text-sm leading-5">{t("Right now. What you own minus what you owe.")}</p>
          {worth.length === 0 ? (
            <p className="mt-4 text-base">{t("No balances stated yet")}</p>
          ) : (
            <CombinedHeadline parts={worthParts} display={display} book={book} nowMs={nowMs} busy={finance.busy} size="text-5xl" />
          )}
          <div className="mt-4 grid w-full min-w-0 grid-cols-2 gap-2">
            <FocusChip label={t("You own")} onClick={() => onFocus("owned")} />
            <FocusChip label={t("You owe")} onClick={() => onFocus("debt")} />
            <FocusChip label={t("Invested")} onClick={() => onFocus("invested")} />
            <FocusChip label={t("Cash & savings")} onClick={() => onFocus("cash")} />
          </div>
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
      ) : null}

      {panels.includes("expenses") ? (
        <section className="fin-panel" data-finance-panel="expenses">
          <h2 className="fin-kicker">{t("This month's expenses")}</h2>
          <p className="fin-muted mt-2 text-sm leading-5">{t("Paid this month.")}</p>
          {spentParts.length === 0 && paidParts.length === 0 ? (
            <p className="mt-4 text-base">{t("Nothing recorded this month")}</p>
          ) : spentParts.length > 0 ? (
            <CombinedHeadline parts={spentParts} display={display} book={book} nowMs={nowMs} busy={finance.busy} size="text-5xl" tone="out" />
          ) : null}
          {paidParts.length > 0 ? (
            <div className="mt-4 border-t border-border pt-3">
              <p className="text-sm font-medium">{t("Payments")}</p>
              <CombinedHeadline parts={paidParts} display={display} book={book} nowMs={nowMs} busy={finance.busy} size="text-2xl" tone="out" />
            </div>
          ) : null}
        </section>
      ) : null}

      {place === "expenses" && panels.includes("activity") ? children : null}

      {panels.includes("coming-expenses") ? (
        <Coming
          title={t("Coming expenses")}
          body={t("Bills, rent, subscriptions, and fixed runs.")}
          rows={comingOut}
          parts={comingOutParts}
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
      ) : null}

      {panels.includes("income") ? (
        <section className="fin-panel" data-finance-panel="income">
          <h2 className="fin-kicker">{t("This month's income")}</h2>
          <p className="fin-muted mt-2 text-sm leading-5">{t("Received this month.")}</p>
          {earnedParts.length === 0 ? (
            <p className="mt-4 text-base">{t("Nothing recorded this month")}</p>
          ) : (
            <CombinedHeadline parts={earnedParts} display={display} book={book} nowMs={nowMs} busy={finance.busy} size="text-5xl" tone="in" />
          )}
        </section>
      ) : null}

      {place === "income" && panels.includes("activity") ? children : null}

      {panels.includes("coming-income") ? (
        <Coming
          title={t("Coming income")}
          body={t("Paydays, planned income, and payouts.")}
          rows={incomeRows}
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
      ) : null}

      {panels.includes("payouts") ? (
        <Coming
          title={t("Investment payouts")}
          body={t("The payout only. The principal stays what you entered.")}
          rows={payoutRows}
          parts={payoutParts}
          emptyTitle={t("Program income")}
          emptyBody={t("A payday, money you will get, or a payout.")}
          onEmpty={openPayout}
          onPaid={(id, date) => finance.markPaid(id, date)}
          onSkip={(id, date) => finance.scheduleAction(id, "skip", date)}
          paidLabel={t("Mark received")}
          book={book}
          display={display}
          nowMs={nowMs}
          busy={finance.busy}
        />
      ) : null}

      {panels.includes("result") ? (
        <section className="fin-panel" data-finance-panel="result">
          <h2 className="fin-kicker">{t("This month's result")}</h2>
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
        </section>
      ) : null}

      {panels.includes("upcoming") ? (
        <section className="fin-panel" data-finance-panel="upcoming">
          <h2 className="fin-kicker">{t("Upcoming")}</h2>
          <p className="fin-muted mt-2 text-sm leading-5">{t("Still ahead. Open a tab for the full list.")}</p>
          <button type="button" className="mt-4 block w-full text-left" onClick={openExpenses}>
            <p className="text-sm font-extrabold tracking-wide uppercase">{t("Coming expenses")}</p>
            {comingOut.length === 0 ? (
              <p className="fin-muted mt-1 text-sm">{t("Nothing planned in this window")}</p>
            ) : (
              <CombinedHeadline parts={comingOutParts} display={display} book={book} nowMs={nowMs} busy={finance.busy} size="text-3xl" tone="out" />
            )}
          </button>
          <button type="button" className="mt-4 block w-full text-left" onClick={openIncome}>
            <p className="text-sm font-extrabold tracking-wide uppercase">{t("Coming income")}</p>
            {comingIn.length === 0 ? (
              <p className="fin-muted mt-1 text-sm">{t("Nothing planned in this window")}</p>
            ) : (
              <CombinedHeadline parts={listed(comingIn)} display={display} book={book} nowMs={nowMs} busy={finance.busy} size="text-3xl" tone="in" />
            )}
          </button>
          {heads.plan.state !== "empty" ? (
            <div className="mt-4 border-t border-border pt-3">
              <div className="flex items-center justify-between gap-3">
                <p className="text-sm font-medium">{t("If these land")}</p>
                <span className="stamp">{t("Not actual")}</span>
              </div>
              <p className="fin-muted mt-1 text-sm leading-5">{t("This month if the plan happens. Not a lifetime total.")}</p>
              <StaticHeadline head={heads.plan} size="text-3xl" />
            </div>
          ) : null}
          <NextThirty nextIn={heads.nextIn} nextOut={heads.nextOut} net={heads.next} overdueIn={heads.overdueIn} overdueOut={heads.overdueOut} />
        </section>
      ) : null}
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
    <section className="fin-window mt-4">
      <h2 className="fin-kicker">{t("Next 30 days")}</h2>
      <p className="fin-muted mt-2 text-sm leading-5">{t("Planned in and out for the next 30 days, plus anything overdue. Not this month's leftover.")}</p>
      <StaticHeadline head={net} size="text-4xl" />
      {nextIn.state === "ready" || nextOut.state === "ready" ? (
        <div className="mt-3 grid w-full min-w-0 grid-cols-2 gap-2">
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
