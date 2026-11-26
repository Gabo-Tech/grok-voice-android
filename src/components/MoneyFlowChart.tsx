import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { init, use, type ECharts } from "echarts/core";
import { SankeyChart } from "echarts/charts";
import { CanvasRenderer } from "echarts/renderers";
import { TooltipComponent } from "echarts/components";
import type { FinanceAccount, FinanceCategory, FinanceFx, FinanceTransaction } from "@/lib/api";
import { ChartCard } from "@/components/ChartKit";
import { ColorSheet } from "@/components/ColorSheet";
import { ChartPeriodBar, useChartPeriod } from "@/components/ChartPeriodBar";
import { bookFromStatus } from "@/lib/financeMath";
import { boundsOf, type ChartPeriod } from "@/lib/chartPeriod";
import { buildMoneyFlow, buildSankeyOption, fitValueScale, flowSlices, spreadColumn, type FlowLink, type FlowNode, type MoneyFlow, type SankeyFrame } from "@/lib/moneyFlow";
import { budgetAsFlow, resolveBudget, shiftMonth, type BudgetRecord } from "@/lib/budget";
import { flowPercent, formatMoney } from "@/lib/money";
import { pushOverlay } from "@/lib/history";
import { activeLocale, fill, useT } from "@/lib/i18n";

use([SankeyChart, CanvasRenderer, TooltipComponent]);

const COLOR_KEY = "grok-flow-colors";
const OPTION_KEY = "grok-flow-options";
const PANEL_KEY = "grok-flow-panel";

type FlowGroupMode = "category" | "entry";

type PortraitLayout = "vertical" | "horizontal";

interface FlowOptions {
  labels: boolean;
  values: boolean;
  group: FlowGroupMode;
  mono: boolean;
  planned: boolean;
  portraitLayout: PortraitLayout;
}

