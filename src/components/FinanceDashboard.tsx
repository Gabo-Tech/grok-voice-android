import { useMemo } from "react";
import type { FinanceState } from "@/hooks/useFinance";
import type { FinanceAccount, FinanceRule, FinanceTransaction } from "@/lib/api";
import {
  addDays,
  addMonths,
  bookFromStatus,
  cashFlow,
  combinedMinor,
  comingRows,
  monthBounds,
  netWorth,
  netWorthChange,
  plannedHits,
  projectWindow,
  type ComingRow,
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
  const nextPlan = useMemo(() => projectWindow(flow, planned, today, addDays(today, 30), today, true), [flow, planned, today]);
  const nowMs = finance.snapshot.fx?.fetchedMs ? Date.now() : Date.now();
  const hero = useMemo(
    () => combinedMinor(worth.map((row) => ({ amountMinor: row.net, currency: row.currency })), display, book, nowMs),
    [worth, display, book, nowMs],
  );
  const monthHero = useMemo(
    () => combinedMinor(flow.map((row) => ({ amountMinor: row.left, currency: row.currency })), display, book, nowMs),
    [flow, display, book, nowMs],
  );
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

  return (
    <div className="fin-stack space-y-6">
      <section className="fin-panel">
        <h2 className="fin-kicker">{t("Net worth")}</h2>
        <p className="fin-muted mt-2 text-sm leading-5">{t("Right now. What you own minus what you owe.")}</p>
        {worth.length === 0 ? (
          <p className="mt-4 text-base">{t("No balances stated yet")}</p>
        ) : (
          <>
            {hero ? (
              <div className="mt-4">
                <p className="fin-num text-5xl leading-none font-medium">{formatMoney(hero.amountMinor, hero.currency)}</p>
                {hero.source === "identity" ? (
                  <p className="fin-muted mt-2 text-sm">{hero.currency}</p>
                ) : (
                  <p className="fin-muted mt-2 text-sm leading-5">
                    {fill("Together in {code}", { code: hero.currency })}
                    {hero.asOf ? ` · ${fill("As of {date}", { date: hero.asOf })}` : ""}
                    {hero.stale ? ` · ${t("Rates are stale")}` : ""}
                  </p>
                )}
              </div>
            ) : (
              <p className="mt-4 text-base leading-6">{t("No combined total. A rate is missing.")}</p>
            )}
            <div className="mt-4 grid grid-cols-2 gap-2">
              <FocusChip label={t("You own")} onClick={() => onFocus("owned")} />
              <FocusChip label={t("You owe")} onClick={() => onFocus("debt")} />
              <FocusChip label={t("Invested")} onClick={() => onFocus("invested")} />
              <FocusChip label={t("Cash & savings")} onClick={() => onFocus("cash")} />
            </div>
            {worth.map((row) => (
              <div key={row.currency} className="mt-4 border-t border-border pt-3">
                <p className="stamp">{row.currency}</p>
                <p className="fin-num mt-2 text-3xl leading-none font-medium">{formatMoney(row.net, row.currency)}</p>
                <div className="mt-3 grid grid-cols-3 gap-2 text-sm">
                  <Mini label={t("Cash & savings")} value={formatMoney(row.cashSavings, row.currency)} />
                  <Mini label={t("Invested")} value={formatMoney(row.invested, row.currency)} />
                  <Mini label={t("You owe")} value={formatMoney(row.owed, row.currency)} tone="out" />
                </div>
              </div>
            ))}
          </>
        )}
      </section>

      <section className="fin-panel">
        <h2 className="fin-kicker">{t("This month")}</h2>
        <p className="fin-muted mt-2 text-sm leading-5">{t("Income received minus expenses paid.")}</p>
        {flow.length === 0 ? (
          <p className="mt-4 text-base">{t("Nothing recorded this month")}</p>
        ) : (
          flow.map((row) => {
            const before = previous.find((item) => item.currency === row.currency);
            return (
              <div key={row.currency} className="mt-4 border-t border-border pt-3">
                <p className="stamp">{row.currency}</p>
                <p className={cn("fin-num mt-2 text-4xl leading-none font-medium", row.left < 0 && "fin-out")}>
                  {formatMoney(row.left, row.currency)}
                </p>
                <p className="fin-muted mt-1 text-sm">{row.left < 0 ? t("Short") : t("Left")}</p>
                <div className="mt-3 grid grid-cols-2 gap-2">
                  <Mini label={t("Incoming")} value={formatMoney(row.incoming, row.currency)} tone="in" />
                  <Mini label={t("Outgoing")} value={formatMoney(row.outgoing, row.currency)} tone="out" />
                </div>
                {before ? (
                  <p className="fin-muted mt-2 text-sm">{fill("Last month left {amount}", { amount: formatMoney(before.left, before.currency) })}</p>
                ) : null}
              </div>
            );
          })
        )}
        {monthHero && monthHero.source !== "identity" ? (
          <p className="fin-muted mt-4 text-sm leading-5">
            {fill("Together in {code}", { code: monthHero.currency })}: {formatMoney(monthHero.amountMinor, monthHero.currency)}
            {monthHero.asOf ? ` · ${fill("As of {date}", { date: monthHero.asOf })}` : ""}
            {monthHero.stale ? ` · ${t("Rates are stale")}` : ""}
          </p>
        ) : null}
        {moves.length > 0 ? (
          <div className="mt-4 border-t border-border pt-3">
            <p className="text-sm font-medium">{t("Change this month")}</p>
            <p className="fin-muted mt-1 text-sm leading-5">{t("From what you recorded. A holding stays at the price you entered.")}</p>
            {moves.map((row) => (
              <p key={row.currency} className={cn("fin-num mt-2 text-lg", row.change < 0 && "fin-out", row.change > 0 && "fin-in")}>
                {formatMoney(row.change, row.currency)}
              </p>
            ))}
          </div>
        ) : null}
      </section>

      <Coming
        title={t("Coming expenses")}
        body={t("Bills, rent, subscriptions, and fixed runs.")}
        rows={expenses}
        emptyTitle={t("Program a bill")}
        emptyBody={t("Rent, a subscription, or a course with an end.")}
        onEmpty={onProgram}
        onPaid={(id, date) => finance.markPaid(id, date)}
        onSkip={(id, date) => finance.scheduleAction(id, "skip", date)}
        paidLabel={t("Mark paid")}
        book={book}
        display={display}
        nowMs={nowMs}
      />
      <Coming
        title={t("Coming income")}
        body={t("Paydays, planned income, and payouts.")}
        rows={income}
        emptyTitle={t("Program income")}
        emptyBody={t("A payday, money you will get, or a payout.")}
        onEmpty={onIncome}
        onPaid={(id, date) => finance.markPaid(id, date)}
        onSkip={(id, date) => finance.scheduleAction(id, "skip", date)}
        paidLabel={t("Mark received")}
        book={book}
        display={display}
        nowMs={nowMs}
      />

      <section className="fin-panel fin-planned">
        <div className="flex items-center justify-between gap-3">
          <h2 className="fin-kicker">{t("If these land")}</h2>
          <span className="stamp">{t("Not actual")}</span>
        </div>
        <p className="fin-muted mt-2 text-sm leading-5">{t("This month if the plan happens. Not a lifetime total.")}</p>
        {monthPlan.length === 0 ? (
          <p className="mt-4 text-base">{t("Nothing planned in this window")}</p>
        ) : (
          monthPlan.map((row) => (
            <div key={row.currency} className="mt-4 border-t border-dashed border-border pt-3">
              <p className="stamp">{row.currency}</p>
              <p className={cn("fin-num mt-2 text-3xl leading-none", row.projected < 0 && "fin-out")}>{formatMoney(row.projected, row.currency)}</p>
              <p className="fin-muted mt-2 text-sm leading-5">
                {t("Left")} {formatMoney(row.actualLeft, row.currency)} · {t("Incoming")} {formatMoney(row.comingIn, row.currency)} · {t("Outgoing")}{" "}
                {formatMoney(row.comingOut, row.currency)}
              </p>
            </div>
          ))
        )}
        {nextPlan.length > 0 ? (
          <div className="mt-4">
            <p className="text-sm font-medium">{t("Next 30 days")}</p>
            {nextPlan.map((row) => (
              <p key={row.currency} className="fin-muted mt-1 text-sm">
                {formatMoney(row.projected, row.currency)} · {t("Includes anything still unpaid.")}
              </p>
            ))}
          </div>
        ) : null}
      </section>
    </div>
  );
}

