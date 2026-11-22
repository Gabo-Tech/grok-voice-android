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
import { buildMoneyFlow, buildSankeyOption, type MoneyFlow } from "@/lib/moneyFlow";
import { budgetAsFlow, resolveBudget, shiftMonth, type BudgetRecord } from "@/lib/budget";
import { flowPercent, formatMoney } from "@/lib/money";
import { pushOverlay } from "@/lib/history";
import { activeLocale, fill, useT } from "@/lib/i18n";

use([SankeyChart, CanvasRenderer, TooltipComponent]);

const COLOR_KEY = "grok-flow-colors";
const OPTION_KEY = "grok-flow-options";

type FlowGroupMode = "category" | "entry";

interface FlowOptions {
  labels: boolean;
  values: boolean;
  group: FlowGroupMode;
  mono: boolean;
  planned: boolean;
}

function readColors(): Record<string, string> {
  try {
    const raw = localStorage.getItem(COLOR_KEY);
    const parsed = raw ? (JSON.parse(raw) as Record<string, string>) : {};
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function readOptions(): FlowOptions {
  try {
    const raw = localStorage.getItem(OPTION_KEY);
    const parsed = raw ? (JSON.parse(raw) as Partial<FlowOptions> & { mode?: string }) : {};
    const legacy = parsed.mode;
    const group: FlowGroupMode = parsed.group === "entry" || legacy === "entry" || legacy === "source" ? "entry" : "category";
    return {
      labels: parsed.labels !== false,
      values: parsed.values !== false,
      group,
      mono: parsed.mono === true || legacy === "mono",
      planned: parsed.planned === true,
    };
  } catch {
    return { labels: true, values: true, group: "category", mono: false, planned: false };
  }
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
  budgets = [],
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
  budgets?: BudgetRecord[];
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
    if (flow === "plain") return { labels: false, values: false, group: "category" as const, mono: false, planned: false };
    if (flow === "entry" || flow === "source") return { labels: true, values: true, group: "entry" as const, mono: false, planned: false };
    if (flow === "category") return { labels: true, values: true, group: "category" as const, mono: false, planned: false };
    if (flow === "planned") return { labels: true, values: true, group: "category" as const, mono: false, planned: true };
    return readOptions();
  });
  const [scale, setScale] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const book = useMemo(() => bookFromStatus(fx), [fx]);
  const nowMs = useMemo(() => Date.now(), [transactions, fx, display]);
  const kinds = useMemo(() => new Map(accounts.map((account) => [account.id, account.kind])), [accounts]);
  const names = useMemo(() => new Map(accounts.map((account) => [account.id, account.name])), [accounts]);
  const months = useMemo(() => monthNames(activeLocale()), []);
  const flowLabels = useMemo(
    () => ({
      income: t("Income"),
      totalIncome: t("Total income"),
      totalSpending: t("Total spending"),
      shortfall: t("Shortfall / From savings"),
      drawn: t("From savings"),
      total: t("Total"),
      surplus: t("Surplus"),
      other: t("Rest"),
      uncategorized: t("Uncategorized"),
      debt: t("Debt payments"),
      savings: t("Savings"),
      investments: t("Investments"),
      card: t("Card"),
      cash: t("Cash"),
      bank: t("Bank"),
      spending: t("Spending"),
    }),
    [t],
  );
  const flow = useMemo(() => {
    const plannedMonths = options.planned ? monthsCovered(bounds.start, bounds.end) : [];
    const plannedTxs = plannedMonths.flatMap((month) => budgetAsFlow(budgets.find((row) => row.period === month)?.lines || inheritLines(budgets, month), month));
    const rows = options.planned
      ? plannedTxs
      : transactions.map((tx) => ({
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
        }));
    return buildMoneyFlow(rows, {
      categories,
      display,
      book,
      nowMs,
      start: bounds.start,
      end: bounds.end,
      expandedOther: openOther,
      group: options.group,
      months,
      nodeColors: { ...Object.fromEntries(categories.filter((row) => row.color).map((row) => [row.name, row.color])), ...colors },
      labels: flowLabels,
    });
  }, [transactions, kinds, names, categories, display, book, nowMs, bounds.start, bounds.end, openOther, colors, options.group, options.planned, budgets, months, flowLabels]);

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
      if (detail.action === "planned") setOptions((current) => ({ ...current, planned: !current.planned }));
      const mode = detail.mode;
      if (mode === "entry" || mode === "source") setOptions((current) => ({ ...current, group: "entry", mono: false }));
      if (mode === "category") setOptions((current) => ({ ...current, group: "category", mono: false }));
      if (mode === "mono") setOptions((current) => ({ ...current, mono: true }));
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
      mono={options.mono}
      tall={full}
      onDrill={(id) => setDrill(id)}
    />
  );

  const controls = (
    <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1" data-swipe-ignore="">
      <button type="button" className="fin-chip" aria-pressed={options.labels} onClick={() => setOptions((current) => ({ ...current, labels: !current.labels }))}>
        {t("Labels")}
      </button>
      <button type="button" className="fin-chip" aria-pressed={options.values} onClick={() => setOptions((current) => ({ ...current, values: !current.values }))}>
        {t("Values")}
      </button>
      <button type="button" className="fin-chip" aria-pressed={options.group === "category" && !options.mono} onClick={() => setOptions((current) => ({ ...current, group: "category", mono: false }))}>
        {t("By category")}
      </button>
      <button type="button" className="fin-chip" aria-pressed={options.group === "entry"} onClick={() => setOptions((current) => ({ ...current, group: "entry", mono: false }))}>
        {t("By entry")}
      </button>
      <button type="button" className="fin-chip" aria-pressed={options.mono} onClick={() => setOptions((current) => ({ ...current, mono: !current.mono }))}>
        {t("One color")}
      </button>
      <button type="button" className="fin-chip" aria-pressed={options.planned} onClick={() => setOptions((current) => ({ ...current, planned: !current.planned }))}>
        {t("Planned")}
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

  const title = flowTitle(period, bounds, t);
  return (
    <ChartCard title={title}>
      <ChartPeriodBar period={period} onChange={setPeriod} />
      {controls}
      {drill ? (
        <button type="button" className="fin-chip mt-2" onClick={() => setDrill("")}>
          {t("Back")}
        </button>
      ) : null}
      <div className="mt-3 overflow-x-auto" data-swipe-ignore="" data-flow-state={flow.state}>
        {flow.state === "empty" ? (
          <p className="text-[15px] leading-5">{options.planned ? t("No budget for this period.") : t("Nothing recorded in this period. Income and spending will show up here.")}</p>
        ) : flow.state === "needs-rate" ? (
          <p className="text-[15px] leading-5">{fill("This chart needs a rate for {codes}.", { codes: flow.missing.join(", ") })}</p>
        ) : full ? null : chart}
        {flow.state === "ready" && flow.noIncome ? <p className="fin-muted mt-2 text-[13px] leading-5">{t("No income this month. Spending still shows.")}</p> : null}
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
              title={title}
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

function flowTitle(period: ChartPeriod, bounds: { start: string; end: string }, t: (english: string) => string): string {
  const locale = activeLocale() === "ca-ES-valencia" ? "ca-ES" : activeLocale();
  let name = t("This month");
  if ((period.preset === "this" || period.preset === "last") && bounds.start) {
    name = new Date(`${bounds.start}T12:00:00`).toLocaleDateString(locale, { month: "long", year: "numeric" });
  } else if (period.preset === "all") {
    name = t("All");
  } else if (bounds.start && bounds.end) {
    name = `${bounds.start} – ${bounds.end}`;
  }
  return fill("Income and spending, {period}", { period: name });
}

function monthNames(locale: string): string[] {
  const tag = locale === "ca-ES-valencia" ? "ca-ES" : locale;
  return Array.from({ length: 12 }, (_, index) => new Date(2026, index, 1).toLocaleDateString(tag, { month: "short" }).replace(".", ""));
}

function monthsCovered(start: string, end: string): string[] {
  if (!start) return [];
  const out: string[] = [];
  let cursor = start.slice(0, 7);
  const last = (end || start).slice(0, 7);
  while (cursor && cursor <= last && out.length < 18) {
    out.push(cursor);
    cursor = shiftMonth(cursor, 1);
  }
  return out;
}

function inheritLines(budgets: BudgetRecord[], month: string) {
  return resolveBudget(budgets, month).budget?.lines || [];
}

function usedColor(value: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  const probe = document.createElement("span");
  probe.style.color = value.startsWith("#") || value.startsWith("rgb") ? value : value.startsWith("var(") ? value : `var(${value})`;
  probe.style.display = "none";
  document.body.appendChild(probe);
  const color = getComputedStyle(probe).color;
  probe.remove();
  return color && color !== "rgba(0, 0, 0, 0)" ? color : fallback;
}

function contrast(fore: string, back: string): number {
  const a = rgb(fore);
  const b = rgb(back);
  if (!a || !b) return 7;
  const light = Math.max(luminance(a), luminance(b));
  const dark = Math.min(luminance(a), luminance(b));
  return (light + 0.05) / (dark + 0.05);
}

function readable(color: string, paper: string, ink: string): string {
  return contrast(color, paper) >= 4.5 ? color : ink;
}

function rgb(color: string): [number, number, number] | null {
  const match = color.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)/);
  if (!match) return null;
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

function luminance(channels: [number, number, number]): number {
  const linear = channels.map((channel) => {
    const c = channel / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
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
  mono,
  tall,
  onDrill,
}: {
  flow: MoneyFlow | null;
  currency: string;
  labelsOn: boolean;
  valuesOn: boolean;
  mono: boolean;
  tall: boolean;
  onDrill: (id: string) => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const chart = useRef<ECharts | null>(null);
  const drillRef = useRef(onDrill);
  drillRef.current = onDrill;
  const safeToReal = useRef(new Map<string, string>());
  const [box, setBox] = useState({ w: 0, h: 0 });
  const [drawn, setDrawn] = useState(460);

  useEffect(() => {
    const node = host.current;
    if (!node) return;
    const instance = init(node, undefined, { renderer: "canvas" });
    chart.current = instance;
    const onClick = (params: { dataType?: string; data?: { name?: string } | null }) => {
      const key = params.data && typeof params.data === "object" ? params.data.name || "" : "";
      const name = safeToReal.current.get(key) || "";
      if (params.dataType === "node" && name) drillRef.current(name);
    };
    instance.on("click", onClick as never);
    const measure = () => {
      const stage = node.closest(".flow-full-stage") as HTMLElement | null;
      const parent = node.parentElement;
      const width = Math.floor((stage || parent)?.clientWidth || node.clientWidth || window.innerWidth);
      const stageHeight = stage && stage.clientHeight > 160 ? stage.clientHeight : 0;
      const fromViewport = window.innerHeight - node.getBoundingClientRect().top - 8;
      const height = Math.floor(stageHeight || fromViewport);
      const next = { w: Math.max(280, width), h: Math.max(240, height) };
      setBox((prev) => (prev.w === next.w && prev.h === next.h ? prev : next));
    };
    measure();
    const observer = new ResizeObserver(measure);
    if (node.parentElement) observer.observe(node.parentElement);
    const stage = node.closest(".flow-full-stage");
    if (stage) observer.observe(stage);
    window.addEventListener("resize", measure);
    window.visualViewport?.addEventListener("resize", measure);
    screen.orientation?.addEventListener("change", measure);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", measure);
      window.visualViewport?.removeEventListener("resize", measure);
      screen.orientation?.removeEventListener("change", measure);
      instance.off("click", onClick);
      instance.dispose();
      chart.current = null;
    };
  }, [tall]);

  useEffect(() => {
    const instance = chart.current;
    if (!instance || !flow || flow.state !== "ready" || box.w < 280 || box.h < 240) return;
    const ink = usedColor("var(--text-primary)", "rgb(58, 52, 44)");
    const muted = usedColor("var(--text-tertiary)", "rgb(138, 129, 117)");
    const brand = usedColor("var(--brand)", "rgb(125, 107, 88)");
    const paper = usedColor("var(--surface-0)", "rgb(23, 24, 27)");
    const line = usedColor("var(--line)", "rgb(217, 210, 197)");
    const colors = Object.fromEntries(flow.nodes.map((node) => [node.id, readable(mono ? brand : usedColor(node.color, brand), paper, ink)]));
    const built = buildSankeyOption({
      nodes: flow.nodes,
      links: flow.links,
      currency,
      width: box.w,
      height: box.h,
      labels: labelsOn,
      values: valuesOn,
      colors,
      ink,
      muted,
      brand,
      paper,
      line,
      mono,
    });
    const frame = built.frame;
    const chartHeight = frame.height;
    setDrawn(chartHeight);
    if (host.current) host.current.style.height = `${chartHeight}px`;
    instance.resize({ width: frame.width, height: chartHeight });
    safeToReal.current = new Map(flow.nodes.map((node, index) => [`n${index}`, node.id]));
    const byId = new Map(flow.nodes.map((node, index) => [`n${index}`, node]));
    const option = built.option;
    option.tooltip = {
      trigger: "item",
      backgroundColor: usedColor("var(--surface-1)", "rgb(244, 241, 234)"),
      borderColor: line,
      textStyle: { color: ink, fontSize: 13 },
      formatter: (raw: unknown) => {
        const params = raw as { dataType?: string; data?: { name?: string; source?: string; target?: string; raw?: number } };
        const value = Number(params.data?.raw || 0);
        if (params.dataType === "edge") {
          const from = byId.get(params.data?.source || "");
          const to = byId.get(params.data?.target || "");
          return `${from?.label || ""} → ${to?.label || ""}<br/>${formatMoney(value, currency)}`;
        }
        const node = byId.get(params.data?.name || "");
        return `${node?.label || ""}<br/>${formatMoney(value, currency)} · ${flowPercent(node?.share || 0)}`;
      },
    };
    try {
      instance.setOption(option, true);
    } catch (error) {
      if (host.current) host.current.dataset.chartError = error instanceof Error ? error.message : "chart";
    }
    let publishing = false;
    const publish = () => {
      if (publishing) return;
      publishing = true;
      nudgeLabels(instance, frame.width, frame.height);
      const report = readLabelAudit(instance, frame.width, frame.height);
      publishing = false;
      const node = host.current;
      if (node) {
        node.dataset.labelAudit = report.ok ? "ok" : "fail";
        node.dataset.labelIssues = report.issues.slice(0, 6).join(" | ");
      }
      (window as Window & { __sankeyAudit?: unknown }).__sankeyAudit = report;
    };
    instance.off("finished");
    instance.on("finished", publish);
  }, [flow, currency, labelsOn, valuesOn, mono, tall, box]);

  return <div ref={host} data-shot="money-flow" style={{ width: "100%", maxWidth: "100%", height: drawn }} />;
}

interface AuditEl {
  ignore?: boolean;
  type?: string;
  style?: { text?: string; x?: number; y?: number };
  x?: number;
  y?: number;
  dirty?: () => void;
  transform?: number[];
  getBoundingRect?: () => { width: number; height: number; clone: () => { x: number; y: number; width: number; height: number; applyTransform: (m: number[]) => void } };
  getComputedTransform?: () => number[] | null;
}

function labelElements(instance: ECharts): Array<{ el: AuditEl; x: number; y: number; w: number; h: number; text: string }> {
  const zr = instance.getZr() as unknown as { storage: { getDisplayList?: (all?: boolean) => AuditEl[] } };
  const out: Array<{ el: AuditEl; x: number; y: number; w: number; h: number; text: string }> = [];
  for (const el of zr.storage.getDisplayList?.(true) || []) {
    const text = el.style?.text || "";
    if (!text.trim() || !el.getBoundingRect) continue;
    const rect = el.getBoundingRect().clone();
    const matrix = el.getComputedTransform?.() || el.transform;
    if (matrix) rect.applyTransform(matrix);
    if (rect.width < 2 || rect.height < 2) continue;
    out.push({ el, x: rect.x, y: rect.y, w: rect.width, h: rect.height, text: text.replace(/\n/g, " ") });
  }
  return out;
}

function boxesHit(a: { x: number; y: number; w: number; h: number }, b: { x: number; y: number; w: number; h: number }): boolean {
  return a.x < b.x + b.w - 1 && a.x + a.w > b.x + 1 && a.y < b.y + b.h - 1 && a.y + a.h > b.y + 1;
}

function nudgeLabels(instance: ECharts, width: number, height: number) {
  const items = labelElements(instance);
  const move = (item: (typeof items)[number], dx: number, dy: number) => {
    item.el.x = (item.el.x || 0) + dx;
    item.el.y = (item.el.y || 0) + dy;
    item.x += dx;
    item.y += dy;
    item.el.dirty?.();
  };
  for (let pass = 0; pass < 16; pass += 1) {
    let moved = false;
    for (let i = 0; i < items.length; i += 1) {
      for (let j = i + 1; j < items.length; j += 1) {
        if (!boxesHit(items[i], items[j])) continue;
        const lower = items[i].y <= items[j].y ? items[j] : items[i];
        const upper = lower === items[i] ? items[j] : items[i];
        move(lower, 0, upper.y + upper.h - lower.y + 6);
        moved = true;
      }
      const item = items[i];
      if (item.y + item.h > height - 2) {
        move(item, 0, height - 2 - (item.y + item.h));
        moved = true;
      }
      if (item.y < 2) {
        move(item, 0, 2 - item.y);
        moved = true;
      }
      if (item.x < 2) {
        move(item, 2 - item.x, 0);
        moved = true;
      }
      if (item.x + item.w > width - 2) {
        move(item, width - 2 - (item.x + item.w), 0);
        moved = true;
      }
    }
    if (!moved) break;
  }
  instance.getZr().refresh();
}

function readLabelAudit(instance: ECharts, width: number, height: number): { ok: boolean; issues: string[]; count: number; boxes: Array<{ t: string; x: number; y: number; w: number; h: number }> } {
  const boxes: Array<{ x: number; y: number; w: number; h: number; text: string }> = [];
  const kinds = new Map<string, number>();
  const zr = instance.getZr() as unknown as { storage: { traverse: (fn: (el: AuditEl) => void) => void; getDisplayList?: (all?: boolean) => AuditEl[] } };
  const visit = (el: AuditEl) => {
    const key = `${el.type || "?"}:${el.style?.text ? "t" : "-"}`;
    kinds.set(key, (kinds.get(key) || 0) + 1);
    const text = el.style?.text || "";
    if (!text.trim()) return;
    const rect = el.getBoundingRect?.().clone();
    if (!rect) return;
    const matrix = el.getComputedTransform?.() || el.transform;
    if (matrix) rect.applyTransform(matrix);
    if (rect.width < 2 || rect.height < 2) return;
    boxes.push({ x: rect.x, y: rect.y, w: rect.width, h: rect.height, text: text.replace(/\n/g, " ") });
  };
  for (const el of zr.storage.getDisplayList?.(true) || []) visit(el);
  const issues: string[] = [];
  const hit = (a: (typeof boxes)[number], b: (typeof boxes)[number]) => a.x < b.x + b.w - 1 && a.x + a.w > b.x + 1 && a.y < b.y + b.h - 1 && a.y + a.h > b.y + 1;
  for (let i = 0; i < boxes.length; i += 1) {
    const box = boxes[i];
    if (box.x < -1 || box.y < -1 || box.x + box.w > width + 1 || box.y + box.h > height + 1) issues.push(`outside ${box.text}`);
    for (let j = i + 1; j < boxes.length; j += 1) {
      if (hit(box, boxes[j])) issues.push(`overlap ${box.text} / ${boxes[j].text}`);
    }
  }
  if (!boxes.length) issues.push([...kinds.entries()].map(([key, count]) => `${key}:${count}`).join(",") || "no elements");
  return {
    ok: issues.length === 0 && boxes.length > 0,
    issues,
    count: boxes.length,
    boxes: boxes.map((box) => ({ t: box.text, x: Math.round(box.x), y: Math.round(box.y), w: Math.round(box.w), h: Math.round(box.h) })),
  };
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
      <div className="mt-1 flex flex-wrap gap-x-3 gap-y-1">
        {rows.map((row) => {
          const swatch = /^#[0-9A-Fa-f]{6}$/.test(colors[row.name] || row.color) ? colors[row.name] || row.color : "var(--brand)";
          return (
            <label key={row.name} className="flex min-h-11 max-w-full items-center gap-2 text-[13px] text-[var(--text-primary)]">
              <span className="relative h-11 w-11 shrink-0">
                <span aria-hidden="true" className="absolute inset-3" style={{ background: swatch }} />
                <input
                  type="color"
                  aria-label={fill("{name} color", { name: row.name })}
                  value={/^#[0-9A-Fa-f]{6}$/.test(swatch) ? swatch : "#7d6b58"}
                  onChange={(event) => onChange(row.name, event.target.value)}
                  className="absolute inset-0 h-11 w-11 cursor-pointer opacity-0"
                />
              </span>
              <span className="min-w-0 truncate">{row.name}</span>
            </label>
          );
        })}
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
    const orientation = screen.orientation as ScreenOrientation & { lock?: (mode: string) => Promise<void>; unlock?: () => void };
    void orientation?.lock?.("landscape")?.catch(() => undefined);
    return () => orientation?.unlock?.();
  }, []);

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