function readPanelOpen(): boolean {
  if (typeof window === "undefined") return false;
  const asked = new URLSearchParams(window.location.search).get("options");
  if (asked === "open") return true;
  if (asked === "closed") return false;
  try {
    return sessionStorage.getItem(PANEL_KEY) === "open";
  } catch {
    return false;
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
      portraitLayout: parsed.portraitLayout === "horizontal" ? "horizontal" : "vertical",
    };
  } catch {
    return { labels: true, values: true, group: "category", mono: false, planned: false, portraitLayout: "vertical" };
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
  flowColors = [],
  flowLayout = "",
  onPaintNode,
  onLayout,
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
  flowColors?: { id: string; color: string }[];
  flowLayout?: string;
  onPaintNode?: (id: string, color: string) => void;
  onLayout?: (layout: PortraitLayout) => void;
}) {
  const t = useT();
  const own = useChartPeriod("flow");
  const period = externalPeriod && !detached ? externalPeriod : own.period;
  const setPeriod = (next: ChartPeriod) => {
    onDetach?.(next);
    own.setPeriod(next);
  };
  const bounds = useMemo(() => boundsOf(period), [period]);
  const [colors, setColors] = useState<Record<string, string>>(() => Object.fromEntries((flowColors || []).filter((row) => row.id && row.color).map((row) => [row.id, row.color])));
  const [openOther, setOpenOther] = useState(false);
  const [panelOpen, setPanelOpen] = useState(readPanelOpen);
  const [colorsOpen, setColorsOpen] = useState(() => typeof window !== "undefined" && new URLSearchParams(window.location.search).get("colors") === "open");
  const [full, setFull] = useState(() => typeof window !== "undefined" && (new URLSearchParams(window.location.search).get("flow") === "full" || new URLSearchParams(window.location.search).get("full") === "1"));
  const [drill, setDrill] = useState("");
  const [options, setOptions] = useState(() => {
    if (typeof window === "undefined") return readOptions();
    const params = new URLSearchParams(window.location.search);
    const flow = params.get("flow");
    const layout: PortraitLayout = params.get("portraitLayout") === "horizontal" ? "horizontal" : flowLayout === "horizontal" ? "horizontal" : "vertical";
    const labels = params.get("labels") !== "0";
    if (flow === "plain") return { labels: false, values: false, group: "category" as const, mono: false, planned: false, portraitLayout: layout };
    if (flow === "entry" || flow === "source") return { labels, values: true, group: "entry" as const, mono: false, planned: false, portraitLayout: layout };
    if (flow === "category") return { labels, values: true, group: "category" as const, mono: false, planned: false, portraitLayout: layout };
    if (flow === "planned") return { labels: true, values: true, group: "category" as const, mono: false, planned: true, portraitLayout: layout };
    const saved = readOptions();
    return { ...saved, portraitLayout: new URLSearchParams(window.location.search).has("portraitLayout") ? layout : saved.portraitLayout };
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
      otherIncome: t("Other income"),
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
      nodeColors: {
        ...Object.fromEntries(categories.filter((row) => row.color).map((row) => [row.name, row.color])),
        ...Object.fromEntries(flowColors.filter((row) => row.id && row.color).map((row) => [row.id, row.color])),
        ...colors,
      },
      labels: flowLabels,
    });
  }, [transactions, kinds, names, categories, display, book, nowMs, bounds.start, bounds.end, openOther, colors, flowColors, options.group, options.planned, budgets, months, flowLabels]);

  useEffect(() => {
    setColors(Object.fromEntries((flowColors || []).filter((row) => row.id && row.color).map((row) => [row.id, row.color])));
  }, [flowColors]);

  useEffect(() => {
    try {
      const raw = localStorage.getItem(COLOR_KEY);
      if (!raw) return;
      const parsed = JSON.parse(raw) as Record<string, string>;
      localStorage.removeItem(COLOR_KEY);
      for (const [id, color] of Object.entries(parsed || {})) {
        if (id && color) onPaintNode?.(id, color);
      }
    } catch {
      /* A phone without saved colors just uses the series palette. */
    }
  }, [onPaintNode]);

  useEffect(() => {
    try {
      sessionStorage.setItem(PANEL_KEY, panelOpen ? "open" : "closed");
    } catch {
      /* The panel still opens for this visit. */
    }
  }, [panelOpen]);

  useEffect(() => {
    if (typeof window === "undefined" || new URLSearchParams(window.location.search).has("portraitLayout")) return;
    try {
      if (localStorage.getItem(OPTION_KEY)) return;
    } catch {
      return;
    }
    if (flowLayout === "vertical" || flowLayout === "horizontal") {
      setOptions((current) => ({ ...current, portraitLayout: flowLayout }));
    }
  }, [flowLayout]);

  useEffect(() => {
    try {
      localStorage.setItem(OPTION_KEY, JSON.stringify(options));
    } catch {
      /* Display options still apply for this visit. */
    }
  }, [options]);

  useEffect(() => {
    const onVoice = (event: Event) => {
      const detail = (event as CustomEvent<{ chart?: string; action?: string; mode?: string; name?: string; color?: string; confirmed?: boolean }>).detail || {};
      if (detail.chart && detail.chart !== "flow") return;
      if (detail.action === "labels") setOptions((current) => ({ ...current, labels: !current.labels }));
      if (detail.action === "values") setOptions((current) => ({ ...current, values: !current.values }));
      if (detail.action === "fullscreen") setFull(true);
      if (detail.action === "planned") setOptions((current) => ({ ...current, planned: !current.planned }));
      const mode = detail.mode;
      if (mode === "entry" || mode === "source") setOptions((current) => ({ ...current, group: "entry", mono: false }));
      if (mode === "category") setOptions((current) => ({ ...current, group: "category", mono: false }));
      if (mode === "mono") setOptions((current) => ({ ...current, mono: true }));
      if (mode === "vertical" || mode === "horizontal") {
        setOptions((current) => ({ ...current, portraitLayout: mode }));
        onLayout?.(mode);
      }
      const action = (detail.action || "").toLowerCase();
      const name = (detail.name || "").trim();
      if ((action === "color" || action === "paint") && name && detail.color) {
        paintColor(name, detail.color, setColors, onPaintNode);
      }
      if (action === "reset") {
        const all = !name || name === "all" || name.toLowerCase() === "colors";
        if (all && !detail.confirmed) return;
        if (all) {
          setColors({});
          try {
            localStorage.removeItem(COLOR_KEY);
          } catch {
            /* The chart still uses the defaults until reload. */
          }
          onPaintNode?.("all", "");
          return;
        }
        paintColor(name, "", setColors, onPaintNode);
      }
    };
    window.addEventListener("grok-finance-voice", onVoice);
    return () => window.removeEventListener("grok-finance-voice", onVoice);
  }, [onLayout, onPaintNode]);

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
      portraitLayout={options.portraitLayout}
      spendingLabel={t("Spending")}
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
      <button type="button" className="fin-chip" aria-pressed={options.portraitLayout === "vertical"} onClick={() => { setOptions((current) => ({ ...current, portraitLayout: "vertical" })); onLayout?.("vertical"); }}>
        {t("Top to bottom")}
      </button>
      <button type="button" className="fin-chip" aria-pressed={options.portraitLayout === "horizontal"} onClick={() => { setOptions((current) => ({ ...current, portraitLayout: "horizontal" })); onLayout?.("horizontal"); }}>
        {t("Left to right")}
      </button>
      <button type="button" className="fin-chip" aria-pressed={options.mono} onClick={() => setOptions((current) => ({ ...current, mono: !current.mono }))}>
        {t("One color")}
      </button>
      <button type="button" className="fin-chip" aria-pressed={options.planned} onClick={() => setOptions((current) => ({ ...current, planned: !current.planned }))}>
        {t("Planned")}
      </button>
      <button type="button" className="fin-chip min-h-11" onClick={() => setColorsOpen(true)}>
        {t("Chart colors")}
      </button>
      <button type="button" className="fin-chip" onClick={() => setOpenOther((value) => !value)}>
        {openOther ? t("Group small flows") : t("Show small flows")}
      </button>
      {detached && onFollow ? (
        <button type="button" className="fin-chip" onClick={onFollow}>
          {t("Use the dashboard period")}
        </button>
      ) : null}
    </div>
  );

  const title = flowTitle(period, bounds, t);
  const periodName = periodCaption(period, bounds, t);
  const paint = (id: string, color: string) => {
    paintColor(id, color, setColors, onPaintNode);
    const node = ready?.nodes.find((row) => row.id === id);
    const category = categories.find((row) => row.name === id || row.name === node?.label);
    if (category) onPaintCategory?.(category.name, color);
  };
  return (
    <ChartCard title={title}>
      <div className="mt-2 flex items-center justify-between gap-2" data-swipe-ignore="">
        <p className="min-w-0 truncate text-[15px] leading-5">{periodName}</p>
        <div className="flex shrink-0 items-center">
          <button type="button" className="grid h-11 w-11 place-items-center" aria-expanded={panelOpen} aria-controls="flow-options" aria-label={t("Options")} onClick={() => setPanelOpen((value) => !value)}>
            <OptionsIcon />
          </button>
          <button type="button" className="grid h-11 w-11 place-items-center" aria-expanded={colorsOpen} aria-label={t("Chart colors")} onClick={() => setColorsOpen((value) => !value)}>
            <PaletteIcon />
          </button>
          <button type="button" className="grid h-11 w-11 place-items-center" aria-label={t("Full screen")} onClick={() => setFull(true)}>
            <ExpandIcon />
          </button>
        </div>
      </div>
      <div id="flow-options" className={panelOpen ? "flow-panel open" : "flow-panel"}>
        <div>
          <ChartPeriodBar period={period} onChange={setPeriod} />
          {controls}
        </div>
      </div>
      {colorsOpen && ready ? (
        <ColorSheet
          series={ready.nodes.filter((node) => node.role !== "child" && node.amountMinor > 0).map((node) => ({
            id: node.id,
            label: node.label,
            color: colors[node.id] || colors[node.label] || node.color,
            amountMinor: node.amountMinor,
            group: colorGroup(node),
          }))}
          currency={display}
          groups={[
            { id: "income", label: t("Income") },
            { id: "spend", label: t("Spending") },
            { id: "save", label: t("Savings and investments") },
          ]}
          onPaint={paint}
          onResetAll={() => paint("all", "")}
          onClose={() => setColorsOpen(false)}
        />
      ) : null}
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
            </FullFlow>,
            document.body,
          )
        : null}
    </ChartCard>
  );
}

function periodCaption(period: ChartPeriod, bounds: { start: string; end: string }, t: (english: string) => string): string {
  const locale = activeLocale() === "ca-ES-valencia" ? "ca-ES" : activeLocale();
  if ((period.preset === "this" || period.preset === "last") && bounds.start) {
    return new Date(`${bounds.start}T12:00:00`).toLocaleDateString(locale, { month: "long", year: "numeric" });
  }
  if (period.preset === "all") return t("All");
  if (bounds.start && bounds.end && period.preset === "custom") return `${bounds.start} – ${bounds.end}`;
  const labels: Record<string, string> = { this: "This month", last: "Last month", "3m": "3M", "6m": "6M", ytd: "YTD", "1y": "1Y", all: "All", custom: "Custom" };
  return t(labels[period.preset] || "This month");
}

