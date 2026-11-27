import { useEffect, useMemo, useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { Sheet } from "@/components/Sheet";
import { api } from "@/lib/api";
import type { FinanceState } from "@/hooks/useFinance";
import type { FinanceBudget, FinanceBudgetLine } from "@/lib/api";
import { clampMonth, overBudget, resolveBudget, shiftMonth, suggestBudgetLines, summarizeBudget, type BudgetRecord, type BudgetRole } from "@/lib/budget";
import { bookFromStatus } from "@/lib/financeMath";
import { ruleAmount } from "@/lib/financeView";
import { formatMoney, minorExponent, moneyParts } from "@/lib/money";
import { activeLocale, fill, useT } from "@/lib/i18n";
import { ChartColorMenu } from "@/components/ColorSheet";
import { resolveColor } from "@/lib/entityColor";
import { todayIso } from "@/components/FinanceDashboard";

const ROLES: BudgetRole[] = ["income", "expense", "savings"];

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
  color = "",
  overLabel,
}: {
  name: string;
  actual: string;
  planned: string;
  ratio: number;
  warn: boolean;
  over: boolean;
  onOpen: () => void;
  color?: string;
  overLabel: string;
}) {
  return (
    <div className="budget-line">
      <button type="button" className="flex min-h-11 w-full items-baseline justify-between gap-3 text-left" onClick={onOpen}>
        <span className="min-w-0">
          <span className="flex min-w-0 items-center gap-2 truncate text-[15px]">
            {color ? <span className="fin-swatch" style={{ background: color }} aria-hidden="true" /> : null}
            {name}
          </span>
          {over ? <span className="block text-[13px] text-[var(--negative)]">{overLabel}</span> : null}
        </span>
        <span className="fin-num shrink-0 text-[13px]">{actual} / {planned}</span>
      </button>
      <div className="budget-bar" data-warn={warn ? "true" : "false"} data-over={over ? "true" : "false"}>
        <span style={{ width: `${Math.min(100, Math.round((Number.isFinite(ratio) ? ratio : 1) * 100))}%`, background: over ? "var(--negative)" : color || undefined }} />
      </div>
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
  const todayMonth = todayIso().slice(0, 7);
  const minMonth = shiftMonth(todayMonth, -24);
  const maxMonth = shiftMonth(todayMonth, 12);
  const [month, setMonth] = useState(() => clampMonth(finance.snapshot.period || todayMonth, minMonth, maxMonth));
  const [open, setOpen] = useState(false);
  const [setup, setSetup] = useState(false);
  const [about, setAbout] = useState(false);
  const [role, setRole] = useState<BudgetRole>("expense");
  const [category, setCategory] = useState("");
  const [subcategory, setSubcategory] = useState("");
  const [label, setLabel] = useState("");
  const [amount, setAmount] = useState("");
  const [pendingDelete, setPendingDelete] = useState("");
  const [editing, setEditing] = useState("");
  const [setupNote, setSetupNote] = useState("");
  const budgets = useMemo(() => (finance.snapshot.budgets || []).map(asRecord), [finance.snapshot.budgets]);
  const resolved = useMemo(() => resolveBudget(budgets, month), [budgets, month]);
  const book = useMemo(() => bookFromStatus(finance.snapshot.fx), [finance.snapshot.fx]);
  const nowMs = useMemo(() => Date.now(), [finance.snapshot]);
  const today = todayIso();
  const kinds = useMemo(() => Object.fromEntries(finance.snapshot.categories.map((row) => [row.name.trim().toLowerCase(), row.kind || ""])), [finance.snapshot.categories]);
  const summary = useMemo(
    () =>
      summarizeBudget(resolved.budget, finance.snapshot.transactions, [], {
        month,
        display,
        book,
        nowMs,
        today,
        inherited: resolved.inherited,
        categoryKinds: kinds,
      }),
    [resolved, finance.snapshot.transactions, month, display, book, nowMs, today, kinds],
  );
  const roleLabel = (value: string) => (value === "income" ? t("Expected income") : value === "savings" ? t("Savings target") : t("Expense limit"));
  const goMonth = (by: number) => setMonth((current) => clampMonth(shiftMonth(current, by), minMonth, maxMonth));

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
      if (detail.view === "budget" || detail.view === "plan") setMonth(clampMonth(finance.snapshot.period || todayMonth, minMonth, maxMonth));
      if (detail.period && /^\d{4}-\d{2}$/.test(detail.period)) setMonth(clampMonth(detail.period, minMonth, maxMonth));
    };
    window.addEventListener("grok-finance-voice", onVoice);
    return () => window.removeEventListener("grok-finance-voice", onVoice);
  }, [finance.snapshot.period, todayMonth, minMonth, maxMonth]);

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

  const prefill = (source: "rules" | "average" | "both") => {
    const lines = suggestBudgetLines({
      source,
      month,
      display,
      book,
      nowMs,
      rules: finance.snapshot.rules.map((rule) => {
        const amount = ruleAmount(rule, finance.snapshot.accounts);
        return {
          name: rule.name,
          amountMinor: amount.amountMinor,
          currency: amount.currency || rule.currency || display,
          direction: rule.direction,
          category: rule.category,
          cadence: rule.cadence,
          purpose: rule.purpose,
          status: rule.status,
          intervalDays: rule.intervalDays,
        };
      }),
      transactions: finance.snapshot.transactions,
    });
    if (lines.length === 0) {
      setSetupNote(t("Nothing to copy yet. Add a line yourself."));
      return;
    }
    saveLines(lines.map((line) => ({ ...line, id: "" })));
    setSetupNote("");
    setSetup(false);
  };

  const locale = activeLocale() === "ca-ES-valencia" ? "ca-ES" : activeLocale();
  const monthLabel = new Date(`${month}-01T12:00:00`).toLocaleDateString(locale, { month: "long", year: "numeric" });
  const lines = resolved.budget?.lines || [];
  const ready = lines.length > 0;
  const incomeBase = summary.incomePlanned > 0 ? summary.incomePlanned : summary.incomeActual;
  const left = incomeBase > 0 ? incomeBase - summary.spendingActual - summary.savingsPlanned : summary.spendingPlanned - summary.spendingActual;
  const leftParts = moneyParts(left, display);
  const incomeRows = summary.rows.filter((row) => row.role === "income");
  const expenseRows = summary.rows.filter((row) => row.role === "expense");
  const savingRows = summary.rows.filter((row) => row.role === "savings");
  const openEditor = (id = "") => {
    const line = resolved.budget?.lines.find((item) => item.id === id);
    setEditing(id);
    setRole((line?.role as BudgetRole) || "expense");
    setCategory(line?.category || "");
    setSubcategory(line?.subcategory || "");
    setLabel(line?.label || "");
    setAmount(line ? String(line.amountMinor / 10 ** minorExponent(display)) : "");
    setPendingDelete("");
    setOpen(true);
  };

  return (
    <section className="fin-panel" data-finance-panel="budget" data-shot="budget">
      <div className="plan-month" data-swipe-ignore="">
        <button type="button" className="plan-chevron" aria-label={t("Previous month")} disabled={month <= minMonth} onClick={() => goMonth(-1)}>
          <ChevronLeft size={18} strokeWidth={1.5} aria-hidden="true" />
        </button>
        <p className="min-w-0 flex-1 text-center text-[16px]">{monthLabel}</p>
        <button type="button" className="plan-chevron" aria-label={t("Next month")} disabled={month >= maxMonth} onClick={() => goMonth(1)}>
          <ChevronRight size={18} strokeWidth={1.5} aria-hidden="true" />
        </button>
      </div>
      <p className="mt-3 text-[15px] leading-6">{t("A budget for one month. Recurring bills stay in Activity.")}</p>
      <button type="button" className="mt-1 min-h-11 text-[15px] underline" aria-expanded={about} onClick={() => setAbout((value) => !value)}>
        {t("What a budget is")}
      </button>
      {about ? (
        <p className="mt-1 text-[15px] leading-6 text-[var(--text-secondary)]">
          {t("Plan is a budget for one month. Expected income is what you think will come in. Each expense row is a limit next to what you already spent. Left to spend is expected income, minus spending so far, minus the savings target. Recurring bills and paydays are in Activity, under Recurring.")}
        </p>
      ) : null}
      {!ready ? (
        <div className="mt-6">
          <p className="text-[16px] leading-6">{t("No budget for this month yet.")}</p>
          <p className="mt-2 text-[15px] leading-6 text-[var(--text-secondary)]">
            {t("Set the income you expect, a limit for each expense, and a savings target. Start from recurring bills, from recent months, or from both.")}
          </p>
          <button type="button" className="mt-4 min-h-11 bg-[var(--text-primary)] px-3 text-[15px] text-[var(--surface-0)]" onClick={() => { setSetupNote(""); setSetup(true); }}>
            {t("Set up this month")}
          </button>
        </div>
      ) : (
        <>
          {resolved.inherited ? <p className="mt-4 text-[13px] leading-5">{t("Carried from an earlier month.")}</p> : null}
          <p className="fin-muted mt-4 text-[13px]">{t("Expected income")}</p>
          <p className="mt-1 text-[15px] leading-5">
            {fill("{received} received of {planned}", { received: formatMoney(summary.incomeActual, display), planned: formatMoney(summary.incomePlanned, display) })}
          </p>
          <p className="fin-muted mt-4 text-[13px]">{t("Left to spend")}</p>
          <p className={`fin-hero-num mt-1 ${left < 0 ? "text-[var(--negative)]" : ""}`}>
            <span className="fin-hero-unit">{leftParts.unit}</span>
            {leftParts.figure}
          </p>
          <p className="mt-2 text-[13px] leading-5 text-[var(--text-secondary)]">{t("Expected income, minus spending so far, minus the savings target.")}</p>
          {overBudget(summary) ? <p className="mt-1 text-[13px] text-[var(--negative)]">{t("Over the plan")}</p> : null}
          <h3 className="mt-6 text-[16px]">{t("Expenses")}</h3>
          <div className="mt-1">
            {expenseRows.map((row) => {
              const categoryRow = finance.snapshot.categories.find((item) => item.name === row.name || item.name === row.category);
              const color = resolveColor(finance.snapshot.entityColors, "category", row.category || row.name, categoryRow?.color || "");
              return (
                <BudgetLineRow
                  key={row.id || row.name}
                  name={row.name}
                  color={color}
                  actual={formatMoney(row.actualMinor, display)}
                  planned={formatMoney(row.plannedMinor, display)}
                  ratio={row.ratio}
                  warn={row.warn}
                  over={row.over}
                  overLabel={t("Over the plan")}
                  onOpen={() => openEditor(row.id)}
                />
              );
            })}
          </div>
          <h3 className="mt-6 text-[16px]">{t("Savings target")}</h3>
          {savingRows.length === 0 ? (
            <button type="button" className="min-h-11 text-[15px] underline" onClick={() => { setRole("savings"); setCategory("Savings"); setEditing(""); setAmount(""); setSubcategory(""); setLabel(""); setOpen(true); }}>
              {t("Add a savings target")}
            </button>
          ) : (
            savingRows.map((row) => (
              <BudgetLineRow
                key={row.id || row.name}
                name={row.name}
                actual={formatMoney(row.actualMinor, display)}
                planned={formatMoney(row.plannedMinor, display)}
                ratio={row.ratio}
                warn={false}
                over={false}
                overLabel=""
                onOpen={() => openEditor(row.id)}
              />
            ))
          )}
          {incomeRows.length > 0 ? (
            <>
              <h3 className="mt-6 text-[16px]">{t("Income")}</h3>
              {incomeRows.map((row) => {
                const categoryRow = finance.snapshot.categories.find((item) => item.name === row.category || item.name === row.name);
                const color = resolveColor(finance.snapshot.entityColors, "income", row.category || row.name, categoryRow?.color || "");
                return (
                  <BudgetLineRow
                    key={row.id || row.name}
                    name={row.name}
                    color={color}
                    actual={formatMoney(row.actualMinor, display)}
                    planned={formatMoney(row.plannedMinor, display)}
                    ratio={row.ratio}
                    warn={false}
                    over={false}
                    overLabel=""
                    onOpen={() => openEditor(row.id)}
                  />
                );
              })}
            </>
          ) : null}
          <div className="mt-4 flex flex-col">
            <button type="button" className="min-h-11 text-left text-[15px] underline" onClick={() => openEditor()}>{t("Add a line")}</button>
            <button type="button" className="min-h-11 text-left text-[15px] underline" onClick={() => finance.saveBudget({ action: "set", period: clampMonth(shiftMonth(month, 1), minMonth, maxMonth), currency: display, replaceLines: true, copyForward: true, lines: resolved.budget?.lines || [], notify: resolved.budget?.notify || false })}>{t("Copy to next month")}</button>
            <button type="button" className="min-h-11 text-left text-[15px] underline" aria-pressed={resolved.budget?.copyForward !== false} onClick={() => saveLines(resolved.budget?.lines || [], { copyForward: resolved.budget?.copyForward === false })}>{t("Use this for later months")}</button>
            <button type="button" className="min-h-11 text-left text-[15px] underline" aria-pressed={Boolean(resolved.budget?.notify)} onClick={() => saveLines(resolved.budget?.lines || [], { notify: !resolved.budget?.notify })}>{t("Notify at 80% and 100%")}</button>
          </div>
          <ChartColorMenu
            series={summary.rows.map((row) => {
              const categoryRow = finance.snapshot.categories.find((item) => item.name === row.name);
              return { id: row.name, label: row.name, color: resolveColor(finance.snapshot.entityColors, categoryRow?.kind === "income" || row.role === "income" ? "income" : "category", row.name, categoryRow?.color || "") };
            })}
            currency={display}
            onPaint={(id, color) => {
              const categoryRow = finance.snapshot.categories.find((item) => item.name === id);
              finance.paintColor(categoryRow?.kind === "income" ? "income" : "category", id, color);
            }}
          />
        </>
      )}
      <Sheet open={setup} title={t("Set up this month")} onClose={() => setSetup(false)} className="finance">
        <div className="flex flex-col">
          <button type="button" className="min-h-11 text-left text-[16px] underline" onClick={() => prefill("rules")}>{t("Use recurring bills")}</button>
          <button type="button" className="min-h-11 text-left text-[16px] underline" onClick={() => prefill("average")}>{t("Use recent months")}</button>
          <button type="button" className="min-h-11 text-left text-[16px] underline" onClick={() => prefill("both")}>{t("Use both")}</button>
          <button type="button" className="min-h-11 text-left text-[16px] underline" onClick={() => { setSetup(false); openEditor(); }}>{t("Start blank")}</button>
          {setupNote ? <p className="mt-2 text-[15px] leading-6">{setupNote}</p> : null}
        </div>
      </Sheet>
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
