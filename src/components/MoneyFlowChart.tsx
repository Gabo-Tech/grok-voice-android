import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { FinanceAccount, FinanceCategory, FinanceFx, FinanceTransaction } from "@/lib/api";
import { ChartCard } from "@/components/ChartKit";
import { ChartPeriodBar, useChartPeriod } from "@/components/ChartPeriodBar";
import { ALLOCATION_PRESETS } from "@/lib/financeLedger";
import { bookFromStatus } from "@/lib/financeMath";
import { buildMoneyFlow, linkedIds, type FlowNode, type MoneyFlow } from "@/lib/moneyFlow";
import { PALETTE } from "@/lib/financeView";
import { formatMoney } from "@/lib/money";
import { fill, useT } from "@/lib/i18n";

const COLOR_KEY = "grok-flow-colors";

function readColors(): Record<string, string> {
  try {
    const raw = localStorage.getItem(COLOR_KEY);
    const parsed = raw ? (JSON.parse(raw) as Record<string, string>) : {};
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

export function MoneyFlowPanel({
  transactions,
  accounts,
  categories,
  display,
  fx,
  onPaintCategory,
}: {
  transactions: FinanceTransaction[];
  accounts: FinanceAccount[];
  categories: FinanceCategory[];
  display: string;
  fx?: FinanceFx | null;
  onPaintCategory?: (name: string, color: string) => void;
}) {
  const t = useT();
  const { period, setPeriod, bounds } = useChartPeriod("flow");
  const [colors, setColors] = useState(readColors);
  const [openOther, setOpenOther] = useState(false);
  const [full, setFull] = useState(false);
  const book = useMemo(() => bookFromStatus(fx), [fx]);
  const nowMs = useMemo(() => Date.now(), [transactions, fx, display]);
  const kinds = useMemo(() => new Map(accounts.map((account) => [account.id, account.kind])), [accounts]);
  const flow = useMemo(
    () =>
      buildMoneyFlow(
        transactions.map((tx) => ({
          amountMinor: tx.amountMinor,
          currency: tx.currency,
          direction: tx.direction,
          category: tx.category,
          date: tx.date,
          accountKind: kinds.get(tx.accountId) || "",
          fromKind: kinds.get(tx.fromAccountId) || "",
        })),
        {
          categories,
          display,
          book,
          nowMs,
          start: bounds.start,
          end: bounds.end,
          expandedOther: openOther,
          nodeColors: { ...Object.fromEntries(categories.map((row) => [row.name, row.color])), ...colors },
          labels: {
            income: t("Income"),
            drawn: t("Drawn from savings"),
            surplus: t("Surplus"),
            other: t("Other"),
            uncategorized: t("Uncategorized"),
            debt: t("Debt payments"),
            savings: t("Savings"),
            investments: t("Investing"),
          },
        },
      ),
    [transactions, categories, display, book, nowMs, bounds.start, bounds.end, openOther, colors, kinds, t],
  );
  const paint = (id: string, color: string) => {
    const node = flow.state === "ready" ? flow.nodes.find((row) => row.id === id) : undefined;
    const category = categories.find((row) => row.name === node?.label);
    if (category && onPaintCategory) onPaintCategory(category.name, color);
    const next = { ...colors, [id]: color, ...(node ? { [node.label]: color } : {}) };
    setColors(next);
    try {
      localStorage.setItem(COLOR_KEY, JSON.stringify(next));
    } catch {
      /* The color still applies for this visit. */
    }
  };

  return (
    <ChartCard
      title={t("Where my money went")}
      action={
        <button type="button" className="fin-chip" onClick={() => setFull(true)}>
          {t("Full screen")}
        </button>
      }
    >
      <p className="fin-muted text-[13px] leading-4">{fill("In {code}. Tap a flow to see the exact amount.", { code: display || "—" })}</p>
      <ChartPeriodBar period={period} onChange={setPeriod} />
      <FlowBody flow={flow} openOther={openOther} onToggleOther={() => setOpenOther((current) => !current)} onPaint={paint} />
      {full
        ? createPortal(
            <FullFlow
              title={t("Where my money went")}
              flow={flow}
              openOther={openOther}
              onToggleOther={() => setOpenOther((current) => !current)}
              onPaint={paint}
              onClose={() => setFull(false)}
            />,
            document.body,
          )
        : null}
    </ChartCard>
  );
}

function FlowBody({
  flow,
  openOther,
  onToggleOther,
  onPaint,
  zoom = 1,
}: {
  flow: MoneyFlow;
  openOther: boolean;
  onToggleOther: () => void;
  onPaint: (id: string, color: string) => void;
  zoom?: number;
}) {
  const t = useT();
  const [selected, setSelected] = useState("");
  const [painting, setPainting] = useState("");
  if (flow.state === "needs-rate") {
    return <p className="mt-3 text-[15px] leading-5">{fill("Combined total needs a rate for {codes}.", { codes: flow.missing.join(", ") })}</p>;
  }
  if (flow.state === "empty") {
    return <p className="mt-3 text-[15px] leading-5">{t("No income or expenses in this period.")}</p>;
  }
  const node = flow.nodes.find((row) => row.id === selected) || null;
  const other = flow.nodes.find((row) => row.id === "out:other");
  return (
    <div style={zoom === 1 ? undefined : { transform: `scale(${zoom})`, transformOrigin: "top center" }}>
      <FlowSvg flow={flow} selected={selected} onSelect={setSelected} />
      {node ? <FlowDetail node={node} currency={flow.currency} onPaint={() => setPainting(painting === node.id ? "" : node.id)} /> : null}
      {painting && node ? (
        <div className="mt-2 flex flex-wrap gap-1">
          {[...PALETTE, ...ALLOCATION_PRESETS.okabe.filter((color) => color !== "#000000"), ...ALLOCATION_PRESETS.dusk].map((swatch) => (
            <button key={swatch} type="button" className="fin-icon" aria-label={swatch} onClick={() => onPaint(node.id, swatch)}>
              <span className="fin-swatch" style={{ background: swatch }} />
            </button>
          ))}
        </div>
      ) : null}
      {other && other.members.length > 0 ? (
        <button type="button" className="fin-chip mt-2" aria-pressed={openOther} onClick={onToggleOther}>
          {openOther ? t("Group small flows") : t("Show small flows")}
        </button>
      ) : null}
    </div>
  );
}

function FlowDetail({ node, currency, onPaint }: { node: FlowNode; currency: string; onPaint: () => void }) {
  const t = useT();
  const percent = node.share > 0 && node.share < 0.001 ? "<0.1" : String(Math.round(node.share * 1000) / 10);
  return (
    <div className="mt-3 border border-[var(--fin-line)] px-3 py-2">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="text-[15px] leading-5">{node.label}</p>
          <p className="fin-muted text-[13px] leading-4">{fill("{percent}% of income", { percent })}</p>
          {node.originals.length > 0 ? <p className="fin-muted text-[13px] leading-4">{node.originals.map((row) => formatMoney(row.amountMinor, row.currency)).join(" · ")}</p> : null}
        </div>
        <div className="text-right">
          <p className="fin-num text-[15px] leading-5">{formatMoney(node.amountMinor, currency)}</p>
          <button type="button" className="fin-icon" aria-label={t("Color")} onClick={onPaint}>
            <span className="fin-swatch" style={{ background: node.color }} />
          </button>
        </div>
      </div>
      {node.members.length > 0 ? (
        <ul className="mt-2 space-y-1">
          {node.members.map((member) => (
            <li key={member.id} className="flex justify-between gap-2 text-[13px] leading-4">
              <span className="min-w-0 truncate">{member.label}</span>
              <span className="fin-num shrink-0">{formatMoney(member.amountMinor, currency)}</span>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

function FlowSvg({ flow, selected, onSelect }: { flow: Extract<MoneyFlow, { state: "ready" }>; selected: string; onSelect: (id: string) => void }) {
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(320);
  useEffect(() => {
    const node = box.current;
    if (!node) return;
    const apply = () => setWidth(Math.max(260, Math.floor(node.clientWidth)));
    apply();
    const observer = new ResizeObserver(apply);
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  const sources = flow.nodes.filter((node) => node.role === "source");
  const sinks = flow.nodes.filter((node) => node.role === "sink");
  const pool = flow.nodes.find((node) => node.id === "pool");
  if (!pool) return null;
  const row = 40;
  const ribbon = 22;
  const poolH = 36;
  const height = sources.length * row + ribbon + poolH + ribbon + Math.max(sinks.length, 1) * row + 2;
  const colL = Math.round(width * 0.62);
  const colR = width - 6;
  const span = Math.max(24, colR - colL);
  const max = Math.max(...flow.nodes.map((node) => node.amountMinor), 1);
  const bar = (amount: number) => Math.max(8, (amount / max) * span);
  const keep = selected ? linkedIds(flow, selected) : null;
  const dim = (id: string) => (keep && !keep.has(id) ? 0.2 : 1);
  const sourceY = (index: number) => index * row + 4;
  const poolY = sources.length * row + ribbon;
  const sinkY = (index: number) => poolY + poolH + ribbon + index * row;
  const percent = (share: number) => (share > 0 && share < 0.001 ? "<0.1%" : `${Math.round(share * 1000) / 10}%`);
  return (
    <div ref={box} className="mt-2 w-full min-w-0">
      <svg viewBox={`0 0 ${width} ${height}`} className="block h-auto w-full" role="img" aria-label={pool.label}>
        {flow.links
          .filter((link) => link.target === "pool")
          .map((link) => {
            const index = sources.findIndex((node) => node.id === link.source);
            const node = sources[index];
            if (!node || index < 0) return null;
            const x = colL + bar(node.amountMinor) / 2;
            const y = sourceY(index) + 28;
            const path = `M ${x} ${y} C ${x} ${y + ribbon / 2}, ${colL + span / 2} ${poolY}, ${colL + span / 2} ${poolY + 4}`;
            return <path key={link.source} d={path} fill="none" stroke={node.color} strokeWidth={Math.max(3, bar(link.amountMinor) * 0.45)} strokeOpacity={0.45 * dim(link.source)} strokeLinecap="butt" />;
          })}
        {flow.links
          .filter((link) => link.source === "pool")
          .map((link) => {
            const index = sinks.findIndex((node) => node.id === link.target);
            const node = sinks[index];
            if (!node || index < 0) return null;
            const x = colL + bar(node.amountMinor) / 2;
            const y = sinkY(index) + 8;
            const fromY = poolY + poolH - 6;
            const path = `M ${colL + span / 2} ${fromY} C ${colL + span / 2} ${fromY + ribbon / 2}, ${x} ${y - 8}, ${x} ${y}`;
            return <path key={link.target} d={path} fill="none" stroke={node.color} strokeWidth={Math.max(3, bar(link.amountMinor) * 0.45)} strokeOpacity={0.45 * dim(link.target)} strokeLinecap="butt" />;
          })}
        {sources.map((node, index) => (
          <g key={node.id} opacity={dim(node.id)} onClick={() => onSelect(node.id)}>
            <text x={4} y={sourceY(index) + 16} fill="currentColor" fontSize={13}>
              {trim(node.label, 16)}
            </text>
            <text x={4} y={sourceY(index) + 32} fill="currentColor" fontSize={13} opacity={0.72}>
              {`${formatMoney(node.amountMinor, flow.currency)} · ${percent(node.share)}`}
            </text>
            <rect x={colL} y={sourceY(index) + 18} width={bar(node.amountMinor)} height={10} rx={2} fill={node.color} />
          </g>
        ))}
        <g opacity={dim("pool")} onClick={() => onSelect("pool")}>
          <text x={4} y={poolY + 14} fill="currentColor" fontSize={13}>
            {trim(`${pool.label} · ${formatMoney(pool.amountMinor, flow.currency)}`, 22)}
          </text>
          <rect x={colL} y={poolY + 16} width={span} height={14} rx={2} fill={pool.color} />
        </g>
        {sinks.map((node, index) => (
          <g key={node.id} opacity={dim(node.id)} onClick={() => onSelect(node.id)}>
            <rect x={colL} y={sinkY(index) + 8} width={bar(node.amountMinor)} height={10} rx={2} fill={node.color} />
            <text x={4} y={sinkY(index) + 16} fill="currentColor" fontSize={13}>
              {trim(node.label, 18)}
            </text>
            <text x={4} y={sinkY(index) + 32} fill="currentColor" fontSize={13} opacity={0.72}>
              {`${formatMoney(node.amountMinor, flow.currency)} · ${percent(node.share)}`}
            </text>
          </g>
        ))}
      </svg>
    </div>
  );
}

function FullFlow({
  title,
  flow,
  openOther,
  onToggleOther,
  onPaint,
  onClose,
}: {
  title: string;
  flow: MoneyFlow;
  openOther: boolean;
  onToggleOther: () => void;
  onPaint: (id: string, color: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [zoom, setZoom] = useState(1);
  const pinch = useRef(0);
  return (
    <div className="finance fixed inset-0 z-50 overflow-auto bg-[var(--fin-canvas)] px-3 pt-4 pb-8" role="dialog" aria-label={title}>
      <div className="mb-3 flex items-center justify-between gap-2">
        <h2 className="text-[15px] leading-5">{title}</h2>
        <button type="button" className="fin-chip" onClick={onClose}>
          {t("Close")}
        </button>
      </div>
      <div
        onTouchStart={(event) => {
          if (event.touches.length === 2) pinch.current = distance(event.touches[0], event.touches[1]);
        }}
        onTouchMove={(event) => {
          if (event.touches.length !== 2 || !pinch.current) return;
          const next = distance(event.touches[0], event.touches[1]);
          setZoom((current) => Math.min(2.4, Math.max(1, current * (next / pinch.current))));
          pinch.current = next;
        }}
      >
        <FlowBody flow={flow} openOther={openOther} onToggleOther={onToggleOther} onPaint={onPaint} zoom={zoom} />
      </div>
    </div>
  );
}

function distance(a: { clientX: number; clientY: number }, b: { clientX: number; clientY: number }): number {
  return Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
}

function trim(value: string, max: number): string {
  return value.length <= max ? value : `${value.slice(0, max - 1)}…`;
}