function flowTitle(period: ChartPeriod, bounds: { start: string; end: string }, t: (english: string) => string): string {
  return fill("Income and spending, {period}", { period: periodCaption(period, bounds, t) });
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
  portraitLayout,
  spendingLabel,
  onDrill,
}: {
  flow: MoneyFlow | null;
  currency: string;
  labelsOn: boolean;
  valuesOn: boolean;
  mono: boolean;
  tall: boolean;
  portraitLayout: "vertical" | "horizontal";
  spendingLabel: string;
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
      let below = 8;
      if (!stage) {
        let ancestor = node.parentElement;
        while (ancestor && ancestor !== document.body) {
          const style = getComputedStyle(ancestor);
          below += parseFloat(style.paddingBottom) || 0;
          below += parseFloat(style.borderBottomWidth) || 0;
          ancestor = ancestor.parentElement;
        }
        below += 16;
      }
      const fromViewport = window.innerHeight - node.getBoundingClientRect().top - below;
      let height = Math.floor(stageHeight || fromViewport);
      if (!stage) {
        const extra = document.documentElement.scrollHeight - window.innerHeight;
        if (extra > 2) height -= extra + 2;
      }
      const next = { w: Math.max(280, width), h: Math.max(160, height) };
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
    if (!instance || !flow || flow.state !== "ready" || box.w < 280 || box.h < 160) return;
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
      portraitLayout,
      spendingLabel,
    });
    const frame = built.frame;
    const chartHeight = frame.height;
    setDrawn(chartHeight);
    if (host.current) host.current.style.height = `${chartHeight}px`;
    instance.resize({ width: frame.width, height: chartHeight });
    safeToReal.current = new Map(built.ids.map((id, index) => [`n${index}`, id]));
    const byId = new Map(built.ids.map((id, index) => [`n${index}`, flow.nodes.find((node) => node.id === id) as FlowNode | undefined]));
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
      (instance as ECharts & { __flowSpread?: boolean }).__flowSpread = false;
      instance.setOption(option, true);
    } catch (error) {
      if (host.current) host.current.dataset.chartError = error instanceof Error ? error.message : "chart";
    }
    let publishing = false;
    const publish = () => {
      if (publishing) return;
      publishing = true;
      const marked = instance as ECharts & { __flowSpread?: boolean };
      if (frame.vertical) {
        if (!marked.__flowSpread) {
          marked.__flowSpread = true;
          spreadVertical(instance, frame, built.nodes, built.links);
        }
      } else if (frame.reference) {
        if (!marked.__flowSpread) {
          marked.__flowSpread = true;
          spreadReference(instance, frame, built.nodes, built.links);
        }
      } else if (frame.portrait) {
        if (!marked.__flowSpread) {
          marked.__flowSpread = true;
          spreadPortrait(instance, frame, built.nodes, built.links);
        }
      } else {
        nudgeLabels(instance, frame.width, frame.height);
        liftSpending(instance);
      }
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
  }, [flow, currency, labelsOn, valuesOn, mono, tall, box, portraitLayout, spendingLabel]);

  useEffect(() => {
    if (tall) return;
    const frame = requestAnimationFrame(() => {
      const extra = document.documentElement.scrollHeight - window.innerHeight;
      if (extra > 2 && box.h > 160) setBox((prev) => ({ ...prev, h: Math.max(160, prev.h - extra - 2) }));
    });
    return () => cancelAnimationFrame(frame);
  }, [drawn, tall, box.h]);

  return <div ref={host} data-shot="money-flow" style={{ width: "100%", maxWidth: "100%", height: tall ? "100%" : drawn }} />;
}

interface AuditEl {
  ignore?: boolean;
  type?: string;
  style?: { text?: string; x?: number; y?: number; fill?: string; fontSize?: number; fontWeight?: number; fontStyle?: string };
  x?: number;
  y?: number;
  parent?: AuditEl;
  rotation?: number;
  originX?: number;
  originY?: number;
  shape?: { x?: number; y?: number; width?: number; height?: number; x1?: number; y1?: number; x2?: number; y2?: number; cpx1?: number; cpy1?: number; cpx2?: number; cpy2?: number; extent?: number; orient?: "vertical" | "horizontal" };
  setShape?: (shape: AuditEl["shape"]) => void;
  dirty?: () => void;
  transform?: number[];
  getBoundingRect?: () => { width: number; height: number; clone: () => { x: number; y: number; width: number; height: number; applyTransform: (m: number[]) => void } };
  getComputedTransform?: () => number[] | null;
}

function labelElements(instance: ECharts): Array<{ el: AuditEl; x: number; y: number; w: number; h: number; text: string; parent?: AuditEl }> {
  const zr = instance.getZr() as unknown as { storage: { getDisplayList?: (all?: boolean) => AuditEl[] } };
  const out: Array<{ el: AuditEl; x: number; y: number; w: number; h: number; text: string; parent?: AuditEl }> = [];
  for (const el of zr.storage.getDisplayList?.(true) || []) {
    const text = el.style?.text || "";
    if (!text.trim() || !el.getBoundingRect) continue;
    const rect = el.getBoundingRect().clone();
    const matrix = el.getComputedTransform?.() || el.transform;
    if (matrix) rect.applyTransform(matrix);
    if (rect.width < 2 || rect.height < 2) continue;
    out.push({ el, parent: el.parent, x: rect.x, y: rect.y, w: rect.width, h: rect.height, text: text.replace(/\n/g, " ") });
  }
  return out;
}

function boxesHit(a: { x: number; y: number; w: number; h: number }, b: { x: number; y: number; w: number; h: number }): boolean {
  return a.x < b.x + b.w - 1 && a.x + a.w > b.x + 1 && a.y < b.y + b.h - 1 && a.y + a.h > b.y + 1;
}

function measure(el: AuditEl): { x: number; y: number; w: number; h: number } | null {
  const rect = el.getBoundingRect?.().clone();
  if (!rect) return null;
  const matrix = el.getComputedTransform?.() || el.transform;
  if (matrix) rect.applyTransform(matrix);
  return { x: rect.x, y: rect.y, w: rect.width, h: rect.height };
}

