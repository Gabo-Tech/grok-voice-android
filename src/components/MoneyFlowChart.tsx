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
import { buildMoneyFlow, sankeyFrame, sankeyValue, type MoneyFlow } from "@/lib/moneyFlow";
import { formatMoney } from "@/lib/money";
import { pushOverlay } from "@/lib/history";
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
  const names = useMemo(() => new Map(accounts.map((account) => [account.id, account.name])), [accounts]);
  const flow = useMemo(
    () =>
      buildMoneyFlow(
        transactions.map((tx) => ({
          amountMinor: tx.amountMinor,
          currency: tx.currency,
          direction: tx.direction,
          category: tx.category,
          subcategory: tx.subcategory,
          notes: tx.notes,
          date: tx.date,
          accountKind: kinds.get(tx.accountId) || "",
          fromKind: kinds.get(tx.fromAccountId) || "",
          accountName: names.get(tx.accountId) || "",
        })),
        {
          categories,
          display,
          book,
          nowMs,
          start: bounds.start,
          end: bounds.end,
          expandedOther: openOther,
          group: options.mode === "source" ? "source" : "category",
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
            card: t("Card"),
            cash: t("Cash"),
            bank: t("Bank"),
            spending: t("Spending"),
          },
        },
      ),
    [transactions, kinds, names, categories, display, book, nowMs, bounds.start, bounds.end, openOther, colors, options.mode, t],
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
  const uncategorized = (ready?.uncategorizedMinor || 0) > 0;

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
        {flow.state === "empty" ? <p className="text-[15px] leading-5">{t("No income or expenses in this period.")}</p> : full ? null : chart}
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
    const height = tall ? Math.max(280, window.innerHeight - 148) : 320;
    instance.resize({ width, height });
    const depths = flow.nodes.map((node) => (node.role === "source" ? 0 : node.role === "pool" ? 1 : node.role === "child" ? 3 : 2));
    const counts = [0, 1, 2, 3].map((depth) => depths.filter((row) => row === depth).length).filter((count) => count > 0);
    const frame = sankeyFrame(height, counts);
    const rightmost = Math.max(...depths, 0);
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
            nodeWidth: frame.nodeWidth,
            nodeGap: frame.nodeGap,
            left: labelsOn ? 12 : 8,
            right: 16,
            top: frame.top,
            bottom: frame.bottom,
            emphasis: { focus: "adjacency" },
            lineStyle: { color: mode === "source" ? "source" : mode === "mono" ? brand : "gradient", curveness: 0.55, opacity: 0.55 },
            label: {
              show: labelsOn,
              color: ink,
              fontSize: 13,
              overflow: "truncate",
              width: Math.max(72, Math.round(width * 0.34)),
              lineHeight: 16,
              formatter: (raw: unknown) => {
                const params = raw as { name?: string };
                const node = byId.get(params.name || "");
                if (!node) return "";
                if (!valuesOn) return node.label;
                const share = total > 0 ? Math.round((node.amountMinor / total) * 1000) / 10 : 0;
                return `${node.label}\n${formatMoney(node.amountMinor, currency)} · ${share}%`;
              },
            },
            data: flow.nodes.map((node) => {
              const depth = node.role === "source" ? 0 : node.role === "pool" ? 1 : node.role === "child" ? 3 : 2;
              const atRight = depth === rightmost;
              return {
                name: safe.get(node.id),
                depth,
                value: Math.max(1, sankeyValue(node.amountMinor, total, frame.floorShare)),
                label: {
                  position: atRight ? "left" : "right",
                  align: atRight ? "right" : "left",
                  distance: 6,
                },
                itemStyle: { color: mode === "mono" ? brand : node.color.startsWith("var(") ? (node.role === "pool" ? brand : muted) : node.color, borderWidth: 0 },
              };
            }),
            links: flow.links.map((link) => ({ source: safe.get(link.source), target: safe.get(link.target), value: Math.max(1, sankeyValue(link.amountMinor, total, frame.floorShare)) })),
          },
        ],
      },
      true,
    );
    } catch (error) {
      if (host.current) host.current.dataset.chartError = error instanceof Error ? error.message : "chart";
    }
  }, [flow, currency, labelsOn, valuesOn, mode, tall]);

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
  const stage = useRef<HTMLDivElement>(null);
  const view = useRef({ scale, pan, onScale, onPan });
  view.current = { scale, pan, onScale, onPan };
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  useEffect(() => pushOverlay(() => closeRef.current()), []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        closeRef.current();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    const node = stage.current;
    if (!node) return;
    const points = new Map<number, { x: number; y: number }>();
    let distance = 0;
    let originX = 0;
    let originY = 0;
    let moved = false;
    let lastTap = 0;

    const zoomBy = (factor: number) => {
      const next = Math.min(4, Math.max(1, view.current.scale * factor));
      view.current.onScale(next);
      if (next === 1) view.current.onPan({ x: 0, y: 0 });
    };
    const place = (id: number, x: number, y: number) => points.set(id, { x, y });
    const pinch = () => {
      if (points.size !== 2) return 0;
      const [a, b] = [...points.values()];
      return Math.hypot(a.x - b.x, a.y - b.y);
    };

    const onPointerDown = (event: PointerEvent) => {
      if (event.pointerType === "touch") return;
      place(event.pointerId, event.clientX, event.clientY);
      originX = event.clientX;
      originY = event.clientY;
      moved = false;
      node.setPointerCapture?.(event.pointerId);
    };
    const onPointerMove = (event: PointerEvent) => {
      if (event.pointerType === "touch" || !points.has(event.pointerId)) return;
      event.preventDefault();
      place(event.pointerId, event.clientX, event.clientY);
      const dx = event.clientX - originX;
      const dy = event.clientY - originY;
      if (Math.abs(dx) + Math.abs(dy) > 4) moved = true;
      view.current.onPan({ x: view.current.pan.x + dx, y: view.current.pan.y + dy });
      originX = event.clientX;
      originY = event.clientY;
    };
    const onPointerUp = (event: PointerEvent) => {
      if (event.pointerType === "touch") return;
      points.delete(event.pointerId);
      if (!moved) {
        const now = Date.now();
        if (now - lastTap < 300) zoomBy(view.current.scale < 1.6 ? 2.2 / view.current.scale : 1 / view.current.scale);
        lastTap = now;
      }
    };

    const onTouchStart = (event: TouchEvent) => {
      for (const touch of Array.from(event.changedTouches)) place(touch.identifier, touch.clientX, touch.clientY);
      originX = event.touches[0]?.clientX || 0;
      originY = event.touches[0]?.clientY || 0;
      moved = false;
      distance = pinch();
    };
    const onTouchMove = (event: TouchEvent) => {
      event.preventDefault();
      for (const touch of Array.from(event.changedTouches)) place(touch.identifier, touch.clientX, touch.clientY);
      const nextDistance = pinch();
      if (event.touches.length >= 2 && distance > 0 && nextDistance > 0) {
        zoomBy(nextDistance / distance);
        distance = nextDistance;
        moved = true;
        return;
      }
      const touch = event.touches[0];
      if (!touch) return;
      const dx = touch.clientX - originX;
      const dy = touch.clientY - originY;
      if (Math.abs(dx) + Math.abs(dy) > 4) moved = true;
      view.current.onPan({ x: view.current.pan.x + dx, y: view.current.pan.y + dy });
      originX = touch.clientX;
      originY = touch.clientY;
    };
    const onTouchEnd = (event: TouchEvent) => {
      for (const touch of Array.from(event.changedTouches)) points.delete(touch.identifier);
      distance = pinch();
      if (points.size === 0 && !moved) {
        const now = Date.now();
        if (now - lastTap < 300) zoomBy(view.current.scale < 1.6 ? 2.2 / view.current.scale : 1 / view.current.scale);
        lastTap = now;
      }
    };

    node.addEventListener("pointerdown", onPointerDown);
    node.addEventListener("pointermove", onPointerMove);
    node.addEventListener("pointerup", onPointerUp);
    node.addEventListener("pointercancel", onPointerUp);
    node.addEventListener("touchstart", onTouchStart, { passive: true });
    node.addEventListener("touchmove", onTouchMove, { passive: false });
    node.addEventListener("touchend", onTouchEnd);
    node.addEventListener("touchcancel", onTouchEnd);
    return () => {
      node.removeEventListener("pointerdown", onPointerDown);
      node.removeEventListener("pointermove", onPointerMove);
      node.removeEventListener("pointerup", onPointerUp);
      node.removeEventListener("pointercancel", onPointerUp);
      node.removeEventListener("touchstart", onTouchStart);
      node.removeEventListener("touchmove", onTouchMove);
      node.removeEventListener("touchend", onTouchEnd);
      node.removeEventListener("touchcancel", onTouchEnd);
    };
  }, []);

  const zoom = (next: number) => onScale(Math.min(4, Math.max(1, next)));

  return (
    <div className="flow-full" data-swipe-ignore="" role="dialog" aria-modal="true" aria-label={title}>
      <div ref={stage} className="flow-full-stage">
        <div className="h-full w-full" style={{ transform: `translate(${pan.x}px, ${pan.y}px) scale(${scale})`, transformOrigin: "center center" }}>
          {children}
        </div>
      </div>
      <div className="flow-full-bar">
        <button type="button" className="fin-chip" onClick={onClose}>
          {t("Close")}
        </button>
          <button type="button" className="flow-zoom" onClick={() => zoom(scale / 1.25)} aria-label={t("Zoom out")}>
            −
          </button>
          <button type="button" className="flow-zoom" onClick={() => zoom(scale * 1.25)} aria-label={t("Zoom in")}>
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
  );
}