function Coming({
  title,
  body,
  rows,
  emptyTitle,
  emptyBody,
  onEmpty,
  onPaid,
  onSkip,
  paidLabel,
  book,
  display,
  nowMs,
}: {
  title: string;
  body: string;
  rows: ComingRow[];
  emptyTitle: string;
  emptyBody: string;
  onEmpty: () => void;
  onPaid: (id: string, date: string) => void;
  onSkip: (id: string, date: string) => void;
  paidLabel: string;
  book: RateBook | null;
  display: string;
  nowMs: number;
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
        groups.map((bucket) => {
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
        })
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
  const converted = row.amountMinor == null ? null : combinedMinor([{ amountMinor: row.amountMinor, currency: row.currency }], display, book, nowMs);
  const showConverted = converted && converted.source !== "identity";
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
      <div className="mt-2 flex gap-2">
        <button type="button" className="stamp min-h-12" onClick={() => onPaid(row.id, row.date)}>
          {paidLabel}
        </button>
        <button type="button" className="stamp min-h-12" onClick={() => onSkip(row.id, row.date)}>
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
    <div>
      <p className="text-xs tracking-wide text-muted-foreground uppercase">{label}</p>
      <p className={cn("fin-num mt-1 text-base font-medium", tone === "in" && "fin-in", tone === "out" && "fin-out")}>{value}</p>
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