/** Stack ribbons in the same order at both ends so a later slot never crosses an earlier one. */
function stackFlowCurves(
  curves: AuditEl[],
  links: FlowLink[],
  placed: Map<number, { localX: number; localY: number; w: number; h: number }>,
  idIndex: Map<string, number>,
  axis: "x" | "y",
) {
  const along = (index: number, end: "source" | "target") => {
    const id = end === "source" ? links[index]?.source : links[index]?.target;
    const box = id == null ? undefined : placed.get(idIndex.get(id) ?? -1);
    if (!box) return 0;
    return axis === "x" ? box.localX : box.localY;
  };
  const outGroups = new Map<number, number[]>();
  const inGroups = new Map<number, number[]>();
  links.forEach((link, index) => {
    const source = idIndex.get(link.source);
    const target = idIndex.get(link.target);
    if (source == null || target == null) return;
    const out = outGroups.get(source) || [];
    out.push(index);
    outGroups.set(source, out);
    const inn = inGroups.get(target) || [];
    inn.push(index);
    inGroups.set(target, inn);
  });
  const outStart = new Map<number, number>();
  const outExtent = new Map<number, number>();
  const inStart = new Map<number, number>();
  const inExtent = new Map<number, number>();
  for (const [source, indexes] of outGroups) {
    const src = placed.get(source);
    if (!src) continue;
    const ordered = [...indexes].sort((a, b) => along(a, "target") - along(b, "target"));
    const span = axis === "y" ? src.h : src.w;
    const slices = flowSlices(ordered.map((index) => links[index].amountMinor), span);
    ordered.forEach((index, slot) => {
      outStart.set(index, slices[slot]?.start || 0);
      outExtent.set(index, slices[slot]?.extent || 1);
    });
  }
  for (const [target, indexes] of inGroups) {
    const dst = placed.get(target);
    if (!dst) continue;
    const ordered = [...indexes].sort((a, b) => along(a, "source") - along(b, "source"));
    const span = axis === "y" ? dst.h : dst.w;
    const slices = flowSlices(ordered.map((index) => links[index].amountMinor), span);
    ordered.forEach((index, slot) => {
      inStart.set(index, slices[slot]?.start || 0);
      inExtent.set(index, slices[slot]?.extent || 1);
    });
  }
  curves.forEach((curve, index) => {
    const link = links[index];
    if (!link) return;
    const source = idIndex.get(link.source);
    const target = idIndex.get(link.target);
    if (source == null || target == null) return;
    const src = placed.get(source);
    const dst = placed.get(target);
    if (!src || !dst) return;
    const extent = axis === "y"
      ? Math.max(1, Math.min(outExtent.get(index) || src.h, inExtent.get(index) || dst.h))
      : Math.max(0, outExtent.get(index) || src.w);
    if (axis === "y") {
      const y1 = src.localY + (outStart.get(index) || 0);
      const y2 = dst.localY + (inStart.get(index) || 0);
      const x1 = src.localX + src.w;
      const x2 = dst.localX;
      curve.setShape?.({
        x1,
        x2,
        y1,
        y2,
        cpy1: y1,
        cpy2: y2,
        cpx1: x1 + (x2 - x1) * 0.45,
        cpx2: x1 + (x2 - x1) * 0.55,
        extent,
      });
    } else {
      const x1 = src.localX + (outStart.get(index) || 0);
      const x2 = dst.localX + (inStart.get(index) || 0);
      const y1 = src.localY + src.h;
      const y2 = dst.localY;
      curve.setShape?.({
        orient: "vertical",
        x1,
        x2,
        y1,
        y2,
        cpx1: x1,
        cpx2: x2,
        cpy1: y1 + (y2 - y1) * 0.45,
        cpy2: y1 + (y2 - y1) * 0.55,
        extent,
      });
    }
    curve.dirty?.();
  });
}

/** Narrow portrait, left to right. Even rows, hairline tails, labels beside each bar. */
function spreadReference(instance: ECharts, frame: SankeyFrame, nodes: FlowNode[], links: FlowLink[]) {
  const zr = instance.getZr() as unknown as { storage: { getDisplayList?: (all?: boolean) => AuditEl[] }; refresh: () => void };
  const list = zr.storage.getDisplayList?.(true) || [];
  const rects = list.filter((el) => el.type === "rect" && el.shape && (el.shape.width || 0) >= 2 && (el.shape.width || 0) <= 24 && (el.shape.height || 0) >= 1);
  const curves = list.filter((el) => el.shape && el.shape.x1 != null && el.shape.y1 != null);
  if (rects.length !== nodes.length) return;
  const paired = rects.map((el, index) => ({
    el,
    index,
    node: nodes[index],
    originY: el.parent?.y || 0,
    old: { x: el.shape?.x || 0, y: el.shape?.y || 0, w: el.shape?.width || 0, h: el.shape?.height || 0 },
  }));
  const columns: Array<typeof paired> = [];
  for (const item of [...paired].sort((a, b) => a.old.x - b.old.x)) {
    const column = columns.find((group) => Math.abs(group[0].old.x - item.old.x) < 6);
    if (column) column.push(item);
    else columns.push([item]);
  }
  const plotH = frame.plot;
  const plotW = Math.max(40, frame.width - frame.left - frame.right);
  const labelBand = frame.lineHeight * 3 + 8;
  const flowTop = labelBand;
  const flowH = Math.max(44, plotH - labelBand);
  const placed = new Map<number, { localX: number; localY: number; w: number; h: number }>();
  columns.forEach((column) => {
    const hub = column.some((item) => item.node.role === "pool");
    const source = column.every((item) => item.node.role === "source");
    const ordered = [...column].sort((a, b) => {
      if (a.node.italic !== b.node.italic) return a.node.italic ? 1 : -1;
      return b.node.amountMinor - a.node.amountMinor;
    });
    const localX = hub ? Math.round(plotW * 0.58) : source ? 0 : Math.max(0, plotW - 4);
    const hubH = Math.min(72, Math.max(28, Math.round(flowH * 0.34)));
    if (!hub) {
      const count = Math.max(1, ordered.length);
      const slot = flowH / count;
      const total = Math.max(1, ordered.reduce((sum, item) => sum + item.node.amountMinor, 0));
      ordered.forEach((item, index) => {
        const h = (item.node.amountMinor / total) * hubH;
        const y = count === 1 ? flowTop + 4 : flowTop + index * slot + Math.max(0, (slot - h) / 2);
        item.el.setShape?.({ x: localX, y, height: h });
        item.el.dirty?.();
        placed.set(item.index, { localX, localY: y, w: 4, h });
      });
      return;
    }
    const y = Math.max(frame.lineHeight * 3, flowTop - 8);
    ordered.forEach((item) => {
      item.el.setShape?.({ x: localX, y, height: hubH });
      item.el.dirty?.();
      placed.set(item.index, { localX, localY: y, w: 4, h: hubH });
    });
  });
  const idIndex = new Map(nodes.map((node, index) => [node.id, index]));
  stackFlowCurves(curves, links, placed, idIndex, "y");
  zr.refresh();
  const muted = usedColor("var(--text-tertiary)", "rgb(138, 129, 117)");
  const texts = (zr.storage.getDisplayList?.(true) || []).flatMap((el) => {
    const label = (el.style?.text || "").trim();
    const box = label ? measure(el) : null;
    return box ? [{ el, text: label.replace(/\n/g, " "), ...box }] : [];
  });
  const ownerOf = (el: AuditEl) => {
    const seen = new Set<AuditEl>();
    let current: AuditEl | undefined = el;
    while (current && !seen.has(current)) {
      seen.add(current);
      const hit = paired.find((item) => item.el === current);
      if (hit) return hit;
      const host = (current as AuditEl & { __hostTarget?: AuditEl }).__hostTarget;
      if (host) {
        const hostHit = paired.find((item) => item.el === host);
        if (hostHit) return hostHit;
      }
      current = current.parent;
    }
    return undefined;
  };
  const groups = new Map<number, typeof texts>();
  for (const text of texts) {
    const owner = ownerOf(text.el);
    if (!owner) continue;
    const group = groups.get(owner.index) || [];
    group.push(text);
    groups.set(owner.index, group);
  }
  for (const item of paired) {
    const box = placed.get(item.index);
    const lines = (groups.get(item.index) || []).sort((a, b) => a.y - b.y);
    if (!box || !lines.length) continue;
    const globalX = (item.el.parent?.x || 0) + box.localX;
    const globalY = item.originY + box.localY;
    const nodeW = item.el.shape?.width || item.old.w;
    const blockH = lines.length * frame.lineHeight;
    const role = item.node.role;
    const middle = role === "pool";
    let cursor = middle ? Math.max(2, globalY - 6 - blockH) : Math.max(frame.seriesTop + labelBand, globalY + box.h / 2 - blockH / 2);
    cursor = Math.max(2, cursor);
    for (const line of lines) {
      const now = measure(line.el);
      if (!now) continue;
      const amount = line.text.includes("·") || /^\d/.test(line.text.trim());
      const fill = amount ? muted : String(item.el.style?.fill || muted);
      line.el.style = { ...line.el.style, fill, fontSize: 13, fontWeight: amount ? 400 : 600, fontStyle: item.node.italic ? "italic" : "normal" };
      line.el.dirty?.();
      const painted = measure(line.el) || now;
      let x = middle ? (frame.left + (frame.width - frame.right)) / 2 - painted.w / 2 : role === "source" ? globalX - 6 - painted.w : globalX + nodeW + 6;
      if (middle) x = Math.max(frame.left, Math.min(x, frame.width - frame.right - painted.w));
      line.el.x = (line.el.x || 0) + (x - painted.x);
      line.el.y = (line.el.y || 0) + (cursor - painted.y);
      line.el.dirty?.();
      cursor += frame.lineHeight;
    }
  }
  zr.refresh();
}

