import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, MoreHorizontal } from "lucide-react";
import { Sheet } from "@/components/Sheet";
import { api } from "@/lib/api";
import type { FinanceState } from "@/hooks/useFinance";
import type { FinanceBudget, FinanceBudgetLine } from "@/lib/api";
import { overBudget, resolveBudget, shiftMonth, summarizeBudget, type BudgetRecord, type BudgetRole } from "@/lib/budget";
import { bookFromStatus, plannedHits } from "@/lib/financeMath";
import { formatMoney, minorExponent, moneyParts } from "@/lib/money";
import { activeLocale, fill, useT } from "@/lib/i18n";
import { ChartColorMenu } from "@/components/ColorSheet";
import { resolveColor } from "@/lib/entityColor";
import { todayIso } from "@/components/FinanceDashboard";

const ROLES: BudgetRole[] = ["income", "expense", "savings", "investment"];

function asRecord(row: FinanceBudget): BudgetRecord {
  return {
    id: row.id,
    period: row.period,
    currency: row.currency,
    lines: row.lines || [],
    copyForward: row.copyForward !== false,
    notify: Boolean(row.notify),
    warned: row.warned || [],
    updatedMs: row.updatedMs || 0,
  };
}

function BudgetLineRow({
  name,
  actual,
  planned,
  ratio,
  warn,
  over,
  onOpen,
  onSwipe,
  color = "",
}: {
  name: string;
  actual: string;
  planned: string;
  ratio: number;
  warn: boolean;
  over: boolean;
  onOpen: () => void;
  onSwipe: () => void;
  color?: string;
}) {
  const drag = useRef<{ x: number; y: number } | null>(null);
  return (
    <div
      className="budget-line"
      onPointerDown={(event) => {
        drag.current = { x: event.clientX, y: event.clientY };
      }}
      onPointerUp={(event) => {
        const origin = drag.current;
        drag.current = null;
        if (!origin) return;
        const dx = event.clientX - origin.x;
        const dy = event.clientY - origin.y;
        if (dx < -48 && Math.abs(dx) > Math.abs(dy)) onSwipe();
      }}
    >
      <button type="button" className="flex min-h-11 w-full items-baseline justify-between gap-3 text-left" onClick={onOpen}>
        <span className="flex min-w-0 items-center gap-2 truncate text-[15px]">{color ? <span className="fin-swatch" style={{ background: color }} aria-hidden="true" /> : null}{name}</span>
        <span className="fin-num shrink-0 text-[13px]">{actual} / {planned}</span>
      </button>
      <div className="budget-bar" data-warn={warn ? "true" : "false"} data-over={over ? "true" : "false"}>
        <span style={{ width: `${Math.min(100, Math.round(ratio * 100))}%`, background: color || undefined }} />
      </div>
    </div>
  );
}

function cnLeft(left: number): string {
  return `fin-hero-num mt-1 ${left < 0 ? "text-[var(--negative)]" : ""}`;
}

function MonthSwitch({ month, label, onChange }: { month: string; label: string; onChange: (month: string) => void }) {
  const t = useT();
  const start = useRef<{ x: number; y: number } | null>(null);
  return (
    <div
      className="plan-month"
      data-swipe-ignore=""
      onPointerDown={(event) => {
        start.current = { x: event.clientX, y: event.clientY };
      }}
      onPointerUp={(event) => {
        if (!start.current) return;
        const dx = event.clientX - start.current.x;
        const dy = event.clientY - start.current.y;
        start.current = null;
        if (Math.abs(dx) < 48 || Math.abs(dx) < Math.abs(dy)) return;
        onChange(shiftMonth(month, dx < 0 ? 1 : -1));
      }}
    >
      <button type="button" className="plan-chevron" aria-label={t("Previous month")} onClick={() => onChange(shiftMonth(month, -1))}>
        <ChevronLeft size={18} strokeWidth={1.5} aria-hidden="true" />
      </button>
      <p className="min-w-0 flex-1 text-center text-[16px]">{label}</p>
      <button type="button" className="plan-chevron" aria-label={t("Next month")} onClick={() => onChange(shiftMonth(month, 1))}>
        <ChevronRight size={18} strokeWidth={1.5} aria-hidden="true" />
      </button>
    </div>
  );
}

function toMinor(text: string, currency: string): number {
  const normalized = text.trim().replace(/\s/g, "").replace(",", ".");
  const value = Number(normalized);
  if (!Number.isFinite(value) || value <= 0) return 0;
  return Math.round(value * 10 ** minorExponent(currency));
}

