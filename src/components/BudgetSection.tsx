import { useEffect, useMemo, useState } from "react";
import { api } from "@/lib/api";
import type { FinanceState } from "@/hooks/useFinance";
import type { FinanceBudget, FinanceBudgetLine } from "@/lib/api";
import { overBudget, resolveBudget, shiftMonth, summarizeBudget, type BudgetRecord, type BudgetRole } from "@/lib/budget";
import { bookFromStatus, plannedHits } from "@/lib/financeMath";
import { flowPercent, formatMoney, minorExponent } from "@/lib/money";
import { fill, useT } from "@/lib/i18n";
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
    lines.push({
      id: "",
      role,
      category: category.trim(),
      subcategory: subcategory.trim(),
      label: label.trim(),
      amountMinor,
      currency: display,
    });
    saveLines(lines);
    setCategory("");
    setSubcategory("");
    setLabel("");
    setAmount("");
    setOpen(false);
  };

  const removeLine = (id: string) => {
    saveLines((resolved.budget?.lines || []).filter((line) => line.id !== id));
    setPendingDelete("");
  };

  return (
    <section className="fin-panel" data-finance-panel="budget" data-shot="budget">
      <div className="flex items-center justify-between gap-3">
        <h2 className="fin-kicker">{t("Budget")}</h2>
        <div className="flex items-center gap-2">
          <button type="button" className="fin-chip" onClick={() => setMonth(shiftMonth(month, -1))} aria-label={t("Previous month")}>
            {t("Previous month")}
          </button>
          <p className="text-[15px]">{month}</p>
          <button type="button" className="fin-chip" onClick={() => setMonth(shiftMonth(month, 1))} aria-label={t("Next month")}>
            {t("Next month")}
          </button>
        </div>
      </div>
      <p className="fin-muted mt-2 text-[13px] leading-5">{t("How this month is planned.")}</p>
      {resolved.inherited ? <p className="mt-2 text-[13px] leading-5">{t("From the template")}</p> : null}
      {!resolved.budget ? (
        <div className="mt-4">
          <p className="text-[15px] leading-5">{t("No budget for this month.")}</p>
          <button type="button" className="mt-3 min-h-11 bg-[var(--text-primary)] px-3 text-[15px] text-[var(--surface-0)]" onClick={() => finance.saveBudget({ action: "create", period: month, currency: display, copyForward: true })}>
            {t("Create budget")}
          </button>
        </div>
      ) : (
        <>
          <dl className="mt-4 space-y-2 text-[15px] leading-5">
            <div className="flex justify-between gap-3"><dt>{t("Expected income")}</dt><dd className="fin-num">{formatMoney(summary.incomeActual, display)} / {formatMoney(summary.incomePlanned, display)}</dd></div>
            <div className="flex justify-between gap-3"><dt>{t("Expense limit")}</dt><dd className="fin-num">{formatMoney(summary.spendingActual, display)} / {formatMoney(summary.spendingPlanned, display)}</dd></div>
            <div className="flex justify-between gap-3"><dt>{t("Planned savings")}</dt><dd className="fin-num">{formatMoney(summary.savingsActual, display)} / {formatMoney(summary.savingsPlanned, display)}</dd></div>
            <div className="flex justify-between gap-3"><dt>{t("Planned investments")}</dt><dd className="fin-num">{formatMoney(summary.investmentsActual, display)} / {formatMoney(summary.investmentsPlanned, display)}</dd></div>
            <div className="flex justify-between gap-3"><dt>{t("Projected net")}</dt><dd className="fin-num">{formatMoney(summary.projectedNet, display)}</dd></div>
          </dl>
          {overBudget(summary) ? <p className="mt-3 text-[15px] leading-5 text-[var(--negative)]">{t("You are over the spending plan.")}</p> : null}
          {summary.warnings.map((row) => (
            <p key={row.name} className="mt-2 text-[13px] leading-5 text-[var(--negative)]">
              {row.over ? fill("{name} is over the plan by {amount}", { name: row.name, amount: formatMoney(row.amountMinor, display) }) : fill("{name} is at {percent}% of the plan", { name: row.name, percent: Math.round(row.ratio * 100) })}
            </p>
          ))}
          <div className="mt-2">
            {summary.rows.map((row) => (
              <div key={row.id || row.name} className="budget-line">
                <div className="flex items-baseline justify-between gap-3">
                  <div className="min-w-0">
                    <p className="text-[15px]">{row.name}</p>
                    <p className="fin-muted text-[13px]">{roleLabel(row.role)}</p>
                  </div>
                  <p className="fin-num text-[15px]">{formatMoney(row.actualMinor, display)}</p>
                </div>
                <div className="budget-bar" data-warn={row.warn ? "true" : "false"} data-over={row.over ? "true" : "false"}>
                  <span style={{ width: `${Math.min(100, Math.round((Number.isFinite(row.ratio) ? row.ratio : 1) * 100))}%` }} />
                </div>
                <p className="fin-muted text-[13px]">
                  {t("Planned")} {formatMoney(row.plannedMinor, display)}
                  {" · "}
                  {t("Actual")} {formatMoney(row.actualMinor, display)}
                  {" · "}
                  {row.remainingMinor < 0 ? t("Overspent") : t("Left")} {formatMoney(Math.abs(row.remainingMinor), display)}
                  {" · "}
                  {flowPercent(Number.isFinite(row.ratio) ? row.ratio : 1)}
                </p>
                {pendingDelete === row.id ? (
                  <div className="flex items-center gap-3">
                    <p className="text-[13px]">{t("Delete this line?")}</p>
                    <button type="button" className="min-h-11 bg-[var(--negative)] px-3 text-[15px] text-[var(--surface-0)]" onClick={() => removeLine(row.id)}>
                      {t("Yes, delete")}
                    </button>
                    <button type="button" className="fin-chip" onClick={() => setPendingDelete("")}>
                      {t("Cancel")}
                    </button>
                  </div>
                ) : (
                  <button type="button" className="fin-chip" onClick={() => setPendingDelete(row.id)}>
                    {t("Delete")}
                  </button>
                )}
              </div>
            ))}
          </div>
          <div className="mt-3 flex flex-wrap gap-x-3 gap-y-1">
            <button type="button" className="fin-chip" onClick={() => setOpen((value) => !value)}>
              {t("Add a line")}
            </button>
            <button
              type="button"
              className="fin-chip"
              onClick={() => finance.saveBudget({ action: "set", period: shiftMonth(month, 1), currency: display, replaceLines: true, copyForward: true, lines: resolved.budget?.lines || [], notify: resolved.budget?.notify || false })}
            >
              {t("Copy to next month")}
            </button>
            <button
              type="button"
              className="fin-chip"
              onClick={() => finance.saveBudget({ action: "set", period: "", currency: display, replaceLines: true, copyForward: true, lines: resolved.budget?.lines || [], notify: resolved.budget?.notify || false })}
            >
              {t("Use as template")}
            </button>
            <button type="button" className="fin-chip" aria-pressed={resolved.budget.notify} onClick={() => saveLines(resolved.budget?.lines || [], { notify: !resolved.budget?.notify })}>
              {t("Notify at 80% and 100%")}
            </button>
          </div>
        </>
      )}
      {open ? (
        <form
          className="mt-4 space-y-3"
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
        </form>
      ) : null}
    </section>
  );
}