/** Narrow portrait, top to bottom. Every node uses one px-per-value, so a ribbon keeps that width from end to end. */
function spreadVertical(instance: ECharts, frame: SankeyFrame, nodes: FlowNode[], links: FlowLink[]) {
  const zr = instance.getZr() as unknown as { storage: { getDisplayList?: (all?: boolean) => AuditEl[] }; refresh: () => void; add?: (el: AuditEl) => void };
  const list = zr.storage.getDisplayList?.(true) || [];
  const rects = list.filter((el) => el.type === "rect" && el.shape && (el.shape.height || 0) >= 2 && (el.shape.height || 0) <= 12);
  const curves = list.filter((el) => el.shape && el.shape.x1 != null && el.shape.y1 != null);
  if (rects.length !== nodes.length) return;
  const paired = rects.map((el, index) => ({
    el,
    index,
    node: nodes[index],
    originX: el.parent?.x || 0,
    originY: el.parent?.y || 0,
    old: { x: el.shape?.x || 0, y: el.shape?.y || 0, w: el.shape?.width || 0, h: el.shape?.height || 0 },
  }));
  const plotW = Math.max(40, frame.width - frame.left - frame.right);
  const rank = (node: FlowNode) => (node.italic ? 1 : 0);
  const barH = 4;
  const sources = paired.filter((item) => item.node.role === "source").sort((a, b) => rank(a.node) - rank(b.node) || b.node.amountMinor - a.node.amountMinor);
  const hubItem = paired.find((item) => item.node.role === "pool");
  const leaves = paired.filter((item) => item.node.role !== "source" && item.node.role !== "pool").sort((a, b) => rank(a.node) - rank(b.node) || b.node.amountMinor - a.node.amountMinor);
  const placed = new Map<number, { x: number; y: number; w: number; h: number; localX: number; localY: number; below: boolean; stack: number; align: "left" | "right" | "above" | "below" }>();
  const placeBar = (item: (typeof paired)[number], localX: number, localY: number, w: number, align: "left" | "right" | "above" | "below", stack = 0) => {
    item.el.setShape?.({ x: localX, y: localY, width: w, height: barH });
    item.el.dirty?.();
    placed.set(item.index, { x: item.originX + localX, y: item.originY + localY, w, h: barH, localX, localY, below: align !== "above", stack, align });
  };
  const labelLane = 156;
  const flowW = Math.max(80, plotW - labelLane);
  const gap = 4;
  const hubAmount = Math.max(1, hubItem?.node.amountMinor || 0);
  const scale = fitValueScale([sources.map((item) => item.node.amountMinor), leaves.map((item) => item.node.amountMinor), [hubAmount]], gap, flowW);
  const hubW = hubAmount * scale;
  const placeRow = (items: typeof sources, y: number, align: "above" | "below") => {
    const slices = flowSlices(items.map((item) => item.node.amountMinor), hubW);
    const gaps = gap * Math.max(0, items.length - 1);
    let x = Math.max(0, (flowW - (hubW + gaps)) / 2);
    items.forEach((item, index) => {
      const w = slices[index]?.extent || 0;
      placeBar(item, x, y, w, align, 0);
      x += w + gap;
    });
  };
  placeRow(sources, 2, "above");
  const hubX = Math.max(0, (flowW - hubW) / 2);
  const hubY = Math.round(frame.plot * 0.42);
  if (hubItem) placeBar(hubItem, hubX, hubY, hubW, "right");
  placeRow(leaves, frame.plot - barH - 4, "below");
  const idIndex = new Map(nodes.map((node, index) => [node.id, index]));
  stackFlowCurves(curves, links, placed, idIndex, "x");
  zr.refresh();
  const muted = usedColor("var(--text-tertiary)", "rgb(138, 129, 117)");
  const liveTexts = (zr.storage.getDisplayList?.(true) || []).flatMap((el) => {
    const label = (el.style?.text || "").trim();
    const box = label ? measure(el) : null;
    return box ? [{ el, text: label.replace(/\n/g, " "), ...box }] : [];
  });
  const ownerIndex = (el: AuditEl) => {
    const seen = new Set<AuditEl>();
    let current: AuditEl | undefined = el;
    while (current && !seen.has(current)) {
      seen.add(current);
      const hit = paired.find((item) => item.el === current);
      if (hit) return hit.index;
      const host = (current as AuditEl & { __hostTarget?: AuditEl }).__hostTarget;
      if (host) {
        const hostHit = paired.find((item) => item.el === host);
        if (hostHit) return hostHit.index;
      }
      current = current.parent;
    }
    return -1;
  };
  const groups = new Map<number, typeof liveTexts>();
  for (const text of liveTexts) {
    const index = ownerIndex(text.el);
    if (index < 0) continue;
    const group = groups.get(index) || [];
    group.push(text);
    groups.set(index, group);
  }
  const blocks: Array<{ align: "left" | "right" | "above" | "below"; preferX: number; w: number; h: number; y: number; lines: Array<{ el: AuditEl; x: number; y: number; w: number; h: number }> }> = [];
  for (const item of paired) {
    const box = placed.get(item.index);
    const lines = (groups.get(item.index) || []).sort((a, b) => a.y - b.y);
    if (!box || !lines.length) continue;
    const blockH = lines.length * frame.lineHeight;
    const stack = box.stack || 0;
    const band = blockH + 6;
    let cursor = box.align === "right"
      ? box.y + box.h / 2 - blockH / 2
      : box.align === "below"
        ? box.y + box.h + 8 + stack * band
        : box.y - 8 - blockH - stack * band;
    cursor = Math.max(2, Math.min(cursor, frame.height - blockH - 2));
    const paintedLines: Array<{ el: AuditEl; x: number; y: number; w: number; h: number }> = [];
    let minX = Number.POSITIVE_INFINITY;
    let maxX = 0;
    for (const line of lines) {
      const now = measure(line.el);
      if (!now) continue;
      const amount = line.text.includes("·");
      line.el.style = { ...line.el.style, fill: amount ? muted : String(item.el.style?.fill || muted), fontSize: 13, fontWeight: amount ? 400 : 600, fontStyle: item.node.italic ? "italic" : "normal" };
      line.el.dirty?.();
      zr.refresh();
      const painted = measure(line.el) || now;
      const x = box.align === "right"
        ? Math.min(box.x + box.w + 8, frame.width - painted.w - 2)
        : Math.max(2, Math.min(box.x + box.w / 2 - painted.w / 2, frame.width - painted.w - 2));
      line.el.x = (line.el.x || 0) + (x - painted.x);
      line.el.y = (line.el.y || 0) + (cursor - painted.y);
      line.el.dirty?.();
      paintedLines.push({ el: line.el, x, y: cursor, w: painted.w, h: frame.lineHeight });
      minX = Math.min(minX, x);
      maxX = Math.max(maxX, x + painted.w);
      cursor += frame.lineHeight;
    }
    if (!paintedLines.length) continue;
    blocks.push({ align: box.align, preferX: minX, w: Math.max(1, maxX - minX), h: blockH, y: paintedLines[0].y, lines: paintedLines });
  }
  const hits = (a: { x: number; y: number; w: number; h: number }, b: { x: number; y: number; w: number; h: number }) =>
    a.x < b.x + b.w + 8 && a.x + a.w + 8 > b.x && a.y < b.y + b.h + 8 && a.y + a.h + 8 > b.y;
  const pack = (align: "above" | "below") => {
    const group = blocks.filter((block) => block.align === align).sort((a, b) => a.preferX - b.preferX);
    const taken: Array<{ x: number; y: number; w: number; h: number }> = blocks.filter((block) => block.align === "right").map((block) => ({ x: block.preferX, y: block.y, w: block.w, h: block.h }));
    for (const block of group) {
      const dir = align === "below" ? 1 : -1;
      let y = block.y;
      let x = Math.max(2, Math.min(block.preferX, frame.width - block.w - 2));
      let placed = false;
      for (let row = 0; row < 8 && !placed; row += 1) {
        const rowY = block.y + dir * row * (block.h + 8);
        if (rowY < 2 || rowY + block.h > frame.height - 2) continue;
        if (!taken.some((other) => hits({ x, y: rowY, w: block.w, h: block.h }, other))) {
          y = rowY;
          placed = true;
        }
      }
      if (!placed) {
        x = Math.max(2, frame.width - block.w - 2);
        y = align === "below" ? Math.max(2, frame.height - block.h - 2) : 2;
        for (let row = 0; row < 10 && !placed; row += 1) {
          const rowY = align === "below" ? y - row * (block.h + 8) : y + row * (block.h + 8);
          if (rowY < 2 || rowY + block.h > frame.height - 2) continue;
          if (!taken.some((other) => hits({ x, y: rowY, w: block.w, h: block.h }, other))) {
            y = rowY;
            placed = true;
          }
        }
      }
      const dx = x - block.preferX;
      const dy = y - block.lines[0].y;
      for (const line of block.lines) {
        line.el.x = (line.el.x || 0) + dx;
        line.el.y = (line.el.y || 0) + dy;
        line.el.dirty?.();
      }
      taken.push({ x, y, w: block.w, h: block.h });
    }
  };
  pack("below");
  pack("above");
  zr.refresh();
}