export function BudgetSection({ finance }: { finance: FinanceState }) {
  const t = useT();
  const display = (finance.snapshot.defaultCurrency || "EUR").toUpperCase();
  const [month, setMonth] = useState(finance.snapshot.period || todayIso().slice(0, 7));
  const [open, setOpen] = useState(false);
  const [role, setRole] = useState<BudgetRole>("expense");
  const [category, setCategory] = useState("");
  const [subcategory, setSubcategory] = useState("");
  const [label, setLabel] = useState("");
  const [amount, setAmount] = useState("");
  const [pendingDelete, setPendingDelete] = useState("");
  const [more, setMore] = useState(false);
  const [editing, setEditing] = useState("");
  const budgets = useMemo(() => (finance.snapshot.budgets || []).map(asRecord), [finance.snapshot.budgets]);
  const resolved = useMemo(() => resolveBudget(budgets, month), [budgets, month]);
  const book = useMemo(() => bookFromStatus(finance.snapshot.fx), [finance.snapshot.fx]);
  const nowMs = useMemo(() => Date.now(), [finance.snapshot]);
  const today = todayIso();
  const scheduled = useMemo(() => plannedHits(finance.snapshot.rules, finance.snapshot.transactions, today), [finance.snapshot.rules, finance.snapshot.transactions, today]);
  const kinds = useMemo(() => Object.fromEntries(finance.snapshot.categories.map((row) => [row.name.trim().toLowerCase(), row.kind || ""])), [finance.snapshot.categories]);
  const summary = useMemo(
    () =>
      summarizeBudget(resolved.budget, finance.snapshot.transactions, scheduled, {
        month,
        display,
        book,
        nowMs,
        today,
        inherited: resolved.inherited,
        categoryKinds: kinds,
      }),
    [resolved, finance.snapshot.transactions, scheduled, month, display, book, nowMs, today, kinds],
  );
  const roleLabel = (value: string) => (value === "income" ? t("Expected income") : value === "savings" ? t("Planned savings") : value === "investment" ? t("Planned investments") : t("Expense limit"));

  useEffect(() => {
    if (!resolved.budget?.notify || summary.warnings.length === 0) return;
    const seen = new Set(resolved.budget.warned || []);
    const fresh = summary.warnings.filter((row) => !seen.has(`${month}:${row.name}:${row.over ? "100" : "80"}`));
    if (fresh.length === 0) return;
    const keys = fresh.map((row) => `${month}:${row.name}:${row.over ? "100" : "80"}`);
    for (const row of fresh) {
      const body = row.over
        ? fill("{name} is over the plan by {amount}", { name: row.name, amount: formatMoney(row.amountMinor, display) })
        : fill("{name} is at {percent}% of the plan", { name: row.name, percent: Math.round(row.ratio * 100) });
      if (finance.inApp) void api.notifyBudget(t("Budget"), body).catch(() => undefined);
    }
    saveLines(resolved.budget.lines, { notify: true, warned: [...seen, ...keys] });
  }, [summary.warnings, resolved.budget, month, display, finance.inApp, t]);

  useEffect(() => {
    const onVoice = (event: Event) => {
      const detail = (event as CustomEvent<{ view?: string; period?: string }>).detail || {};
      if (detail.view === "budget") setMonth(finance.snapshot.period || todayIso().slice(0, 7));
      if (detail.period && /^\d{4}-\d{2}$/.test(detail.period)) setMonth(detail.period);
    };
    window.addEventListener("grok-finance-voice", onVoice);
    return () => window.removeEventListener("grok-finance-voice", onVoice);
  }, [finance.snapshot.period]);

  const saveLines = (lines: FinanceBudgetLine[], extra: Record<string, unknown> = {}) => {
    finance.saveBudget({
      action: "set",
      period: month,
      currency: display,
      replaceLines: true,
      copyForward: resolved.budget?.copyForward !== false,
      notify: resolved.budget?.notify || false,
      lines,
      ...extra,
    });
  };

  const addLine = () => {
    const amountMinor = toMinor(amount, display);
    if (!category.trim() || amountMinor <= 0) return;
    const lines = [...(resolved.budget?.lines || [])];
    const next = { id: editing, role, category: category.trim(), subcategory: subcategory.trim(), label: label.trim(), amountMinor, currency: display };
    const index = lines.findIndex((line) => line.id === editing);
    if (editing && index >= 0) lines[index] = next;
    else lines.push({ ...next, id: "" });
    saveLines(lines);
    setCategory("");
    setSubcategory("");
    setLabel("");
    setAmount("");
    setEditing("");
    setPendingDelete("");
    setOpen(false);
  };

  const removeLine = (id: string) => {
    saveLines((resolved.budget?.lines || []).filter((line) => line.id !== id));
    setPendingDelete("");
    setEditing("");
    setOpen(false);
  };

  const locale = activeLocale() === "ca-ES-valencia" ? "ca-ES" : activeLocale();
  const monthLabel = new Date(`${month}-01T12:00:00`).toLocaleDateString(locale, { month: "long", year: "numeric" });
  const left = summary.spendingPlanned - summary.spendingActual;
  const spentRatio = summary.spendingPlanned > 0 ? summary.spendingActual / summary.spendingPlanned : 0;
  const leftParts = moneyParts(left, display);

  return (
    <section className="fin-panel" data-finance-panel="budget" data-shot="budget">
      <MonthSwitch month={month} label={monthLabel} onChange={setMonth} />
      {resolved.inherited ? <p className="mt-2 text-[13px] leading-5">{t("From the template")}</p> : null}
      {!resolved.budget ? (
        <button type="button" className="plan-empty mt-4 w-full text-left" onClick={() => finance.saveBudget({ action: "create", period: month, currency: display, copyForward: true })}>
          <span className="plan-ghost" aria-hidden="true">
            <i style={{ width: "72%" }} />
            <i style={{ width: "48%" }} />
            <i style={{ width: "30%" }} />
          </span>
          <span className="mt-3 inline-flex min-h-11 items-center bg-[var(--text-primary)] px-3 text-[15px] text-[var(--surface-0)]">{fill("Plan {month}", { month: monthLabel })}</span>
        </button>
      ) : (
        <>
          <p className="fin-muted mt-4 text-[13px]">{t("Left to spend")}</p>
          <p className={cnLeft(left)}>
            <span className="fin-hero-unit">{leftParts.unit}</span>
            {leftParts.figure}
          </p>
          <div className="budget-bar mt-3" data-over={overBudget(summary) ? "true" : "false"}>
            <span style={{ width: `${Math.min(100, Math.round(spentRatio * 100))}%` }} />
          </div>
          <div className="mt-3 flex justify-end">
            <button type="button" className="plan-chevron" aria-label={t("More")} aria-expanded={more} onClick={() => setMore((value) => !value)}>
              <MoreHorizontal size={18} strokeWidth={1.5} aria-hidden="true" />
            </button>
          </div>
          {more ? (
            <div className="flex flex-col" data-swipe-ignore="">
              <button type="button" className="min-h-11 text-left text-[15px] underline" onClick={() => { setEditing(""); setOpen(true); setMore(false); }}>{t("Add a line")}</button>
              <button type="button" className="min-h-11 text-left text-[15px] underline" onClick={() => { finance.saveBudget({ action: "set", period: shiftMonth(month, 1), currency: display, replaceLines: true, copyForward: true, lines: resolved.budget?.lines || [], notify: resolved.budget?.notify || false }); setMore(false); }}>{t("Copy to next month")}</button>
              <button type="button" className="min-h-11 text-left text-[15px] underline" onClick={() => { finance.saveBudget({ action: "set", period: "", currency: display, replaceLines: true, copyForward: true, lines: resolved.budget?.lines || [], notify: resolved.budget?.notify || false }); setMore(false); }}>{t("Use as template")}</button>
              <button type="button" className="min-h-11 text-left text-[15px] underline" aria-pressed={resolved.budget.notify} onClick={() => { saveLines(resolved.budget?.lines || [], { notify: !resolved.budget?.notify }); setMore(false); }}>{t("Notify at 80% and 100%")}</button>
            </div>
          ) : null}
          <div className="mt-1">
            {summary.rows.map((row) => {
              const category = finance.snapshot.categories.find((item) => item.name === row.name);
              const color = resolveColor(finance.snapshot.entityColors, category?.kind === "income" ? "income" : "category", row.name, category?.color || "");
              return (
              <BudgetLineRow
                key={row.id || row.name}
                name={row.name}
                color={color}
                actual={formatMoney(row.actualMinor, display)}
                planned={formatMoney(row.plannedMinor, display)}
                ratio={Number.isFinite(row.ratio) ? row.ratio : 1}
                warn={row.warn}
                over={row.over}
                onOpen={() => {
                  const line = resolved.budget?.lines.find((item) => item.id === row.id);
                  setEditing(row.id);
                  setRole((line?.role as BudgetRole) || "expense");
                  setCategory(line?.category || row.name);
                  setSubcategory(line?.subcategory || "");
                  setLabel(line?.label || "");
                  setAmount(line ? String(line.amountMinor / 10 ** minorExponent(display)) : "");
                  setPendingDelete("");
                  setOpen(true);
                }}
                onSwipe={() => {
                  const line = resolved.budget?.lines.find((item) => item.id === row.id);
                  setEditing(row.id);
                  setRole((line?.role as BudgetRole) || "expense");
                  setCategory(line?.category || row.name);
                  setSubcategory(line?.subcategory || "");
                  setLabel(line?.label || "");
                  setAmount(line ? String(line.amountMinor / 10 ** minorExponent(display)) : "");
                  setPendingDelete(row.id);
                  setOpen(true);
                }}
              />
              );
            })}
            <ChartColorMenu
              series={summary.rows.map((row) => {
                const category = finance.snapshot.categories.find((item) => item.name === row.name);
                return { id: row.name, label: row.name, color: resolveColor(finance.snapshot.entityColors, category?.kind === "income" ? "income" : "category", row.name, category?.color || "") };
              })}
              currency={display}
              onPaint={(id, color) => {
                const category = finance.snapshot.categories.find((item) => item.name === id);
                finance.paintColor(category?.kind === "income" ? "income" : "category", id, color);
              }}
            />
          </div>
        </>
      )}
      <Sheet open={open} title={editing ? t("Edit line") : t("Add a line")} onClose={() => { setOpen(false); setPendingDelete(""); setEditing(""); }} className="finance">
        <form
          className="space-y-3"
          onSubmit={(event) => {
            event.preventDefault();
            addLine();
          }}
        >
          <div className="flex flex-wrap gap-2" role="group" aria-label={t("Kind")}>
            {ROLES.map((item) => (
              <button key={item} type="button" className="fin-chip" aria-pressed={role === item} onClick={() => setRole(item)}>
                {roleLabel(item)}
              </button>
            ))}
          </div>
          <label className="block text-[13px]">
            {t("Category")}
            <input className="mt-1 block h-11 w-full border-0 border-b border-[var(--line)] bg-transparent text-[16px]" value={category} onChange={(event) => setCategory(event.target.value)} />
          </label>
          <label className="block text-[13px]">
            {t("Subcategory")}
            <input className="mt-1 block h-11 w-full border-0 border-b border-[var(--line)] bg-transparent text-[16px]" value={subcategory} onChange={(event) => setSubcategory(event.target.value)} />
          </label>
          <label className="block text-[13px]">
            {t("Entry name")}
            <input className="mt-1 block h-11 w-full border-0 border-b border-[var(--line)] bg-transparent text-[16px]" placeholder={t("Optional, such as Salary 1 Nov")} value={label} onChange={(event) => setLabel(event.target.value)} />
          </label>
          <label className="block text-[13px]">
            {t("Amount")}
            <input className="mt-1 block h-11 w-full border-0 border-b border-[var(--line)] bg-transparent text-[16px]" inputMode="decimal" value={amount} onChange={(event) => setAmount(event.target.value)} />
          </label>
          <button type="submit" className="min-h-11 bg-[var(--text-primary)] px-3 text-[15px] text-[var(--surface-0)]">
            {t("Save")}
          </button>
          {editing ? (
            pendingDelete === editing ? (
              <div className="flex flex-wrap items-center gap-3">
                <p className="text-[13px]">{t("Delete this line?")}</p>
                <button type="button" className="min-h-11 bg-[var(--negative)] px-3 text-[15px] text-[var(--surface-0)]" onClick={() => removeLine(editing)}>
                  {t("Yes, delete")}
                </button>
                <button type="button" className="min-h-11 px-3 text-[15px] underline" onClick={() => setPendingDelete("")}>
                  {t("Cancel")}
                </button>
              </div>
            ) : (
              <button type="button" className="min-h-11 px-3 text-[15px] text-[var(--negative)]" onClick={() => setPendingDelete(editing)}>
                {t("Delete")}
              </button>
            )
          ) : null}
        </form>
      </Sheet>
    </section>
  );
}
