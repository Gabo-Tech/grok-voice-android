import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { init, use, type ECharts } from "echarts/core";
import { SankeyChart } from "echarts/charts";
import { CanvasRenderer } from "echarts/renderers";
import { TooltipComponent } from "echarts/components";
import type { FinanceAccount, FinanceCategory, FinanceFx, FinanceTransaction } from "@/lib/api";
import { ChartCard } from "@/components/ChartKit";
import { ChartPeriodBar, useChartPeriod } from "@/components/ChartPeriodBar";
import { bookFromStatus } from "@/lib/financeMath";
import { boundsOf, type ChartPeriod } from "@/lib/chartPeriod";
import { buildMoneyFlow, type MoneyFlow } from "@/lib/moneyFlow";
import { formatMoney } from "@/lib/money";
import { fill, useT } from "@/lib/i18n";

use([SankeyChart, CanvasRenderer, TooltipComponent]);

const COLOR_KEY = "grok-flow-colors";
const OPTION_KEY = "grok-flow-options";

type ColorMode = "category" | "source" | "mono";

function readColors(): Record<string, string> {
  try {
    const raw = localStorage.getItem(COLOR_KEY);
    const parsed = raw ? (JSON.parse(raw) as Record<string, string>) : {};
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function readOptions(): { labels: boolean; values: boolean; mode: ColorMode } {
  try {
    const raw = localStorage.getItem(OPTION_KEY);
    const parsed = raw ? (JSON.parse(raw) as { labels?: boolean; values?: boolean; mode?: ColorMode }) : {};
    const mode = parsed.mode === "source" || parsed.mode === "mono" ? parsed.mode : "category";
    return { labels: parsed.labels !== false, values: parsed.values !== false, mode };
  } catch {
    return { labels: true, values: true, mode: "category" };
  }
}

function token(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value || fallback;
}

export function MoneyFlowPanel({
  transactions,
  accounts,
  categories,
  display,
  fx,
  onPaintCategory,
  onReassign,
  externalPeriod = null,
  detached = false,
  onDetach,
  onFollow,
}: {
  transactions: FinanceTransaction[];
  accounts: FinanceAccount[];
  categories: FinanceCategory[];
  display: string;
  fx?: FinanceFx | null;
  onPaintCategory?: (name: string, color: string) => void;
  onReassign?: () => void;
  externalPeriod?: ChartPeriod | null;
  detached?: boolean;
  onDetach?: (period: ChartPeriod) => void;
  onFollow?: () => void;
}) {
  const t = useT();
  const own = useChartPeriod("flow");
  const period = externalPeriod && !detached ? externalPeriod : own.period;
  const setPeriod = (next: ChartPeriod) => {
    onDetach?.(next);
    own.setPeriod(next);
  };
  const bounds = useMemo(() => boundsOf(period), [period]);
  const [colors, setColors] = useState(readColors);
  const [openOther, setOpenOther] = useState(false);
  const [full, setFull] = useState(() => typeof window !== "undefined" && new URLSearchParams(window.location.search).get("flow") === "full");
  const [drill, setDrill] = useState("");
  const [options, setOptions] = useState(() => {
    if (typeof window === "undefined") return readOptions();
    const flow = new URLSearchParams(window.location.search).get("flow");
    if (flow === "plain") return { labels: false, values: false, mode: "mono" as const };
    if (flow === "source") return { labels: true, values: true, mode: "source" as const };
    if (flow === "category") return { labels: true, values: true, mode: "category" as const };
    return readOptions();
  });
  const [scale, setScale] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
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
    [transactions, kinds, categories, display, book, nowMs, bounds.start, bounds.end, openOther, colors, t],
  );

  useEffect(() => {
    try {
      localStorage.setItem(OPTION_KEY, JSON.stringify(options));
    } catch {
      /* Display options still apply for this visit. */
    }
  }, [options]);

  useEffect(() => {
    const onVoice = (event: Event) => {
      const detail = (event as CustomEvent<{ chart?: string; action?: string; mode?: string }>).detail || {};
      if (detail.chart !== "flow") return;
      if (detail.action === "labels") setOptions((current) => ({ ...current, labels: !current.labels }));
      if (detail.action === "values") setOptions((current) => ({ ...current, values: !current.values }));
      if (detail.action === "fullscreen") setFull(true);
      if (detail.action === "color" && (detail.mode === "category" || detail.mode === "source" || detail.mode === "mono")) {
        setOptions((current) => ({ ...current, mode: detail.mode as ColorMode }));
      }
    };
    window.addEventListener("grok-finance-voice", onVoice);
    return () => window.removeEventListener("grok-finance-voice", onVoice);
  }, []);

  const ready = flow.state === "ready" ? flow : null;
  const view = useMemo(() => (ready ? drilled(ready, drill) : null), [ready, drill]);
  const uncategorized = ready?.nodes.some((node) => node.kind === "expense" && node.label === t("Uncategorized")) || false;

  const chart = (
    <FlowChart
      flow={view}
      currency={display}
      labelsOn={options.labels}
      valuesOn={options.values}
      mode={options.mode}
      tall={full}
      onDrill={(id) => setDrill(id)}
    />
  );

  const controls = (
    <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1" data-swipe-ignore="">
      <button type="button" className="fin-chip" aria-pressed={options.labels} onClick={() => setOptions((current) => ({ ...current, labels: !current.labels }))}>
        {t("Labels")}
      </button>
      <button type="button" className="fin-chip" aria-pressed={options.values} onClick={() => setOptions((current) => ({ ...current, values: !current.values }))}>
        {t("Values")}
      </button>
      <button type="button" className="fin-chip" aria-pressed={options.mode === "category"} onClick={() => setOptions((current) => ({ ...current, mode: "category" }))}>
        {t("By category")}
      </button>
      <button type="button" className="fin-chip" aria-pressed={options.mode === "source"} onClick={() => setOptions((current) => ({ ...current, mode: "source" }))}>
        {t("By source")}
      </button>
      <button type="button" className="fin-chip" aria-pressed={options.mode === "mono"} onClick={() => setOptions((current) => ({ ...current, mode: "mono" }))}>
        {t("One color")}
      </button>
      <button type="button" className="fin-chip" onClick={() => setOpenOther((value) => !value)}>
        {openOther ? t("Group small flows") : t("Show small flows")}
      </button>
      <button type="button" className="fin-chip" onClick={() => setFull(true)}>
        {t("Full screen")}
      </button>
      {detached && onFollow ? (
        <button type="button" className="fin-chip" onClick={onFollow}>
          {t("Use the dashboard period")}
        </button>
      ) : null}
    </div>
  );

  return (
    <ChartCard title={t("Where my money went")}>
      <ChartPeriodBar period={period} onChange={setPeriod} />
      {controls}
      {drill ? (
        <button type="button" className="fin-chip mt-2" onClick={() => setDrill("")}>
          {t("Back")}
        </button>
      ) : null}
      <div className="mt-3 overflow-x-auto" data-swipe-ignore="">
        {flow.state === "empty" ? <p className="text-[15px] leading-5">{t("No income or expenses in this period.")}</p> : chart}
      </div>
      {uncategorized && onReassign ? (
        <button type="button" className="fin-chip mt-2" onClick={onReassign}>
          {t("Set a category")}
        </button>
      ) : null}
      <CategoryColors
        categories={categories}
        colors={colors}
        onChange={(name, color) => {
          setColors((current) => {
            const next = { ...current, [name]: color };
            try {
              localStorage.setItem(COLOR_KEY, JSON.stringify(next));
            } catch {
              /* The color still applies until the page reloads. */
            }
            return next;
          });
          onPaintCategory?.(name, color);
        }}
      />
      {full
        ? createPortal(
            <FullFlow
              title={t("Where my money went")}
              scale={scale}
              pan={pan}
              onScale={setScale}
              onPan={setPan}
              onClose={() => {
                setFull(false);
                setScale(1);
                setPan({ x: 0, y: 0 });
              }}
            >
              {chart}
              {controls}
            </FullFlow>,
            document.body,
          )
        : null}
    </ChartCard>
  );
}

function drilled(flow: MoneyFlow, id: string): MoneyFlow {
  if (flow.state !== "ready" || !id) return flow;
  const node = flow.nodes.find((row) => row.id === id);
  if (!node || (node.role !== "sink" && node.role !== "child")) return flow;
  const keep = new Set<string>([id, ...flow.links.filter((link) => link.source === id).map((link) => link.target)]);
  if (node.role === "child") {
    const parent = flow.links.find((link) => link.target === id)?.source;
    if (parent) keep.add(parent);
  }
  const nodes = flow.nodes.filter((row) => keep.has(row.id));
  const links = flow.links.filter((link) => keep.has(link.source) && keep.has(link.target));
  return { ...flow, nodes, links };
}

function FlowChart({
  flow,
  currency,
  labelsOn,
  valuesOn,
  mode,
  tall,
  onDrill,
}: {
  flow: MoneyFlow | null;
  currency: string;
  labelsOn: boolean;
  valuesOn: boolean;
  mode: ColorMode;
  tall: boolean;
  onDrill: (id: string) => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const chart = useRef<ECharts | null>(null);
  const drillRef = useRef(onDrill);
  drillRef.current = onDrill;
  const safeToReal = useRef(new Map<string, string>());

  useEffect(() => {
    const node = host.current;
    if (!node) return;
    const visible = Math.min(node.parentElement?.clientWidth || window.innerWidth, window.innerWidth - (tall ? 16 : 36));
    const width = Math.max(280, visible);
    node.style.width = `${width}px`;
    node.style.maxWidth = "100%";
    const instance = init(node, undefined, { renderer: "canvas", width, height: tall ? Math.max(280, window.innerHeight - 120) : 320 });
    chart.current = instance;
    const onClick = (params: { dataType?: string; data?: { name?: string; id?: string } | null }) => {
      const data = params.data;
      const key = data && typeof data === "object" ? data.name || "" : "";
      const name = safeToReal.current.get(key) || "";
      if (params.dataType === "node" && name) drillRef.current(name);
    };
    instance.on("click", onClick as never);
    const resize = () => instance.resize();
    window.addEventListener("resize", resize);
    return () => {
      window.removeEventListener("resize", resize);
      instance.off("click", onClick);
      instance.dispose();
      chart.current = null;
    };
  }, []);

  useEffect(() => {
    const instance = chart.current;
    if (!instance || !flow || flow.state !== "ready") return;
    const ink = token("--text-primary", "#3a342c");
    const brand = token("--brand", "#7d6b58");
    const muted = token("--text-tertiary", "#8a8175");
    const total = flow.nodes.find((node) => node.id === "pool")?.amountMinor || flow.incomeMinor || 1;
    const width = Math.max(280, host.current?.clientWidth || window.innerWidth - 32);
    instance.resize({ width, height: tall ? Math.max(280, window.innerHeight - 120) : 320 });
    const safe = new Map(flow.nodes.map((node, index) => [node.id, `n${index}`]));
    const bySafe = new Map(flow.nodes.map((node, index) => [`n${index}`, node]));
    safeToReal.current = new Map(flow.nodes.map((node, index) => [`n${index}`, node.id]));
    const byId = bySafe;
    try {
    instance.setOption(
      {
        backgroundColor: token("--surface-0", "#17181b"),
        tooltip: {
          trigger: "item",
          backgroundColor: token("--surface-1", "#f4f1ea"),
          borderColor: token("--line", "#d9d2c5"),
          textStyle: { color: ink, fontSize: 13 },
          formatter: (raw: unknown) => {
            const params = raw as { dataType?: string; data?: { name?: string; source?: string; target?: string; value?: number } };
            const value = Number(params.data?.value || 0);
            const share = total > 0 ? Math.round((value / total) * 1000) / 10 : 0;
            if (params.dataType === "edge") {
              const from = byId.get(params.data?.source || "")?.label || "";
              const to = byId.get(params.data?.target || "")?.label || "";
              return `${from} → ${to}<br/>${formatMoney(value, currency)} · ${share}%`;
            }
            const name = byId.get(params.data?.name || "")?.label || "";
            return `${name}<br/>${formatMoney(value, currency)} · ${share}%`;
          },
        },
        series: [
          {
            type: "sankey",
            orient: "horizontal",
            nodeAlign: "left",
            draggable: false,
            nodeWidth: 14,
            nodeGap: 14,
            left: 8,
            right: labelsOn ? 88 : 12,
            top: 8,
            bottom: 8,
            emphasis: { focus: "adjacency" },
            lineStyle: { color: mode === "source" ? "source" : mode === "mono" ? brand : "gradient", curveness: 0.5, opacity: 0.45 },
            label: {
              show: labelsOn,
              color: ink,
              fontSize: 13,
              formatter: (raw: unknown) => {
                const params = raw as { name?: string };
                const node = byId.get(params.name || "");
                if (!node) return "";
                if (!valuesOn) return node.label;
                const share = total > 0 ? Math.round((node.amountMinor / total) * 1000) / 10 : 0;
                return `${node.label}\n${formatMoney(node.amountMinor, currency)} · ${share}%`;
              },
            },
            data: flow.nodes.map((node) => ({
              name: safe.get(node.id),
              depth: node.role === "source" ? 0 : node.role === "pool" ? 1 : node.role === "child" ? 3 : 2,
              value: Math.max(1, node.amountMinor),
              itemStyle: { color: mode === "mono" ? brand : node.color.startsWith("var(") ? (node.role === "pool" ? brand : muted) : node.color },
            })),
            links: flow.links.map((link) => ({ source: safe.get(link.source), target: safe.get(link.target), value: Math.max(1, link.amountMinor) })),
          },
        ],
      },
      true,
    );
    } catch (error) {
      if (host.current) host.current.dataset.chartError = error instanceof Error ? error.message : "chart";
    }
  }, [flow, currency, labelsOn, valuesOn, mode]);

  useEffect(() => {
    chart.current?.resize();
  }, [tall, labelsOn]);

  return <div ref={host} style={{ width: "100%", maxWidth: "100%", height: tall ? "100%" : 360 }} />;
}

function CategoryColors({
  categories,
  colors,
  onChange,
}: {
  categories: FinanceCategory[];
  colors: Record<string, string>;
  onChange: (name: string, color: string) => void;
}) {
  const t = useT();
  const rows = categories.filter((row) => !row.parent);
  if (!rows.length) return null;
  return (
    <div className="mt-3" data-swipe-ignore="">
      <p className="fin-muted text-[13px] leading-4">{t("Category colors")}</p>
      <div className="mt-1 flex flex-wrap gap-2">
        {rows.map((row) => (
          <label key={row.name} className="flex min-h-11 items-center gap-2 text-[13px]">
            <input
              type="color"
              aria-label={fill("{name} color", { name: row.name })}
              value={/^#[0-9A-Fa-f]{6}$/.test(colors[row.name] || row.color) ? colors[row.name] || row.color : "#7d6b58"}
              onChange={(event) => onChange(row.name, event.target.value)}
              className="h-11 w-11 border-0 bg-transparent p-0"
            />
            {row.name}
          </label>
        ))}
      </div>
    </div>
  );
}

function FullFlow({
  title,
  scale,
  pan,
  onScale,
  onPan,
  onClose,
  children,
}: {
  title: string;
  scale: number;
  pan: { x: number; y: number };
  onScale: (scale: number) => void;
  onPan: (pan: { x: number; y: number }) => void;
  onClose: () => void;
  children: ReactNode;
}) {
  const t = useT();
  const pointers = useRef(new Map<number, { x: number; y: number }>());
  const last = useRef({ distance: 0, x: 0, y: 0, tap: 0 });

  useEffect(() => {
    const orientation = screen.orientation as ScreenOrientation & { lock?: (next: string) => Promise<void> };
    orientation.lock?.("landscape").catch(() => undefined);
    return () => {
      orientation.unlock?.();
    };
  }, []);

  const zoom = (next: number) => onScale(Math.min(4, Math.max(1, next)));

  return (
    <div className="fixed inset-0 z-50 flex flex-col bg-[var(--surface-0)]" data-swipe-ignore="" role="dialog" aria-label={title}>
      <div className="flex min-h-11 items-center justify-between gap-3 px-3">
        <button type="button" className="fin-chip" onClick={onClose}>
          {t("Close")}
        </button>
        <div className="flex gap-2">
          <button type="button" className="fin-primary h-11 w-11" onClick={() => zoom(scale / 1.25)} aria-label={t("Zoom out")}>
            −
          </button>
          <button type="button" className="fin-primary h-11 w-11" onClick={() => zoom(scale * 1.25)} aria-label={t("Zoom in")}>
            +
          </button>
          <button
            type="button"
            className="fin-chip"
            onClick={() => {
              onScale(1);
              onPan({ x: 0, y: 0 });
            }}
          >
            {t("Reset")}
          </button>
        </div>
      </div>
      <div
        className="min-h-0 flex-1 overflow-hidden"
        onPointerDown={(event) => {
          pointers.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
          last.current.x = event.clientX;
          last.current.y = event.clientY;
          if (pointers.current.size === 2) {
            const [a, b] = [...pointers.current.values()];
            last.current.distance = Math.hypot(a.x - b.x, a.y - b.y);
          }
        }}
        onPointerMove={(event) => {
          if (!pointers.current.has(event.pointerId)) return;
          pointers.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
          if (pointers.current.size === 2 && last.current.distance > 0) {
            const [a, b] = [...pointers.current.values()];
            const distance = Math.hypot(a.x - b.x, a.y - b.y);
            zoom(scale * (distance / last.current.distance));
            last.current.distance = distance;
            return;
          }
          if (pointers.current.size === 1) {
            onPan({ x: pan.x + event.clientX - last.current.x, y: pan.y + event.clientY - last.current.y });
            last.current.x = event.clientX;
            last.current.y = event.clientY;
          }
        }}
        onPointerUp={(event) => {
          pointers.current.delete(event.pointerId);
          const now = Date.now();
          if (now - last.current.tap < 280) {
            onScale(1);
            onPan({ x: 0, y: 0 });
          }
          last.current.tap = now;
        }}
        onPointerCancel={() => pointers.current.clear()}
      >
        <div className="h-full w-full" style={{ transform: `translate(${pan.x}px, ${pan.y}px) scale(${scale})`, transformOrigin: "center center" }}>
          {children}
        </div>
      </div>
    </div>
  );
}