/** Wide portrait only. Thin the ribbons to a width-based thickness and give each leaf its own row. */
function spreadPortrait(instance: ECharts, frame: SankeyFrame, nodes: FlowNode[], links: FlowLink[]) {
  const zr = instance.getZr() as unknown as { storage: { getDisplayList?: (all?: boolean) => AuditEl[] }; refresh: () => void };
  const list = zr.storage.getDisplayList?.(true) || [];
  const rects = list.filter((el) => el.type === "rect" && el.shape && (el.shape.width || 0) >= 2 && (el.shape.width || 0) <= 24 && (el.shape.height || 0) > 3);
  const curves = list.filter((el) => el.shape && el.shape.x1 != null && el.shape.y1 != null);
  if (rects.length !== nodes.length) return;
  const texts = list.flatMap((el) => {
    const label = (el.style?.text || "").trim();
    const box = label ? measure(el) : null;
    return box ? [{ el, text: label.replace(/\n/g, " "), ...box }] : [];
  });
  const paired = rects.map((el, index) => ({
    el,
    index,
    node: nodes[index],
    originX: el.parent?.x || 0,
    originY: el.parent?.y || 0,
    old: { x: el.shape?.x || 0, y: el.shape?.y || 0, w: el.shape?.width || 0, h: el.shape?.height || 0 },
  }));
  const columns: Array<typeof paired> = [];
  for (const item of [...paired].sort((a, b) => a.old.x - b.old.x)) {
    const column = columns.find((group) => Math.abs(group[0].old.x - item.old.x) < 6);
    if (column) column.push(item);
    else columns.push([item]);
  }
  for (const column of columns) column.sort((a, b) => a.old.y - b.old.y);
  const maxSum = Math.max(1, ...columns.map((column) => column.reduce((sum, item) => sum + item.old.h, 0)));
  const placed = new Map<number, { x: number; y: number; w: number; h: number; localY: number }>();
  for (const column of columns) {
    const sum = column.reduce((total, item) => total + item.old.h, 0);
    const boxes = spreadColumn(column.map((item) => item.old.h), frame.seriesTop, frame.plot, frame.flowPx * (sum / maxSum));
    column.forEach((item, index) => {
      const next = boxes[index];
      const localY = next.y - item.originY;
      item.el.setShape?.({ y: localY, height: next.h });
      item.el.dirty?.();
      placed.set(item.index, { x: item.originX + item.old.x, y: item.originY + localY, w: item.old.w, h: next.h, localY });
    });
  }
  const idIndex = new Map(nodes.map((node, index) => [node.id, index]));
  curves.forEach((curve, index) => {
    const link = links[index];
    const shape = curve.shape;
    if (!link || !shape || shape.y1 == null || shape.y2 == null) return;
    const source = idIndex.get(link.source);
    const target = idIndex.get(link.target);
    if (source == null || target == null) return;
    const src = placed.get(source);
    const dst = placed.get(target);
    const oldSrc = paired[source];
    const oldDst = paired[target];
    if (!src || !dst || !oldSrc || !oldDst) return;
    const sy = oldSrc.old.h > 0 ? (shape.y1 - oldSrc.old.y) / oldSrc.old.h : 0;
    const ty = oldDst.old.h > 0 ? (shape.y2 - oldDst.old.y) / oldDst.old.h : 0;
    const y1 = src.localY + Math.min(1, Math.max(0, sy)) * src.h;
    const y2 = dst.localY + Math.min(1, Math.max(0, ty)) * dst.h;
    const x1 = shape.x1 || 0;
    const x2 = shape.x2 || 0;
    curve.setShape?.({
      y1,
      y2,
      cpy1: y1,
      cpy2: y2,
      cpx1: x1 * 0.55 + x2 * 0.45,
      cpx2: x1 * 0.45 + x2 * 0.55,
      extent: Math.max(1, (shape.extent || 1) * (src.h / Math.max(1, oldSrc.old.h))),
    });
    curve.dirty?.();
  });
  const groups = new Map<number, typeof texts>();
  const ownerIndex = (el: AuditEl) => {
    const seen = new Set<AuditEl>();
    let current: AuditEl | undefined = el;
    while (current && !seen.has(current)) {
      seen.add(current);
      const hit = paired.find((item) => item.el === current);
      if (hit) return hit.index;
      const host = (current as AuditEl & { __hostTarget?: AuditEl }).__hostTarget;
      if (host) {
        const hostHit = paired.find((item) => item.el === host);
        if (hostHit) return hostHit.index;
      }
      current = current.parent;
    }
    return -1;
  };
  for (const text of texts) {
    const index = ownerIndex(text.el);
    if (index < 0) continue;
    const group = groups.get(index) || [];
    group.push(text);
    groups.set(index, group);
  }
  zr.refresh();
  let margin = 4;
  const roleOf = new Map<number, string>();
  const nearTopOf = new Map<number, boolean>();
  const members = new Map<number, Array<{ el: AuditEl; x: number; y: number; w: number; h: number }>>();
  const sourceNodes = paired.filter((item) => item.node.role === "source");
  const gutterEdge = sourceNodes.length
    ? Math.min(...sourceNodes.map((item) => placed.get(item.index)?.x ?? frame.left))
    : Math.min(...[...placed.values()].map((box) => box.x));
  const orderedItems = [...paired].sort((a, b) => {
    const rank = (role: string) => (role === "pool" ? 0 : role === "hub" ? 1 : role === "source" ? 2 : 3);
    return rank(a.node.role) - rank(b.node.role) || a.old.y - b.old.y || a.old.x - b.old.x;
  });
  for (const item of orderedItems) {
    const box = placed.get(item.index);
    const lines = (groups.get(item.index) || []).sort((a, b) => a.y - b.y);
    if (!box || !lines.length) continue;
    for (const line of lines) {
      const now = measure(line.el);
      if (!now) continue;
      line.x = now.x;
      line.y = now.y;
      line.w = now.w;
      line.h = now.h;
    }
    const blockH = lines.length * frame.lineHeight;
    const role = item.node.role;
    roleOf.set(item.index, role);
    const nearTop = box.y <= frame.seriesTop + frame.flowPx + 8;
    nearTopOf.set(item.index, nearTop);
    let cursor = box.y + box.h / 2 - blockH / 2;
    if ((role === "pool" || role === "hub") && nearTop) {
      cursor = margin;
      margin += blockH + 8;
    } else if (role === "pool" || role === "hub") {
      cursor = box.y - 8 - blockH;
    }
    if (role === "source" && cursor < margin) cursor = margin;
    cursor = Math.max(2, Math.min(cursor, frame.height - blockH - 2));
    const placedLines: Array<{ el: AuditEl; x: number; y: number; w: number; h: number }> = [];
    const centered = box.y + box.h / 2 - blockH / 2;
    const anchorTop = role === "pool"
      ? box.y - 8 - blockH - 40
      : role === "hub" && item.node.kind !== "spending"
        ? box.y + box.h + 8
        : role === "hub"
          ? box.y - 8 - blockH
          : centered;
    const deltaY = cursor - anchorTop;
    for (const line of lines) {
      const x = role === "sink" || role === "child"
        ? Math.min(frame.width - line.w - 2, box.x + box.w + 8)
        : Math.max(2, (role === "source" ? box.x : gutterEdge) - 8 - line.w);
      line.el.x = (line.el.x || 0) + (x - line.x);
      line.el.y = (line.el.y || 0) + deltaY;
      line.el.dirty?.();
      placedLines.push({ el: line.el, x, y: cursor, w: line.w, h: line.h });
      cursor += frame.lineHeight;
    }
    members.set(item.index, placedLines);
  }
  const blockOf = (index: number) => {
    const lines = members.get(index) || [];
    if (!lines.length) return null;
    const x = Math.min(...lines.map((line) => line.x));
    const y = Math.min(...lines.map((line) => line.y));
    return {
      x,
      y,
      w: Math.max(...lines.map((line) => line.x + line.w)) - x,
      h: Math.max(...lines.map((line) => line.y + line.h)) - y,
    };
  };
  const shift = (index: number, dy: number) => {
    if (!dy) return;
    for (const line of members.get(index) || []) {
      line.el.y = (line.el.y || 0) + dy;
      line.el.dirty?.();
      line.y += dy;
    }
  };
  for (let pass = 0; pass < 10; pass += 1) {
    let moved = false;
    const hubs = [...members.keys()].filter((index) => roleOf.get(index) === "hub" || roleOf.get(index) === "pool");
    const sinks = [...members.keys()].filter((index) => roleOf.get(index) === "sink" || roleOf.get(index) === "child");
    const sources = [...members.keys()].filter((index) => roleOf.get(index) === "source");
    for (const hub of hubs) {
      const hubBox = blockOf(hub);
      if (!hubBox) continue;
      for (const sink of sinks) {
        const sinkBox = blockOf(sink);
        if (!sinkBox || !boxesHit(hubBox, sinkBox)) continue;
        shift(hub, sinkBox.y - 6 - (hubBox.y + hubBox.h));
        moved = true;
      }
      for (const source of sources) {
        const sourceBox = blockOf(source);
        if (!sourceBox || !boxesHit(hubBox, sourceBox)) continue;
        if (nearTopOf.get(hub)) shift(source, hubBox.y + hubBox.h + 6 - sourceBox.y);
        else shift(hub, sourceBox.y - 6 - (hubBox.y + hubBox.h));
        moved = true;
      }
      for (const other of hubs) {
        if (other === hub) continue;
        const otherBox = blockOf(other);
        if (!otherBox || !boxesHit(hubBox, otherBox)) continue;
        const lower = hubBox.y >= otherBox.y ? hub : other;
        const upper = lower === hub ? otherBox : hubBox;
        shift(lower, upper.y + upper.h + 6 - blockOf(lower)!.y);
        moved = true;
      }
    }
    for (const index of members.keys()) {
      const box = blockOf(index);
      if (!box) continue;
      if (box.y < 2) {
        shift(index, 2 - box.y);
        moved = true;
      } else if (box.y + box.h > frame.height - 2) {
        shift(index, frame.height - 2 - (box.y + box.h));
        moved = true;
      }
    }
    if (!moved) break;
  }
  zr.refresh();
}

