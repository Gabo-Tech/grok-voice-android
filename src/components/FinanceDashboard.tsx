import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Info, Repeat } from "lucide-react";
import type { FinanceState } from "@/hooks/useFinance";
import type { FinanceAccount, FinanceRule, FinanceTransaction } from "@/lib/api";
import { CombinedHeadline, StaticHeadline } from "@/components/CombinedTotal";
import { Loader } from "@/components/Loader";
import {
  addDays,
  addMonths,
  bookFromStatus,
  cashFlow,
  comingCashRows,
  headlineOf,
  monthBounds,
  netWorth,
  worthAccount,
  netWorthChange,
  plannedHits,
  projectWindow,
  subtractHeadlines,
  type ComingRow,
  type Headline,
  type MoneyPart,
  type RateBook,
} from "@/lib/financeMath";
import { selectDebt, type DebtSelection, type DebtSliceId } from "@/lib/financeDebt";
import { flowParts as flowPartsOf, monthActivity, monthCash, netWorthParts, worthBuckets, type CashTx } from "@/lib/financeLedger";
import { formatMoney, moneyParts } from "@/lib/money";
import { ChartColorMenu } from "@/components/ColorSheet";
import { resolveColor } from "@/lib/entityColor";
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
  const worth = netWorth(accounts.map((account) => worthAccount(account)));
  const bounds = monthBounds(today);
  const flow = cashFlow(snapshot.transactions, bounds.start, bounds.end);
  const cards = snapshot.accounts.map(cardShape);
  const expenses = comingCashRows(snapshot.rules, snapshot.transactions, cards, today, "out");
  const income = comingCashRows(snapshot.rules, snapshot.transactions, cards, today, "in");
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
  onOpenRule,
  onRefreshPrices,
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
  onOpenRule?: (id: string) => void;
  onRefreshPrices?: () => void;
  children?: ReactNode;
}) {
  const t = useT();
  const today = todayIso();
  const display = (finance.snapshot.defaultCurrency || "").toUpperCase();
  const book = useMemo(() => bookFromStatus(finance.snapshot.fx), [finance.snapshot.fx]);
  const nowMs = useMemo(() => Date.now(), [finance.snapshot]);
  const quotes = finance.snapshot.prices?.quotes || [];
  const worth = useMemo(
    () => netWorth(accounts.map((account) => worthAccount(account))),
    [accounts],
  );
  const net = useMemo(() => netWorthParts(accounts, display, book, quotes, nowMs), [accounts, display, book, quotes, nowMs]);
  const buckets = useMemo(() => worthBuckets(accounts, display, book, quotes, nowMs), [accounts, display, book, quotes, nowMs]);
  const bounds = monthBounds(today);
  const flow = useMemo(() => cashFlow(finance.snapshot.transactions, bounds.start, bounds.end), [finance.snapshot.transactions, bounds.start, bounds.end]);
  const previous = useMemo(
    () => cashFlow(finance.snapshot.transactions, bounds.prevStart, bounds.prevEnd),
    [finance.snapshot.transactions, bounds.prevStart, bounds.prevEnd],
  );
  const rules = finance.snapshot.rules;
  const txs = finance.snapshot.transactions;
  const cardAccounts = useMemo(() => accounts.map(cardShape), [accounts]);
  const comingOut = useMemo(() => comingCashRows(rules, txs, cardAccounts, today, "out"), [rules, txs, cardAccounts, today]);
  const comingIn = useMemo(() => comingCashRows(rules, txs, cardAccounts, today, "in"), [rules, txs, cardAccounts, today]);
  const payoutRows = useMemo(() => comingIn.filter((row) => row.purpose === "investment_payout"), [comingIn]);
  const incomeRows = useMemo(() => comingIn.filter((row) => row.purpose !== "investment_payout"), [comingIn]);
  const planned = useMemo(() => plannedHits(rules, txs, today), [rules, txs, today]);
  const monthPlan = useMemo(() => projectWindow(flow, planned, today, bounds.end, today, true), [flow, planned, today, bounds.end]);
  const horizon = addDays(today, 30);
  const yesterday = addDays(today, -1);
  const nextPlan = useMemo(() => projectWindow([], planned, today, horizon, today, false), [planned, today, horizon]);
  const overduePlan = useMemo(() => projectWindow([], planned, "1970-01-01", yesterday, today, false), [planned, today, yesterday]);
  const worthParts = net.parts.length > 0 ? net.parts : worth.map((row) => ({ amountMinor: row.net, currency: row.currency }));
  const debt = useMemo(
    () =>
      selectDebt(
        accounts.map((account) => ({
          id: account.id,
          name: account.name,
          kind: account.kind,
          currency: account.currency,
          shownBalanceMinor: account.shownBalanceMinor,
        })),
        planned.map((hit) => ({
          id: `${hit.scheduleId}-${hit.date}`,
          name: hit.name,
          amountMinor: hit.amountMinor,
          currency: hit.currency,
          date: hit.date,
          direction: hit.direction,
          accountId: hit.accountId,
        })),
        display,
        book,
        nowMs,
        today,
      ),
    [accounts, planned, display, book, nowMs, today],
  );
  const worthShown = useMemo(() => {
    const bills = debt.slices.find((slice) => slice.id === "bills");
    const extra = (bills?.items || []).filter((item) => item.amountMinor > 0).map((item) => ({ amountMinor: -item.amountMinor, currency: item.currency }));
    return [...worthParts, ...extra];
  }, [worthParts, debt]);
  const realized = useMemo(
    () =>
      monthActivity(
        txs,
        planned
          .filter((hit) => !txs.some((tx) => tx.ruleId === hit.scheduleId && (tx.occurrenceDate || tx.date) === hit.date))
          .map((hit) => ({ date: hit.date, direction: hit.direction, amountMinor: hit.amountMinor, currency: hit.currency })),
        bounds.start,
        bounds.end,
        today,
      ),
    [txs, planned, bounds.start, bounds.end, today],
  );
  const spentParts = useMemo(() => flowPartsOf(realized, bounds.start, bounds.end, "expense"), [realized, bounds.start, bounds.end]);
  const paidParts = useMemo(() => periodParts(txs, bounds.start, bounds.end, "payment"), [txs, bounds.start, bounds.end]);
  const earnedParts = useMemo(() => flowPartsOf(realized, bounds.start, bounds.end, "income"), [realized, bounds.start, bounds.end]);
  const resultFlow = useMemo(() => cashFlow(realized, bounds.start, bounds.end), [realized, bounds.start, bounds.end]);
  const flowParts = useMemo(() => resultFlow.map((row) => ({ amountMinor: row.left, currency: row.currency })), [resultFlow]);
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
    <div className="fin-stack w-full min-w-0 space-y-2" data-finance-place={place}>
      {finance.refreshing ? <Loader size="block" label={t("Checking rates")} /> : null}
      {panels.includes("net-worth") ? (
        <section className="fin-panel fin-tight" data-finance-panel="net-worth">
          <h2 className="fin-kicker">{t("Net worth")}</h2>
          <p className="fin-muted mt-1 text-sm leading-5">{t("Right now. What you own minus what you owe.")}</p>
          {worthShown.length === 0 ? (
            <p className="mt-3 text-base">{t("No balances stated yet")}</p>
          ) : (
            <CombinedHeadline parts={worthShown} display={display} book={book} nowMs={nowMs} busy={finance.refreshing} size="text-5xl" />
          )}
          {net.unpriced.length > 0 ? (
            <button type="button" className="fin-chip mt-2" onClick={onRefreshPrices}>
              {fill("No price for {name}", { name: net.unpriced.join(", ") })}
              {" · "}
              {t("Refresh price now")}
            </button>
          ) : null}
          <WorthRows buckets={buckets.buckets} owed={debt.amountMinor} display={display} onFocus={onFocus} />
          <DebtBlock debt={debt} display={display} />
        </section>
      ) : null}

      {panels.includes("month") ? (
        <div data-finance-panel="month">
          <MonthSection txs={txs} accounts={accounts} display={display} book={book} nowMs={nowMs} bounds={bounds} entityColors={finance.snapshot.entityColors} onPaint={finance.paintColor} />
        </div>
      ) : null}

      {panels.includes("next") ? (
        <section className="fin-panel fin-card" data-finance-panel="next">
          <h2 className="fin-kicker">{t("Next 30 days")}</h2>
          <p className="fin-muted mt-2 text-[13px] leading-5">{t("Bills from cash or a bank, and card payments that are due.")}</p>
          <button type="button" className="mt-3 block min-h-11 w-full text-left" onClick={openExpenses}>
            <p className="text-[13px] font-extrabold tracking-wide uppercase">{t("Due")}</p>
            {comingOut.length === 0 ? <p className="fin-muted mt-1 text-[13px]">{t("Nothing due in the next 30 days.")}</p> : <CombinedHeadline parts={comingOutParts} display={display} book={book} nowMs={nowMs} busy={finance.refreshing} size="text-3xl" tone="out" />}
          </button>
          <button type="button" className="mt-2 block min-h-11 w-full text-left" onClick={openIncome}>
            <p className="text-[13px] font-extrabold tracking-wide uppercase">{t("Coming in")}</p>
            {comingIn.length === 0 ? <p className="fin-muted mt-1 text-[13px]">{t("Nothing due in the next 30 days.")}</p> : <CombinedHeadline parts={listed(comingIn)} display={display} book={book} nowMs={nowMs} busy={finance.refreshing} size="text-3xl" tone="in" />}
          </button>
        </section>
      ) : null}

      {panels.includes("expenses") ? (
        <section className="fin-panel" data-finance-panel="expenses">
          <h2 className="fin-kicker">{t("This month's expenses")}</h2>
          <p className="fin-muted mt-2 text-sm leading-5">{t("Paid this month.")}</p>
          {spentParts.length === 0 && paidParts.length === 0 ? (
            <p className="mt-4 text-base">{t("Nothing recorded this month")}</p>
          ) : spentParts.length > 0 ? (
            <CombinedHeadline parts={spentParts} display={display} book={book} nowMs={nowMs} busy={finance.refreshing} size="text-5xl" tone="out" />
          ) : null}
          {paidParts.length > 0 ? (
            <div className="mt-4 border-t border-border pt-3">
              <p className="text-sm font-medium">{t("Payments")}</p>
              <CombinedHeadline parts={paidParts} display={display} book={book} nowMs={nowMs} busy={finance.refreshing} size="text-2xl" tone="out" />
            </div>
          ) : null}
        </section>
      ) : null}

      {place === "activity" && panels.includes("activity") ? children : null}

      {panels.includes("coming-expenses") ? (
        <Coming
          title={t("Coming expenses")}
          body={place === "plan" ? "" : t("The next 30 days. Bills paid from cash or a bank, and credit-card payments that are due.")}
          rows={comingOut}
          parts={comingOutParts}
          emptyTitle={t("Program a bill")}
          emptyBody={place === "plan" ? "" : t("Rent, a subscription, or a course with an end.")}
          onEmpty={onProgram}
          onOpen={onOpenRule}
          onPaid={(id, date) => finance.markPaid(id, date)}
          onSkip={(id, date) => finance.scheduleAction(id, "skip", date)}
          paidLabel={t("Mark paid")}
          book={book}
          display={display}
          nowMs={nowMs}
          busy={finance.refreshing}
          dense={place === "plan"}
          today={today}
          colors={Object.fromEntries(finance.snapshot.categories.map((row) => [row.name, resolveColor(finance.snapshot.entityColors, row.kind === "income" ? "income" : "category", row.name, row.color)]))}
        />
      ) : null}

      {panels.includes("income") ? (
        <section className="fin-panel" data-finance-panel="income">
          <h2 className="fin-kicker">{t("This month's income")}</h2>
          <p className="fin-muted mt-2 text-sm leading-5">{t("Received this month.")}</p>
          {earnedParts.length === 0 ? (
            <p className="mt-4 text-base">{t("Nothing recorded this month")}</p>
          ) : (
            <CombinedHeadline parts={earnedParts} display={display} book={book} nowMs={nowMs} busy={finance.refreshing} size="text-5xl" tone="in" />
          )}
        </section>
      ) : null}

      {place === "income" && panels.includes("activity") ? children : null}

      {panels.includes("coming-income") ? (
        <Coming
          title={t("Coming income")}
          body={place === "plan" ? "" : t("The next 30 days.")}
          rows={incomeRows}
          parts={incomeParts}
          emptyTitle={t("Program income")}
          emptyBody={place === "plan" ? "" : t("A payday, money you will get, or a payout.")}
          onEmpty={onIncome}
          onOpen={onOpenRule}
          dense={place === "plan"}
          today={today}
          colors={Object.fromEntries(finance.snapshot.categories.map((row) => [row.name, resolveColor(finance.snapshot.entityColors, row.kind === "income" ? "income" : "category", row.name, row.color)]))}
          onPaid={(id, date) => finance.markPaid(id, date)}
          onSkip={(id, date) => finance.scheduleAction(id, "skip", date)}
          paidLabel={t("Mark received")}
          book={book}
          display={display}
          nowMs={nowMs}
          busy={finance.refreshing}
        />
      ) : null}

      {panels.includes("payouts") ? (
        <Coming
          title={t("Investment payouts")}
          body={place === "plan" ? "" : t("The payout only. The principal stays what you entered.")}
          rows={payoutRows}
          parts={payoutParts}
          emptyTitle={t("Program income")}
          emptyBody={place === "plan" ? "" : t("A payday, money you will get, or a payout.")}
          onEmpty={openPayout}
          onOpen={onOpenRule}
          dense={place === "plan"}
          today={today}
          colors={Object.fromEntries(finance.snapshot.categories.map((row) => [row.name, resolveColor(finance.snapshot.entityColors, row.kind === "income" ? "income" : "category", row.name, row.color)]))}
          onPaid={(id, date) => finance.markPaid(id, date)}
          onSkip={(id, date) => finance.scheduleAction(id, "skip", date)}
          paidLabel={t("Mark received")}
          book={book}
          display={display}
          nowMs={nowMs}
          busy={finance.refreshing}
        />
      ) : null}

      {panels.includes("card-debt") ? (
        <section className="fin-panel" data-finance-panel="card-debt">
          <h2 className="fin-kicker">{t("Debt")}</h2>
          <p className="fin-muted mt-2 text-sm leading-5">{t("Cards, loans, people, and overdue bills. A future bill is not included.")}</p>
          {debt.amountMinor <= 0 ? <p className="mt-4 text-base">{t("No debt")}</p> : <DebtBlock debt={debt} display={display} />}
        </section>
      ) : null}

      {panels.includes("result") ? (
        <section className="fin-panel" data-finance-panel="result">
          <h2 className="fin-kicker">{t("This month's result")}</h2>
          <p className="fin-muted mt-2 text-[13px] leading-5">{t("Income minus expenses. A charge on a card counts once.")}</p>
          {flow.length === 0 ? (
            <p className="mt-4 text-base">{t("Nothing recorded this month")}</p>
          ) : (
            <>
              <CombinedHeadline parts={flowParts} display={display} book={book} nowMs={nowMs} busy={finance.refreshing} size="text-5xl" tone="auto" />
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
              <CombinedHeadline parts={comingOutParts} display={display} book={book} nowMs={nowMs} busy={finance.refreshing} size="text-3xl" tone="out" />
            )}
          </button>
          <button type="button" className="mt-4 block w-full text-left" onClick={openIncome}>
            <p className="text-sm font-extrabold tracking-wide uppercase">{t("Coming income")}</p>
            {comingIn.length === 0 ? (
              <p className="fin-muted mt-1 text-sm">{t("Nothing planned in this window")}</p>
            ) : (
              <CombinedHeadline parts={listed(comingIn)} display={display} book={book} nowMs={nowMs} busy={finance.refreshing} size="text-3xl" tone="in" />
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
          <OverdueLine overdueIn={heads.overdueIn} overdueOut={heads.overdueOut} />
        </section>
      ) : null}
    </div>
  );
}

function OverdueLine({ overdueIn, overdueOut }: { overdueIn: Headline; overdueOut: Headline }) {
  const t = useT();
  const open = (overdueIn.state === "ready" && overdueIn.amountMinor !== 0) || (overdueOut.state === "ready" && overdueOut.amountMinor !== 0);
  if (!open) return null;
  return (
    <p className="fin-muted mt-3 text-sm leading-5 break-words">
      {t("Overdue still open")}
      {overdueIn.state === "ready" && overdueIn.amountMinor ? ` · ${t("Incoming")} ${formatMoney(overdueIn.amountMinor, overdueIn.currency)}` : ""}
      {overdueOut.state === "ready" && overdueOut.amountMinor ? ` · ${t("Outgoing")} ${formatMoney(overdueOut.amountMinor, overdueOut.currency)}` : ""}
    </p>
  );
}

function cardShape(account: FinanceAccount) {
  return {
    id: account.id,
    name: account.name,
    kind: account.kind,
    currency: account.currency,
    shownBalanceMinor: account.shownBalanceMinor,
    paymentMinor: account.paymentMinor,
    dueDate: account.dueDate,
    dueDay: account.dueDay,
  };
}

function Coming({
  title,
  body,
  rows,
  parts,
  emptyTitle,
  emptyBody,
  onEmpty,
  onOpen,
  onPaid,
  onSkip,
  paidLabel,
  book,
  display,
  nowMs,
  busy,
  dense = false,
  today = "",
  colors = {},
}: {
  title: string;
  body: string;
  rows: ComingRow[];
  parts: MoneyPart[];
  emptyTitle: string;
  emptyBody: string;
  onEmpty: () => void;
  onOpen?: (id: string) => void;
  onPaid: (id: string, date: string) => void;
  onSkip: (id: string, date: string) => void;
  paidLabel: string;
  book: RateBook | null;
  display: string;
  nowMs: number;
  busy: boolean;
  dense?: boolean;
  today?: string;
  colors?: Record<string, string>;
}) {
  const t = useT();
  const [info, setInfo] = useState(false);
  const groups = dense ? (["week", "later"] as const) : (["overdue", "week", "month", "later"] as const);
  const label = { overdue: t("Overdue"), week: t("This week"), month: t("This month"), later: t("Later") };
  const inGroup = (row: ComingRow, bucket: string) => {
    if (!dense) return row.bucket === bucket;
    if (bucket === "week") return row.bucket === "overdue" || row.bucket === "week";
    return row.bucket === "month" || row.bucket === "later";
  };
  const natives = parts.map((part) => formatMoney(part.amountMinor, part.currency)).join(" · ");
  return (
    <section className="fin-panel">
      <div className="flex items-center justify-between gap-3">
        <h2 className="fin-kicker">{title}</h2>
        {dense ? <span className="plan-chip">30d</span> : null}
      </div>
      {body ? <p className="fin-muted mt-2 text-sm leading-5">{body}</p> : null}
      {rows.length === 0 ? (
        <button type="button" className="mt-4 min-h-11 w-full text-left text-[16px]" onClick={onEmpty}>
          {emptyTitle}
          {emptyBody ? <span className="fin-muted mt-1 block text-[13px]">{emptyBody}</span> : null}
        </button>
      ) : (
        <>
          <div className="mt-3 flex items-start justify-between gap-3">
            <CombinedHeadline parts={parts} display={display} book={book} nowMs={nowMs} busy={busy} size="text-4xl" quiet={dense} />
            {dense ? (
              <button type="button" className="plan-chevron" aria-label={t("Details")} aria-expanded={info} onClick={() => setInfo((value) => !value)}>
                <Info size={18} strokeWidth={1.5} aria-hidden="true" />
              </button>
            ) : null}
          </div>
          {info ? <p className="fin-muted text-[13px] leading-5">{natives}</p> : null}
          {dense && today ? <DueTimeline today={today} dates={rows.map((row) => row.date)} /> : null}
          {groups.map((bucket) => {
            const items = rows.filter((row) => inGroup(row, bucket));
            if (items.length === 0) return null;
            return (
              <div key={bucket}>
                <p className={dense ? "plan-rule" : "stamp mt-4"}>{label[bucket]}</p>
                <div className={dense ? "" : "mt-2 space-y-3"}>
                  {items.map((row) => (
                    <ComingLine
                      key={`${row.id}-${row.date}`}
                      row={row}
                      paidLabel={paidLabel}
                      onPaid={onPaid}
                      onSkip={onSkip}
                      onOpen={onOpen}
                      book={book}
                      display={display}
                      nowMs={nowMs}
                      dense={dense}
                      today={today}
                      color={colors[row.category] || resolveColor(undefined, "category", row.category, "")}
                    />
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

function DueTimeline({ today, dates }: { today: string; dates: string[] }) {
  const due = new Set(dates);
  return (
    <div className="plan-timeline" aria-hidden="true">
      {Array.from({ length: 30 }, (_, index) => {
        const date = addDays(today, index);
        return <span key={date} data-due={due.has(date) ? "true" : "false"} />;
      })}
    </div>
  );
}

function relativeWhen(date: string, today: string, t: (english: string) => string): string {
  if (!today) return date;
  const days = Math.round((Date.parse(`${date}T12:00:00`) - Date.parse(`${today}T12:00:00`)) / 86_400_000);
  if (!Number.isFinite(days)) return date;
  if (days < 0) return t("Overdue");
  if (days === 0) return t("Today");
  if (days === 1) return t("Tomorrow");
  return fill("in {n} d", { n: String(days) });
}

function ComingLine({
  row,
  paidLabel,
  onPaid,
  onSkip,
  onOpen,
  book,
  display,
  nowMs,
  dense = false,
  today = "",
  color = "var(--text-tertiary)",
}: {
  row: ComingRow;
  paidLabel: string;
  onPaid: (id: string, date: string) => void;
  onSkip: (id: string, date: string) => void;
  onOpen?: (id: string) => void;
  book: RateBook | null;
  display: string;
  nowMs: number;
  dense?: boolean;
  today?: string;
  color?: string;
}) {
  const t = useT();
  void nowMs;
  const drag = useRef<{ x: number; y: number } | null>(null);
  const [shift, setShift] = useState(0);
  const converted = row.amountMinor == null ? null : headlineOf([{ amountMinor: row.amountMinor, currency: row.currency }], display, book, nowMs);
  const showConverted = converted && converted.state === "ready" && converted.source !== "identity";
  const repeating = row.progress.mode === "open" || (row.purpose !== "one_off" && row.purpose !== "");
  const open = () => onOpen?.(row.id);
  if (!dense) {
    const progress = row.progress;
    return (
      <div className={cn("border px-3 py-3", row.bucket === "overdue" ? "border-border" : "fin-planned")}>
        <button type="button" className="flex w-full items-baseline justify-between gap-3 text-left" onClick={open}>
          <div className="min-w-0">
            <p className="truncate text-base">{row.name}</p>
            <p className="fin-muted text-sm">{row.date}</p>
          </div>
          <div className="text-right">
            <p className="fin-num text-lg">{row.amountMinor != null ? formatMoney(row.amountMinor, row.currency) : ""}</p>
            {showConverted ? <p className="fin-muted text-[13px]">{formatMoney(converted.amountMinor, converted.currency)}</p> : null}
          </div>
        </button>
        {progress.mode === "fixed" && progress.totalCount != null && progress.remainingCount != null ? (
          <p className="mt-2 text-sm">{fill("{paid} of {total}", { paid: progress.paidCount, total: progress.totalCount })}</p>
        ) : null}
        <div className="mt-3 flex flex-wrap gap-2">
          <button type="button" className="stamp min-h-12 px-3" onClick={() => onPaid(row.id, row.date)}>{paidLabel}</button>
          <button type="button" className="stamp min-h-12 px-3" onClick={() => onSkip(row.id, row.date)}>{t("Skip")}</button>
        </div>
      </div>
    );
  }
  return (
    <div className="plan-bill" data-shift={shift > 24 ? "paid" : shift < -24 ? "skip" : ""}>
      <div
        className="plan-bill-row"
        style={{ transform: `translateX(${shift}px)` }}
        onPointerDown={(event) => {
          drag.current = { x: event.clientX, y: event.clientY };
        }}
        onPointerMove={(event) => {
          if (!drag.current) return;
          const dx = event.clientX - drag.current.x;
          const dy = event.clientY - drag.current.y;
          if (Math.abs(dx) < 8 || Math.abs(dy) > Math.abs(dx)) return;
          setShift(Math.max(-96, Math.min(96, dx)));
        }}
        onPointerUp={(event) => {
          const origin = drag.current;
          drag.current = null;
          if (!origin) return;
          const dx = event.clientX - origin.x;
          const dy = event.clientY - origin.y;
          if (dx > 64) onPaid(row.id, row.date);
          else if (dx < -64) onSkip(row.id, row.date);
          else if (Math.abs(dx) < 8 && Math.abs(dy) < 8) open();
          setShift(0);
        }}
        onPointerCancel={() => {
          drag.current = null;
          setShift(0);
        }}
      >
        <button type="button" className="flex min-h-11 w-full items-center gap-3 text-left" onClick={open}>
          <span className="plan-dot" style={{ background: color }} aria-hidden="true" />
          <span className="min-w-0 flex-1">
            <span className="flex items-center gap-2">
              <span className="truncate text-[16px]">{row.name}</span>
              {repeating ? <Repeat size={14} strokeWidth={1.5} aria-label={t("Recurring bill")} /> : null}
            </span>
            <span className="fin-muted block text-[13px]">{relativeWhen(row.date, today, t)}</span>
          </span>
          <span className="shrink-0 text-right">
            <span className="fin-num block text-[16px]">{row.amountMinor != null ? formatMoney(row.amountMinor, row.currency) : ""}</span>
            {showConverted ? <span className="fin-muted block text-[13px]">{formatMoney(converted.amountMinor, converted.currency)}</span> : null}
          </span>
        </button>
      </div>
    </div>
  );
}

function MonthSection({
  txs,
  accounts,
  display,
  book,
  nowMs,
  bounds,
  entityColors = [],
  onPaint,
}: {
  txs: FinanceTransaction[];
  accounts: FinanceAccount[];
  display: string;
  book: RateBook | null;
  nowMs: number;
  bounds: { start: string; end: string };
  entityColors?: { kind: string; id: string; color: string }[];
  onPaint?: (kind: string, id: string, color: string) => void;
}) {
  const t = useT();
  const kind = (id: string) => accounts.find((account) => account.id === id)?.kind || "";
  const rows: CashTx[] = txs
    .filter((tx) => tx.date >= bounds.start && tx.date <= bounds.end)
    .map((tx) => ({
      currency: tx.currency,
      direction: tx.direction,
      amountMinor: tx.amountMinor,
      date: tx.date,
      accountKind: kind(tx.accountId),
      fromKind: kind(tx.fromAccountId),
    }));
  const lines = monthCash(rows, bounds.start, bounds.end);
  const parts = (pick: (line: (typeof lines)[number]) => number) => lines.flatMap((line) => (pick(line) ? [{ amountMinor: pick(line), currency: line.currency }] : []));
  const income = parts((line) => line.income);
  const expenses = parts((line) => line.expenses);
  const cards = parts((line) => line.cardPayments);
  const debts = parts((line) => line.debtPayments);
  const result = parts((line) => line.result);
  const flow = parts((line) => line.cashFlow);
  if (lines.length === 0) {
    return (
      <section className="fin-panel fin-tight">
        <h2 className="fin-kicker">{t("This month")}</h2>
        <p className="mt-3 text-[15px] leading-5">{t("Nothing recorded this month")}</p>
      </section>
    );
  }
  return (
    <section className="fin-panel fin-tight" data-finance-panel="month">
      <h2 className="fin-kicker">{t("This month")}</h2>
      <p className="fin-muted mt-1 text-[13px] leading-5">{t("Income minus expenses. A charge on a card counts once.")}</p>
      <MoneyRow label={t("Income")} parts={income} display={display} book={book} nowMs={nowMs} color={resolveColor(entityColors, "series", "activity:income", "")} />
      <MoneyRow label={t("Expenses")} parts={expenses} display={display} book={book} nowMs={nowMs} color={resolveColor(entityColors, "series", "activity:expenses", "")} />
      {cards.length > 0 ? <MoneyRow label={t("Card payments")} parts={cards} display={display} book={book} nowMs={nowMs} /> : null}
      {debts.length > 0 ? <MoneyRow label={t("Debt payments")} parts={debts} display={display} book={book} nowMs={nowMs} /> : null}
      <MoneyRow label={t("Result")} parts={result} display={display} book={book} nowMs={nowMs} emphasize color={resolveColor(entityColors, "series", "activity:result", "")} />
      <MoneyRow label={t("Cash flow")} parts={flow} display={display} book={book} nowMs={nowMs} color={resolveColor(entityColors, "series", "activity:cash", "")} />
      {onPaint ? (
        <ChartColorMenu
          series={[
            { id: "activity:income", label: t("Income"), color: resolveColor(entityColors, "series", "activity:income", "") },
            { id: "activity:expenses", label: t("Expenses"), color: resolveColor(entityColors, "series", "activity:expenses", "") },
            { id: "activity:result", label: t("Result"), color: resolveColor(entityColors, "series", "activity:result", "") },
            { id: "activity:cash", label: t("Cash flow"), color: resolveColor(entityColors, "series", "activity:cash", "") },
          ]}
          currency={display}
          onPaint={(id, color) => onPaint("series", id, color)}
        />
      ) : null}
    </section>
  );
}

function MoneyRow({
  label,
  parts,
  display,
  book,
  nowMs,
  emphasize,
  color = "",
}: {
  label: string;
  parts: MoneyPart[];
  display: string;
  book: RateBook | null;
  nowMs: number;
  emphasize?: boolean;
  color?: string;
}) {
  const head = headlineOf(parts, display, book, nowMs);
  const text = head.state === "ready" ? signedMoney(head.amountMinor, head.currency) : head.state === "needs-rate" ? head.missing.join(", ") : "—";
  const tone = head.state === "ready" && emphasize ? (head.amountMinor < 0 ? "fin-out" : head.amountMinor > 0 ? "fin-in" : "") : "";
  return (
    <div className="flex min-h-11 items-center justify-between gap-3 border-t border-[var(--line-soft)]">
      <span className="flex items-center gap-2 text-[15px]">{color ? <span className="fin-swatch" style={{ background: color }} aria-hidden="true" /> : null}{label}</span>
      <span className={cn("fin-num text-[15px]", tone)}>{text}</span>
    </div>
  );
}

function signedMoney(minor: number, currency: string): string {
  const parts = moneyParts(minor, currency);
  return `${parts.unit} ${parts.figure}`.trim();
}

function WorthRows({
  buckets,
  owed,
  display,
  onFocus,
}: {
  buckets: ReturnType<typeof worthBuckets>["buckets"];
  owed: number;
  display: string;
  onFocus: (focus: WorthFocus) => void;
}) {
  const t = useT();
  const money = buckets.find((row) => row.id === "money")?.amountMinor || 0;
  const invested = buckets.find((row) => row.id === "investment")?.amountMinor || 0;
  const credit = buckets.find((row) => row.id === "credit")?.amountMinor || 0;
  const rows: Array<{ focus: WorthFocus; label: string; amount: number }> = [
    { focus: "owned", label: t("You own"), amount: money + invested + credit },
    { focus: "debt", label: t("You owe"), amount: owed },
    { focus: "invested", label: t("Invested"), amount: invested },
    { focus: "cash", label: t("Cash & savings"), amount: money },
  ];
  return (
    <div className="mt-4">
      {rows.map((row) => (
        <button key={row.focus} type="button" className="flex min-h-11 w-full items-center justify-between gap-3 border-t border-[var(--line-soft)] text-left" onClick={() => onFocus(row.focus)}>
          <span className="text-[15px]">{row.label}</span>
          <span className="fin-num text-[15px]">{formatMoney(row.amount, display)}</span>
        </button>
      ))}
    </div>
  );
}

function DebtBlock({ debt, display }: { debt: DebtSelection; display: string }) {
  const t = useT();
  const [open, setOpen] = useState<DebtSliceId | null>(null);
  const labels: Record<DebtSliceId, string> = {
    cards: t("Cards"),
    loans: t("Loans"),
    people: t("Owed to people"),
    bills: t("Overdue bills"),
  };
  if (debt.amountMinor <= 0 && debt.slices.length === 0) return null;
  const slice = debt.slices.find((row) => row.id === open);
  return (
    <div className="mt-4 border-t border-[var(--line-soft)] pt-3">
      <div className="flex min-h-11 items-center justify-between gap-3">
        <span className="text-[15px]">{t("Debt")}</span>
        <span className="fin-num fin-out text-[15px]">{formatMoney(debt.amountMinor, debt.currency || display)}</span>
      </div>
      {debt.natives.length > 0 ? <p className="fin-muted text-[13px] leading-5">{debt.natives.map((row) => formatMoney(row.amountMinor, row.currency)).join(" · ")}</p> : null}
      {debt.missing.length > 0 ? <p className="fin-muted text-[13px] leading-5">{debt.missing.join(", ")}</p> : null}
      {debt.slices.map((row) => (
        <button
          key={row.id}
          type="button"
          className="flex min-h-11 w-full items-center justify-between gap-3 text-left"
          aria-expanded={open === row.id}
          onClick={() => setOpen(open === row.id ? null : row.id)}
        >
          <span className="text-[15px]">{labels[row.id]}</span>
          <span className="fin-num text-[15px]">{formatMoney(row.amountMinor, debt.currency || display)}</span>
        </button>
      ))}
      {slice ? (
        <ul className="mt-1">
          {slice.items.map((item) => (
            <li key={item.id} className="flex min-h-11 items-center justify-between gap-3 border-t border-[var(--line-soft)]">
              <span className="text-[15px]">{item.label}</span>
              <span className="text-right">
                <span className="fin-num block text-[15px]">{formatMoney(item.displayMinor || 0, debt.currency || display)}</span>
                {item.currency.toUpperCase() !== (debt.currency || display).toUpperCase() ? (
                  <span className="fin-muted text-[13px] leading-4">{formatMoney(item.amountMinor, item.currency)}</span>
                ) : null}
              </span>
            </li>
          ))}
        </ul>
      ) : null}
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
