import { useMemo, useRef, useState, type ReactNode } from "react";
import type { FinanceAccount, FinanceFx, FinanceTransaction } from "@/lib/api";
import { RatePanel } from "@/components/CombinedTotal";
import { bookFromStatus, headlineOf, type Headline, type MoneyPart, type RateBook } from "@/lib/financeMath";
import { formatMoney } from "@/lib/money";
import { displayColor, isDebt, netByCurrency, sixMonths } from "@/lib/financeView";
import { fill, useT } from "@/lib/i18n";

const SAGE = "var(--sage)";
const ROSE = "var(--rose)";
const GOLD = "var(--gold)";

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
  planned = [],
  display = "",
  fx,
  busy = false,
  onRefresh,
  onSaveRate,
  onPick,
}: {
  accounts: FinanceAccount[];
  transactions: FinanceTransaction[];
  planned?: { currency: string; income: number; expense: number }[];
  display?: string;
  fx?: FinanceFx | null;
  busy?: boolean;
  onRefresh?: () => void;
  onSaveRate?: (base: string, quote: string, rate: string) => void;
  onPick: (pick: ChartPick) => void;
}) {
  const t = useT();
  const scroller = useRef<HTMLDivElement>(null);
  const [page, setPage] = useState(0);
  const book = useMemo(() => bookFromStatus(fx), [fx]);
  const nowMs = useMemo(() => Date.now(), [fx, transactions, accounts, planned, display]);
  let askRate = true;
  const cards = buildCards();
  if (cards.length === 0) return null;
  const go = (index: number) => {
    const node = scroller.current;
    if (!node) return;
    const child = node.children[index] as HTMLElement | undefined;
    child?.scrollIntoView({ behavior: "smooth", inline: "start", block: "nearest" });
    setPage(index);
  };
  return (
    <section className="min-w-0 max-w-full">
      <p className="fin-kicker mb-3">{t("Charts")}</p>
      <div
        ref={scroller}
        className="flex max-w-full snap-x snap-mandatory gap-3 overflow-x-auto pb-2"
        onScroll={(event) => {
          const node = event.currentTarget;
          const width = node.clientWidth || 1;
          setPage(Math.round(node.scrollLeft / width));
        }}
      >
        {cards.map((card) => (
          <article key={card.key} className="fin-panel w-full max-w-full shrink-0 snap-start">
            <h3 className="fin-kicker">{card.title}</h3>
            <div className="mt-4 min-w-0">{card.body}</div>
          </article>
        ))}
      </div>
      <div className="mt-3 flex justify-center gap-1">
        {cards.map((card, index) => (
          <button key={card.key} type="button" className="flex h-11 w-11 items-center justify-center" aria-label={`${index + 1}`} onClick={() => go(index)}>
            <span className={index === page ? "fin-dot on" : "fin-dot"} />
          </button>
        ))}
      </div>
      <p className="fin-muted mt-1 text-center text-sm">{t("Tap a chart to open it in Activity.")}</p>
    </section>
  );

  function claimAsk() {
    const allowed = askRate;
    askRate = false;
    return allowed;
  }

  function blocked(missing: string[], key: string, title: string) {
    return {
      key,
      title,
      body: (
        <NeedRate
          codes={missing}
          display={display}
          busy={busy}
          ask={claimAsk()}
          onRefresh={onRefresh}
          onSave={onSaveRate}
        />
      ),
    };
  }

  function buildCards() {
    const cards: Array<{ key: string; title: string; body: ReactNode }> = [];
    const code = display.trim().toUpperCase();
    const nets = netByCurrency(accounts);
    if (nets.length > 0) {
      const money = headlineOf(nets.map((net) => ({ amountMinor: net.money, currency: net.currency })), code, book, nowMs);
      const debt = headlineOf(nets.map((net) => ({ amountMinor: net.debt, currency: net.currency })), code, book, nowMs);
      const invest = headlineOf(nets.map((net) => ({ amountMinor: net.investment, currency: net.currency })), code, book, nowMs);
      const net = headlineOf(nets.map((row) => ({ amountMinor: row.net, currency: row.currency })), code, book, nowMs);
      const missing = [...new Set([...money.missing, ...debt.missing, ...invest.missing, ...net.missing])];
      if (missing.length > 0) {
        cards.push(blocked(missing, "ring", t("Money, debts, and investments")));
      } else {
        const slices = [
          { label: t("Money"), value: Math.abs(money.amountMinor), color: SAGE, pick: { group: "money" as const, currency: code } },
          { label: t("Debts"), value: Math.abs(debt.amountMinor), color: ROSE, pick: { group: "debt" as const, currency: code } },
          { label: t("Investments"), value: Math.abs(invest.amountMinor), color: GOLD, pick: { group: "investment" as const, currency: code } },
        ].filter((slice) => slice.value > 0);
        if (slices.length > 0 && net.state === "ready") {
          cards.push({
            key: "ring",
            title: t("Money, debts, and investments"),
            body: <Donut slices={slices} center={formatMoney(net.amountMinor, net.currency)} onPick={onPick} />,
          });
        }
      }
    }
    const bars = accounts
      .map((account, index) => ({
        label: account.name,
        value: Math.abs(account.shownBalanceMinor ?? 0),
        color: displayColor(account.color, index),
        caption: account.shownBalanceMinor == null ? t("Not set") : formatMoney(account.shownBalanceMinor, account.currency),
        pick: { accountId: account.id, currency: account.currency },
      }))
      .filter((bar) => bar.label);
    if (bars.length > 0) cards.push({ key: "bars", title: t("Every account"), body: <Bars rows={bars} onPick={onPick} /> });
    if (planned.some((row) => row.income || row.expense)) {
      const income = headlineOf(planned.map((row) => ({ amountMinor: row.income, currency: row.currency })), code, book, nowMs);
      const expense = headlineOf(planned.map((row) => ({ amountMinor: row.expense, currency: row.currency })), code, book, nowMs);
      const missing = [...new Set([...income.missing, ...expense.missing])];
      cards.push(
        missing.length
          ? blocked(missing, "planned", t("Planned, next 12 months"))
          : {
              key: "planned",
              title: t("Planned, next 12 months"),
              body: (
                <div>
                  <p className="stamp mb-3">{t("Not actual")}</p>
                  <p className="fin-num mb-3 text-3xl leading-none break-words">
                    {formatMoney((income.state === "ready" ? income.amountMinor : 0) - (expense.state === "ready" ? expense.amountMinor : 0), code)}
                  </p>
                  <Grouped rows={[{ currency: code, income: readyAmount(income), expense: readyAmount(expense), payment: 0 }]} onPick={onPick} />
                </div>
              ),
            },
      );
    }
    const month = monthParts(transactions);
    if (month.income.length || month.expense.length || month.payment.length) {
      const income = headlineOf(month.income, code, book, nowMs);
      const expense = headlineOf(month.expense, code, book, nowMs);
      const payment = headlineOf(month.payment, code, book, nowMs);
      const left = headlineOf(month.left, code, book, nowMs);
      const missing = [...new Set([...income.missing, ...expense.missing, ...payment.missing, ...left.missing])];
      cards.push(
        missing.length
          ? blocked(missing, "month", t("Income and expenses"))
          : {
              key: "month",
              title: t("Income and expenses"),
              body: (
                <div>
                  {left.state === "ready" ? <p className="fin-num mb-3 text-3xl leading-none break-words">{formatMoney(left.amountMinor, left.currency)}</p> : null}
                  <Grouped rows={[{ currency: code, income: readyAmount(income), expense: readyAmount(expense), payment: readyAmount(payment) }]} onPick={onPick} />
                </div>
              ),
            },
      );
    }
    const categories = categoryParts(transactions);
    if (categories.length > 0) {
      const missing = new Set<string>();
      const merged = new Map<string, number>();
      for (const row of categories) {
        const head = headlineOf([{ amountMinor: row.amountMinor, currency: row.currency }], code, book, nowMs);
        if (head.state === "needs-rate") head.missing.forEach((item) => missing.add(item));
        else if (head.state === "ready") merged.set(row.name, (merged.get(row.name) || 0) + head.amountMinor);
      }
      cards.push(
        missing.size
          ? blocked([...missing], "category", t("By category"))
          : {
              key: "category",
              title: t("By category"),
              body: (
                <Bars
                  rows={[...merged.entries()].map(([name, value], index) => ({
                    label: name === "Expense" ? t("Expense") : name,
                    value,
                    color: displayColor("", index),
                    caption: formatMoney(value, code),
                    pick: { category: name, currency: code },
                  }))}
                  onPick={onPick}
                />
              ),
            },
      );
    }
    const trend = trendReady(transactions, code, book, nowMs);
    if (trend.kind === "missing") cards.push(blocked(trend.missing, "trend", t("Six months")));
    else if (trend.kind === "points") cards.push({ key: "trend", title: t("Six months"), body: <Trend points={trend.points} onPick={onPick} /> });
    const debts = accounts
      .filter((account) => isDebt(account.kind) && account.shownBalanceMinor != null)
      .map((account, index) => ({
        label: account.name,
        value: Math.abs(account.shownBalanceMinor ?? 0),
        color: displayColor(account.color, index),
        caption: formatMoney(account.shownBalanceMinor, account.currency),
        pick: { accountId: account.id, currency: account.currency, group: "debt" as const },
      }));
    if (debts.length > 0) cards.push({ key: "debts", title: t("Debts owed"), body: <Bars rows={debts} onPick={onPick} /> });
    const holdings = accounts.filter((account) => account.kind === "investment" && account.shownBalanceMinor != null);
    if (holdings.length > 0) {
      const head = headlineOf(
        holdings.map((account) => ({ amountMinor: account.shownBalanceMinor || 0, currency: account.currency })),
        code,
        book,
        nowMs,
      );
      if (head.state === "needs-rate") cards.push(blocked(head.missing, "holdings", t("Holdings")));
      else {
        cards.push({
          key: "holdings",
          title: t("Holdings"),
          body: (
            <Donut
              slices={holdings.map((account, index) => ({
                label: account.name,
                value: Math.abs(headlineOf([{ amountMinor: account.shownBalanceMinor || 0, currency: account.currency }], code, book, nowMs).amountMinor),
                color: displayColor(account.color, index),
                pick: { accountId: account.id, group: "investment" as const },
              }))}
              center={head.state === "ready" ? formatMoney(head.amountMinor, head.currency) : ""}
              onPick={onPick}
            />
          ),
        });
      }
    }
    return cards;
  }
}