/** Landscape only. Lift the Total spending label off the red bar without moving the rest. */
function liftSpending(instance: ECharts) {
  const zr = instance.getZr() as unknown as { storage: { getDisplayList?: (all?: boolean) => AuditEl[] }; refresh: () => void };
  const list = zr.storage.getDisplayList?.(true) || [];
  const nodes = list.flatMap((el) => {
    if (el.type !== "rect" || !el.shape || (el.shape.width || 0) > 24 || (el.shape.height || 0) < 4) return [];
    const box = measure(el);
    return box ? [box] : [];
  });
  const spending = list.filter((el) => (el.style?.text || "").includes("Total spending") || (el.style?.text || "").includes("spending"));
  if (!spending.length || !nodes.length) return;
  const boxes = spending.flatMap((el) => {
    const box = measure(el);
    return box ? [{ el, ...box }] : [];
  });
  const top = Math.min(...boxes.map((box) => box.y));
  const bottom = Math.max(...boxes.map((box) => box.y + box.h));
  const left = Math.min(...boxes.map((box) => box.x));
  const right = Math.max(...boxes.map((box) => box.x + box.w));
  let dy = 0;
  for (const node of nodes) {
    const close = left < node.x + node.w + 8 && right > node.x - 8 && top < node.y + node.h + 8 && bottom > node.y - 8;
    if (!close) continue;
    dy = Math.min(dy, node.y - 8 - bottom);
  }
  if (dy < 0 && top + dy >= 2) {
    for (const box of boxes) {
      box.el.y = (box.el.y || 0) + dy;
      box.el.dirty?.();
    }
    zr.refresh();
  }
}

