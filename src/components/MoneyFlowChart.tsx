import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { FinanceAccount, FinanceCategory, FinanceFx, FinanceTransaction } from "@/lib/api";
import { ChartCard } from "@/components/ChartKit";
import { ChartPeriodBar, useChartPeriod } from "@/components/ChartPeriodBar";
import { bookFromStatus } from "@/lib/financeMath";
import { buildMoneyFlow, layoutSankey, type FlowNode, type MoneyFlow } from "@/lib/moneyFlow";
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
          subcategory: tx.subcategory,
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
            drawn: t("From savings"),
            total: t("Total"),
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
  const [drill, setDrill] = useState("");
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
      {drill ? (
        <button type="button" className="fin-chip mt-2" onClick={() => setDrill("")}>
          {t("Back")}
        </button>
      ) : null}
      <FlowSvg flow={flow} selected={selected} drill={drill} onSelect={setSelected} onDrill={setDrill} />
      {node ? <FlowDetail node={node} currency={flow.currency} onPaint={() => setPainting(painting === node.id ? "" : node.id)} /> : null}
      {painting && node ? (
        <div className="mt-2 flex flex-wrap gap-1">
          {["var(--brand)", "var(--text-primary)", "var(--text-tertiary)", "var(--line)"].map((swatch) => (
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
          <p className="fin-muted text-[13px] leading-4">{fill("{percent}%", { percent })}</p>
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

function FlowSvg({
  flow,
  selected,
  drill,
  onSelect,
  onDrill,
}: {
  flow: Extract<MoneyFlow, { state: "ready" }>;
  selected: string;
  drill: string;
  onSelect: (id: string) => void;
  onDrill: (id: string) => void;
}) {
  const t = useT();
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
  const drilled = drill ? flow.nodes.find((node) => node.id === drill && node.role === "sink") : undefined;
  const sources = drilled ? [drilled] : flow.nodes.filter((node) => node.role === "source");
  const pool = drilled || flow.nodes.find((node) => node.id === "pool");
  const sinks = drilled
    ? drilled.children.map((child) => flow.nodes.find((node) => node.id === child.id)).filter((node): node is FlowNode => Boolean(node))
    : flow.nodes.filter((node) => node.role === "sink");
  if (!pool) return null;
  if (drilled && sinks.length === 0) {
    return <p className="mt-3 text-[15px] leading-5">{t("No subcategories yet")}</p>;
  }
  const columns = [sources.map((node) => node.id), drilled ? [] : ["pool"], sinks.map((node) => node.id)].filter((column) => column.length > 0);
  const amounts: Record<string, number> = {};
  for (const node of [...sources, ...(drilled ? [] : [pool]), ...sinks]) amounts[node.id] = node.amountMinor;
  const laid = layoutSankey(columns, amounts, flow.links, width);
  const labelOf = (id: string) => flow.nodes.find((node) => node.id === id);
  const percent = (share: number) => (share > 0 && share < 0.001 ? "<0.1%" : `${Math.round(share * 1000) / 10}%`);
  return (
    <div ref={box} className="mt-2 w-full min-w-0">
      <svg viewBox={`0 0 ${width} ${laid.height}`} className="block h-auto w-full overflow-visible" role="img" aria-label={pool.label}>
        {laid.ribbons.map((ribbon) => {
          const node = labelOf(ribbon.source);
          const dim = selected && selected !== ribbon.source && selected !== ribbon.target && selected !== "pool" ? 0.2 : 0.55;
          return <path key={`${ribbon.source}-${ribbon.target}`} d={ribbon.d} fill={node?.color || "var(--text-tertiary)"} fillOpacity={dim} />;
        })}
        {laid.nodes.map((boxNode) => {
          const node = labelOf(boxNode.id);
          if (!node) return null;
          const dim = selected && selected !== node.id && selected !== "pool" ? 0.35 : 1;
          const open = node.role === "sink" && node.children.length > 0 && !drilled;
          return (
            <g key={node.id} opacity={dim} onClick={() => (open ? onDrill(node.id) : onSelect(node.id))}>
              <rect x={0} y={boxNode.y} width={width} height={boxNode.h} fill="transparent" />
              <rect x={boxNode.x} y={boxNode.barY} width={Math.max(boxNode.w, 8)} height={boxNode.barH} fill={node.color} />
              <text x={boxNode.w + 8} y={boxNode.y + 18} fill="currentColor" fontSize={13}>
                {trim(node.label, 22)}
              </text>
              <text x={boxNode.w + 8} y={boxNode.y + 34} fill="currentColor" fontSize={13} opacity={0.72}>
                {`${formatMoney(node.amountMinor, flow.currency)} · ${percent(node.share)}`}
              </text>
            </g>
          );
        })}
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