function readyAmount(head: Headline): number {
  return head.state === "ready" ? head.amountMinor : 0;
}

function monthParts(txs: FinanceTransaction[]) {
  const now = new Date();
  const key = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
  const income: MoneyPart[] = [];
  const expense: MoneyPart[] = [];
  const payment: MoneyPart[] = [];
  const left = new Map<string, number>();
  for (const tx of txs) {
    if (!tx.date.startsWith(key) || !tx.currency) continue;
    const part = { amountMinor: tx.amountMinor, currency: tx.currency };
    if (tx.direction === "income") income.push(part);
    if (tx.direction === "expense") expense.push(part);
    if (tx.direction === "payment") payment.push(part);
    const sign = tx.direction === "income" ? 1 : tx.direction === "expense" || tx.direction === "payment" ? -1 : 0;
    if (sign) left.set(tx.currency, (left.get(tx.currency) || 0) + sign * tx.amountMinor);
  }
  return { income, expense, payment, left: [...left.entries()].map(([currency, amountMinor]) => ({ currency, amountMinor })) };
}

function categoryParts(txs: FinanceTransaction[]) {
  const now = new Date();
  const key = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
  const by = new Map<string, { name: string; currency: string; amountMinor: number }>();
  for (const tx of txs) {
    if (tx.direction !== "expense" || !tx.date.startsWith(key) || !tx.currency) continue;
    const name = tx.category || "Expense";
    const id = `${name}:${tx.currency}`;
    const row = by.get(id) ?? { name, currency: tx.currency, amountMinor: 0 };
    row.amountMinor += tx.amountMinor;
    by.set(id, row);
  }
  return [...by.values()];
}