function nudgeLabels(instance: ECharts, width: number, height: number) {
  const items = labelElements(instance);
  const grouped = new Map<AuditEl, typeof items>();
  for (const item of items) {
    const key = item.parent || item.el;
    const group = grouped.get(key) || [];
    group.push(item);
    grouped.set(key, group);
  }
  const clusters = [...grouped.values()].map((group) => {
    const x = Math.min(...group.map((row) => row.x));
    const y = Math.min(...group.map((row) => row.y));
    return {
      x,
      y,
      w: Math.max(...group.map((row) => row.x + row.w)) - x,
      h: Math.max(...group.map((row) => row.y + row.h)) - y,
      items: group,
    };
  });
  const move = (cluster: (typeof clusters)[number], dx: number, dy: number) => {
    for (const item of cluster.items) {
      item.el.x = (item.el.x || 0) + dx;
      item.el.y = (item.el.y || 0) + dy;
      item.x += dx;
      item.y += dy;
      item.el.dirty?.();
    }
    cluster.x += dx;
    cluster.y += dy;
  };
  for (let pass = 0; pass < 16; pass += 1) {
    let moved = false;
    for (let i = 0; i < clusters.length; i += 1) {
      for (let j = i + 1; j < clusters.length; j += 1) {
        if (!boxesHit(clusters[i], clusters[j])) continue;
        const lower = clusters[i].y <= clusters[j].y ? clusters[j] : clusters[i];
        const upper = lower === clusters[i] ? clusters[j] : clusters[i];
        move(lower, 0, upper.y + upper.h + 8 - lower.y);
        moved = true;
      }
      const cluster = clusters[i];
      if (cluster.y + cluster.h > height - 2) {
        move(cluster, 0, height - 2 - (cluster.y + cluster.h));
        moved = true;
      }
      if (cluster.y < 2) {
        move(cluster, 0, 2 - cluster.y);
        moved = true;
      }
      if (cluster.x < 2) {
        move(cluster, 2 - cluster.x, 0);
        moved = true;
      }
      if (cluster.x + cluster.w > width - 2) {
        move(cluster, width - 2 - (cluster.x + cluster.w), 0);
        moved = true;
      }
    }
    if (!moved) break;
  }
  for (let i = 0; i < clusters.length; i += 1) {
    for (let j = i + 1; j < clusters.length; j += 1) {
      if (!boxesHit(clusters[i], clusters[j])) continue;
      const lower = clusters[i].y >= clusters[j].y ? clusters[i] : clusters[j];
      for (const item of lower.items) {
        const text = item.el.style?.text || "";
        const name = text.split("\n")[0];
        if (!name || name === text) continue;
        item.el.style = { ...item.el.style, text: name };
        item.el.dirty?.();
      }
    }
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

function paintColor(
  id: string,
  color: string,
  setColors: (update: (current: Record<string, string>) => Record<string, string>) => void,
  onPaintNode?: (id: string, color: string) => void,
) {
  setColors((current) => {
    const next = { ...current };
    if (!color) {
      if (id === "all") {
        for (const key of Object.keys(next)) delete next[key];
      } else {
        delete next[id];
      }
    } else {
      next[id] = color;
    }
    return next;
  });
  onPaintNode?.(id, color);
}

function colorGroup(node: FlowNode): "income" | "spend" | "save" {
  if (node.kind === "savings" || node.kind === "investment" || node.kind === "drawn") return "save";
  if (node.role === "source" || node.role === "pool" || node.kind === "income" || node.kind === "pool") return "income";
  return "spend";
}


function OptionsIcon() {
  return (
    <svg width="22" height="22" viewBox="0 0 22 22" aria-hidden="true">
      <path d="M4 6h14M4 11h14M4 16h14" fill="none" stroke="currentColor" strokeWidth="1.5" />
      <circle cx="8" cy="6" r="2" fill="var(--surface-0)" stroke="currentColor" strokeWidth="1.5" />
      <circle cx="14" cy="11" r="2" fill="var(--surface-0)" stroke="currentColor" strokeWidth="1.5" />
      <circle cx="9" cy="16" r="2" fill="var(--surface-0)" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  );
}

function PaletteIcon() {
  return (
    <svg width="22" height="22" viewBox="0 0 22 22" aria-hidden="true">
      <path d="M11 3.5a7.5 7.5 0 1 0 0 15h1.2a1.8 1.8 0 0 0 0-3.6h-.4a1.4 1.4 0 0 1 0-2.8H14a4.5 4.5 0 0 0 4.5-4.5A4.1 4.1 0 0 0 11 3.5Z" fill="none" stroke="currentColor" strokeWidth="1.5" />
      <circle cx="7.5" cy="9" r="1" fill="currentColor" />
      <circle cx="11" cy="7" r="1" fill="currentColor" />
      <circle cx="14.5" cy="9" r="1" fill="currentColor" />
    </svg>
  );
}

function ExpandIcon() {
  return (
    <svg width="22" height="22" viewBox="0 0 22 22" aria-hidden="true">
      <path d="M8 4H4v4M14 4h4v4M8 18H4v-4M14 18h4v-4" fill="none" stroke="currentColor" strokeWidth="1.5" />
    </svg>
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
