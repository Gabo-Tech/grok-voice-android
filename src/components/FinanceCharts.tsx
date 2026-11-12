import { useRef, useState, type ReactNode } from "react";
import type { FinanceAccount, FinanceTransaction } from "@/lib/api";
import { formatMoney } from "@/lib/money";
import { displayColor, isDebt, netByCurrency, sixMonths } from "@/lib/financeView";
import { useT } from "@/lib/i18n";

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
  onPick,
}: {
  accounts: FinanceAccount[];
  transactions: FinanceTransaction[];
  planned?: { currency: string; income: number; expense: number }[];
  onPick: (pick: ChartPick) => void;
}) {
  const t = useT();
  const scroller = useRef<HTMLDivElement>(null);
  const [page, setPage] = useState(0);
  const cards = buildCards(accounts, transactions, planned, t);
  if (cards.length === 0) return null;
  const go = (index: number) => {
    const node = scroller.current;
    if (!node) return;
    const child = node.children[index] as HTMLElement | undefined;
    child?.scrollIntoView({ behavior: "smooth", inline: "start", block: "nearest" });
    setPage(index);
  };
  return (
    <section>
      <p className="fin-kicker mb-3">{t("Charts")}</p>
      <div
        ref={scroller}
        className="flex snap-x snap-mandatory gap-3 overflow-x-auto pb-2"
        onScroll={(event) => {
          const node = event.currentTarget;
          const width = node.clientWidth || 1;
          setPage(Math.round(node.scrollLeft / width));
        }}
      >
        {cards.map((card) => (
          <article key={card.key} className="fin-panel w-full shrink-0 snap-start">
            <h3 className="fin-kicker">{card.title}</h3>
            <div className="mt-4">{card.body}</div>
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

  function buildCards(rows: FinanceAccount[], txs: FinanceTransaction[], plannedRows: { currency: string; income: number; expense: number }[], translate: (text: string) => string) {
    const cards: Array<{ key: string; title: string; body: ReactNode }> = [];
    const nets = netByCurrency(rows);
    nets.forEach((net) => {
      const slices = [
        { label: translate("Money"), value: Math.abs(net.money), color: "#7D9A86", pick: { currency: net.currency, group: "money" as const } },
        { label: translate("Debts"), value: Math.abs(net.debt), color: "#C48984", pick: { currency: net.currency, group: "debt" as const } },
        { label: translate("Investments"), value: Math.abs(net.investment), color: "#C4B5A0", pick: { currency: net.currency, group: "investment" as const } },
      ].filter((slice) => slice.value > 0);
      if (slices.length === 0) return;
      cards.push({
        key: `ring-${net.currency}`,
        title: `${translate("Money, debts, and investments")} · ${net.currency}`,
        body: <Donut slices={slices} center={formatMoney(net.net, net.currency)} onPick={onPick} />,
      });
    });
    const bars = rows
      .map((account, index) => ({
        label: account.name,
        value: Math.abs(account.shownBalanceMinor ?? 0),
        color: displayColor(account.color, index),
        caption: account.shownBalanceMinor == null ? translate("Not set") : formatMoney(account.shownBalanceMinor, account.currency),
        pick: { accountId: account.id, currency: account.currency },
      }))
      .filter((bar) => bar.label);
    if (bars.length > 0) {
      cards.push({ key: "bars", title: translate("Every account"), body: <Bars rows={bars} onPick={onPick} /> });
    }
    if (plannedRows.some((row) => row.income || row.expense)) {
      cards.push({
        key: "planned",
        title: translate("Planned, next 12 months"),
        body: (
          <div>
            <p className="stamp mb-3">{translate("Not actual")}</p>
            <Grouped rows={plannedRows.map((row) => ({ ...row, payment: 0 }))} onPick={onPick} />
          </div>
        ),
      });
    }
    const month = monthBars(txs);
    if (month.length > 0) {
      cards.push({ key: "month", title: translate("Income and expenses"), body: <Grouped rows={month} onPick={onPick} /> });
    }
    const categories = categoryBars(txs);
    if (categories.length > 0) {
      cards.push({ key: "category", title: translate("By category"), body: <Bars rows={categories} onPick={onPick} /> });
    }
    const trend = trendPoints(txs);
    if (trend.some((point) => point.income || point.expense)) {
      cards.push({ key: "trend", title: translate("Six months"), body: <Trend points={trend} onPick={onPick} /> });
    }
    const debts = rows
      .filter((account) => isDebt(account.kind) && account.shownBalanceMinor != null)
      .map((account, index) => ({
        label: account.name,
        value: Math.abs(account.shownBalanceMinor ?? 0),
        color: displayColor(account.color, index),
        caption: formatMoney(account.shownBalanceMinor, account.currency),
        pick: { accountId: account.id, currency: account.currency, group: "debt" as const },
      }));
    if (debts.length > 0) cards.push({ key: "debts", title: translate("Debts owed"), body: <Bars rows={debts} onPick={onPick} /> });
    const holdings = rows
      .filter((account) => account.kind === "investment" && account.shownBalanceMinor != null)
      .map((account, index) => ({
        label: account.name,
        value: Math.abs(account.shownBalanceMinor ?? 0),
        color: displayColor(account.color, index),
        caption: formatMoney(account.shownBalanceMinor, account.currency),
        pick: { accountId: account.id, group: "investment" as const },
      }));
    if (holdings.length > 0) {
      cards.push({
        key: "holdings",
        title: translate("Holdings"),
        body: <Donut slices={holdings.map((row) => ({ ...row, pick: row.pick }))} center="" onPick={onPick} />,
      });
    }
    return cards;
  }
}

function monthBars(txs: FinanceTransaction[]) {
  const now = new Date();
  const key = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
  const by = new Map<string, { income: number; expense: number; payment: number }>();
  for (const tx of txs) {
    if (!tx.date.startsWith(key)) continue;
    const row = by.get(tx.currency) ?? { income: 0, expense: 0, payment: 0 };
    if (tx.direction === "income") row.income += tx.amountMinor;
    if (tx.direction === "expense") row.expense += tx.amountMinor;
    if (tx.direction === "payment") row.payment += tx.amountMinor;
    by.set(tx.currency, row);
  }
  return [...by.entries()].map(([currency, row]) => ({ currency, ...row }));
}

function categoryBars(txs: FinanceTransaction[]) {
  const now = new Date();
  const key = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
  const by = new Map<string, { value: number; currency: string }>();
  txs
    .filter((tx) => tx.direction === "expense" && tx.date.startsWith(key))
    .forEach((tx, index) => {
      const name = tx.category || "Expense";
      const row = by.get(`${name}:${tx.currency}`) ?? { value: 0, currency: tx.currency };
      row.value += tx.amountMinor;
      by.set(`${name}:${tx.currency}`, row);
      void index;
    });
  return [...by.entries()].map(([id, row], index) => ({
    label: id.split(":")[0],
    value: row.value,
    color: displayColor("", index),
    caption: formatMoney(row.value, row.currency),
    pick: { category: id.split(":")[0], currency: row.currency },
  }));
}

function trendPoints(txs: FinanceTransaction[]) {
  return sixMonths().map((key) => {
    let income = 0;
    let expense = 0;
    for (const tx of txs) {
      if (!tx.date.startsWith(key)) continue;
      if (tx.direction === "income") income += tx.amountMinor;
      if (tx.direction === "expense") expense += tx.amountMinor;
    }
    return { key, label: key.slice(5), income, expense, net: income - expense };
  });
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
    <div>
      <svg viewBox="0 0 240 240" className="mx-auto h-56 w-56">
        {paths.map((slice) => (
          <path key={slice.label} d={slice.path} fill={slice.color} onClick={() => onPick(slice.pick)}>
            <title>{slice.label}</title>
          </path>
        ))}
        <text x="120" y="124" textAnchor="middle" fill="currentColor" fontSize="16" className="fin-num">
          {center}
        </text>
      </svg>
      <div className="mt-2 space-y-1">
        {slices.map((slice) => (
          <button key={slice.label} type="button" className="flex min-h-12 w-full items-center gap-3 text-left" onClick={() => onPick(slice.pick)}>
            <span className="fin-swatch" style={{ background: slice.color }} />
            <span className="flex-1 text-base">{slice.label}</span>
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
        <button key={`${row.label}-${row.caption}`} type="button" className="block min-h-12 w-full text-left" onClick={() => onPick(row.pick)}>
          <span className="flex items-baseline justify-between gap-3">
            <span className="truncate text-base">{row.label}</span>
            <span className="fin-num text-base">{row.caption}</span>
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
          { label: t("Income"), value: row.income, color: "#7D9A86", direction: "income" },
          { label: t("Expenses"), value: row.expense, color: "#C48984", direction: "expense" },
          { label: t("Payments"), value: row.payment, color: "#C4B5A0", direction: "payment" },
        ];
        return (
          <div key={row.currency}>
            <p className="fin-kicker mb-2">{row.currency}</p>
            {bars.map((bar) => (
              <button key={bar.label} type="button" className="mb-2 block min-h-12 w-full text-left" onClick={() => onPick({ currency: row.currency })}>
                <span className="flex justify-between text-sm">
                  <span>{bar.label}</span>
                  <span className="fin-num">{formatMoney(bar.value, row.currency)}</span>
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
    <div>
      <svg viewBox="0 0 240 130" className="h-44 w-full">
        <path d={path("income")} fill="none" stroke="#7D9A86" strokeWidth="2.5" />
        <path d={path("expense")} fill="none" stroke="#C48984" strokeWidth="2.5" />
        <path d={path("net")} fill="none" stroke="#C4B5A0" strokeWidth="2.5" />
      </svg>
      <div className="mt-2 flex justify-between gap-1">
        {points.map((point) => (
          <button key={point.key} type="button" className="min-h-11 flex-1 text-xs" onClick={() => onPick({ month: point.key })}>
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