function trendReady(txs: FinanceTransaction[], display: string, book: RateBook | null, nowMs: number):
  | { kind: "empty" }
  | { kind: "missing"; missing: string[] }
  | { kind: "points"; points: Array<{ key: string; label: string; income: number; expense: number; net: number }> } {
  const missing = new Set<string>();
  const points = sixMonths().map((key) => {
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
    const incomeMinor = readyAmount(income);
    const expenseMinor = readyAmount(expense);
    return { key, label: key.slice(5), income: incomeMinor, expense: expenseMinor, net: incomeMinor - expenseMinor };
  });
  if (missing.size > 0) return { kind: "missing", missing: [...missing].sort() };
  if (!points.some((point) => point.income || point.expense)) return { kind: "empty" };
  return { kind: "points", points };
}

function NeedRate({
  codes,
  display,
  busy,
  ask,
  onRefresh,
  onSave,
}: {
  codes: string[];
  display: string;
  busy: boolean;
  ask: boolean;
  onRefresh?: () => void;
  onSave?: (base: string, quote: string, rate: string) => void;
}) {
  useT();
  if (ask && onRefresh && onSave) {
    return <RatePanel bare codes={codes} display={display} busy={busy} onRefresh={onRefresh} onSave={onSave} />;
  }
  return <p className="text-base leading-6 break-words">{fill("Combined total needs a rate for {codes}.", { codes: codes.join(", ") })}</p>;
}

function Donut({
  slices,
  center,
  onPick,
}: {
  slices: Array<{ label: string; value: number; color: string; pick: ChartPick }>;
  center: string;
  onPick: (pick: ChartPick) => void;
}) {
  const total = slices.reduce((sum, slice) => sum + slice.value, 0) || 1;
  let angle = -90;
  const paths = slices.map((slice) => {
    const sweep = (slice.value / total) * 360;
    const path = arc(120, 120, 78, 48, angle, angle + Math.max(sweep, 0.8));
    angle += sweep;
    return { ...slice, path };
  });
  return (
    <div className="min-w-0">
      <svg viewBox="0 0 240 240" className="mx-auto h-56 w-56 max-w-full">
        {paths.map((slice) => (
          <path key={slice.label} d={slice.path} fill={slice.color} onClick={() => onPick(slice.pick)}>
            <title>{slice.label}</title>
          </path>
        ))}
        <text x="120" y="124" textAnchor="middle" fill="currentColor" fontSize={center.length > 14 ? 11 : 16} className="fin-num">
          {center}
        </text>
      </svg>
      <div className="mt-2 space-y-1">
        {slices.map((slice) => (
          <button key={slice.label} type="button" className="flex min-h-12 w-full min-w-0 items-center gap-3 text-left" onClick={() => onPick(slice.pick)}>
            <span className="fin-swatch shrink-0" style={{ background: slice.color }} />
            <span className="min-w-0 flex-1 truncate text-base">{slice.label}</span>
          </button>
        ))}
      </div>
    </div>
  );
}

function Bars({
  rows,
  onPick,
}: {
  rows: Array<{ label: string; value: number; color: string; caption: string; pick: ChartPick }>;
  onPick: (pick: ChartPick) => void;
}) {
  const max = Math.max(...rows.map((row) => row.value), 1);
  return (
    <div className="space-y-3">
      {rows.map((row) => (
        <button key={`${row.label}-${row.caption}`} type="button" className="block min-h-12 w-full min-w-0 text-left" onClick={() => onPick(row.pick)}>
          <span className="flex min-w-0 items-baseline justify-between gap-3">
            <span className="min-w-0 truncate text-base">{row.label}</span>
            <span className="fin-num shrink-0 text-base">{row.caption}</span>
          </span>
          <span className="fin-track mt-1 block">
            <span className="block h-full" style={{ width: `${Math.max(4, (row.value / max) * 100)}%`, background: row.color }} />
          </span>
        </button>
      ))}
    </div>
  );
}

function Grouped({
  rows,
  onPick,
}: {
  rows: Array<{ currency: string; income: number; expense: number; payment: number }>;
  onPick: (pick: ChartPick) => void;
}) {
  const t = useT();
  return (
    <div className="space-y-5">
      {rows.map((row) => {
        const max = Math.max(row.income, row.expense, row.payment, 1);
        const bars = [
          { label: t("Income"), value: row.income, color: SAGE, direction: "income" },
          { label: t("Expenses"), value: row.expense, color: ROSE, direction: "expense" },
          { label: t("Payments"), value: row.payment, color: GOLD, direction: "payment" },
        ];
        return (
          <div key={row.currency} className="min-w-0">
            <p className="fin-kicker mb-2">{row.currency}</p>
            {bars.map((bar) => (
              <button key={bar.label} type="button" className="mb-2 block min-h-12 w-full text-left" onClick={() => onPick({ currency: row.currency })}>
                <span className="flex justify-between gap-3 text-sm">
                  <span>{bar.label}</span>
                  <span className="fin-num break-words">{formatMoney(bar.value, row.currency)}</span>
                </span>
                <span className="fin-track mt-1 block">
                  <span className="block h-full" style={{ width: `${Math.max(bar.value ? 4 : 0, (bar.value / max) * 100)}%`, background: bar.color }} />
                </span>
              </button>
            ))}
          </div>
        );
      })}
    </div>
  );
}

function Trend({
  points,
  onPick,
}: {
  points: Array<{ key: string; label: string; income: number; expense: number; net: number }>;
  onPick: (pick: ChartPick) => void;
}) {
  const max = Math.max(...points.flatMap((point) => [point.income, point.expense, Math.abs(point.net)]), 1);
  const x = (index: number) => 16 + (index * 200) / Math.max(points.length - 1, 1);
  const y = (value: number) => 110 - (Math.abs(value) / max) * 90;
  const path = (key: "income" | "expense" | "net") => points.map((point, index) => `${index === 0 ? "M" : "L"} ${x(index)} ${y(point[key])}`).join(" ");
  return (
    <div className="min-w-0">
      <svg viewBox="0 0 240 130" className="h-44 w-full max-w-full">
        <path d={path("income")} fill="none" stroke={SAGE} strokeWidth="2.5" />
        <path d={path("expense")} fill="none" stroke={ROSE} strokeWidth="2.5" />
        <path d={path("net")} fill="none" stroke={GOLD} strokeWidth="2.5" />
      </svg>
      <div className="mt-2 flex justify-between gap-1">
        {points.map((point) => (
          <button key={point.key} type="button" className="min-h-11 min-w-0 flex-1 text-xs" onClick={() => onPick({ month: point.key })}>
            {point.label}
          </button>
        ))}
      </div>
    </div>
  );
}

function arc(cx: number, cy: number, outer: number, inner: number, start: number, end: number): string {
  const sweep = Math.min(end - start, 359.8);
  const large = sweep > 180 ? 1 : 0;
  const a = polar(cx, cy, outer, start);
  const b = polar(cx, cy, outer, start + sweep);
  const c = polar(cx, cy, inner, start + sweep);
  const d = polar(cx, cy, inner, start);
  return `M ${a.x} ${a.y} A ${outer} ${outer} 0 ${large} 1 ${b.x} ${b.y} L ${c.x} ${c.y} A ${inner} ${inner} 0 ${large} 0 ${d.x} ${d.y} Z`;
}

function polar(cx: number, cy: number, radius: number, degrees: number) {
  const radians = (degrees * Math.PI) / 180;
  return { x: cx + radius * Math.cos(radians), y: cy + radius * Math.sin(radians) };
}
