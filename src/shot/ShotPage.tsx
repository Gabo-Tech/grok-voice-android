import { useMemo } from "react";
import { BudgetSection } from "@/components/BudgetSection";
import { FinanceDashboard } from "@/components/FinanceDashboard";
import { MoneyFlowPanel } from "@/components/MoneyFlowChart";
import { todayIso } from "@/components/FinanceDashboard";
import type { FinanceState } from "@/hooks/useFinance";
import type { FinanceAccount, FinanceBudget, FinanceCategory, FinanceRule, FinanceSnapshot, FinanceTransaction } from "@/lib/api";
import { emptyFinance } from "@/lib/money";

function iso(offset: number): string {
  const date = new Date();
  date.setDate(date.getDate() + offset);
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

function tx(partial: Pick<FinanceTransaction, "id" | "amountMinor" | "direction" | "category" | "date">): FinanceTransaction {
  return {
    currency: "CHF",
    subcategory: "",
    accountId: "bank",
    fromAccountId: "",
    notes: "",
    recurrence: "once",
    ruleId: "",
    createdMs: 1,
    updatedMs: 1,
    ...partial,
  };
}

function rule(partial: Pick<FinanceRule, "id" | "name" | "amountMinor" | "nextDate" | "category"> & { direction?: string }): FinanceRule {
  return {
    currency: "EUR",
    direction: partial.direction || "expense",
    cadence: "monthly",
    weekdays: [],
    accountId: "bank",
    notes: "",
    calendarEventId: "",
    updatedMs: 1,
    fromAccount: false,
    leadDays: 0,
    reminderId: 0,
    purpose: "recurring",
    status: "active",
    startDate: partial.nextDate,
    endKind: "none",
    endCount: 0,
    endDate: "",
    ...partial,
  };
}

const categories: FinanceCategory[] = [
  { name: "Salary", color: "#3caa68", kind: "income" },
  { name: "Housing", color: "#4C78D8", kind: "expense" },
  { name: "Food", color: "#e07a74", kind: "expense" },
  { name: "Transport", color: "#e0b15a", kind: "expense" },
  { name: "Subscriptions", color: "#6aa8e0", kind: "expense" },
  { name: "Coffee", color: "#c4a484", kind: "expense" },
  { name: "Tips", color: "#d7a3c4", kind: "expense" },
];

function flowTransactions(empty: boolean): FinanceTransaction[] {
  const month = todayIso().slice(0, 7);
  const expenses = [
    tx({ id: "housing", amountMinor: 140000, direction: "expense", category: "Housing", date: `${month}-02` }),
    tx({ id: "food", amountMinor: 62000, direction: "expense", category: "Food", date: `${month}-04` }),
    tx({ id: "transport", amountMinor: 28000, direction: "expense", category: "Transport", date: `${month}-06` }),
    tx({ id: "subs", amountMinor: 19000, direction: "expense", category: "Subscriptions", date: `${month}-08` }),
    tx({ id: "coffee", amountMinor: 2000, direction: "expense", category: "Coffee", date: `${month}-09` }),
    tx({ id: "tips", amountMinor: 1500, direction: "expense", category: "Tips", date: `${month}-10` }),
  ];
  if (empty) return expenses;
  return [
    tx({ id: "salary-1", amountMinor: 420000, direction: "income", category: "Salary", date: `${month}-01` }),
    tx({ id: "salary-2", amountMinor: 180000, direction: "income", category: "Salary", date: `${month}-15` }),
    ...expenses,
  ];
}

function FlowShot({ empty }: { empty: boolean }) {
  return (
    <div className="finance min-h-screen bg-[var(--surface-0)] px-4 py-6 text-[var(--text-primary)]">
      <MoneyFlowPanel transactions={flowTransactions(empty)} accounts={[] as FinanceAccount[]} categories={categories} display="CHF" />
    </div>
  );
}

function planFinance(empty: boolean): FinanceState {
  const month = todayIso().slice(0, 7);
  const base = emptyFinance("en-CH");
  const budget: FinanceBudget = {
    id: "b1",
    period: month,
    currency: "CHF",
    copyForward: true,
    notify: false,
    warned: [],
    updatedMs: 1,
    lines: [
      { id: "l1", role: "expense", category: "Housing", subcategory: "", label: "", amountMinor: 150000, currency: "CHF" },
      { id: "l2", role: "expense", category: "Food", subcategory: "", label: "", amountMinor: 80000, currency: "CHF" },
      { id: "l3", role: "expense", category: "Transport", subcategory: "", label: "", amountMinor: 40000, currency: "CHF" },
    ],
  };
  const snapshot: FinanceSnapshot = {
    ...base,
    defaultCurrency: "CHF",
    period: month,
    categories,
    transactions: empty
      ? []
      : [
          tx({ id: "housing-a", amountMinor: 140000, direction: "expense", category: "Housing", date: `${month}-02` }),
          tx({ id: "food-a", amountMinor: 41000, direction: "expense", category: "Food", date: `${month}-04` }),
        ],
    rules: empty
      ? []
      : [
          rule({ id: "cyber", name: "Cyber payment", amountMinor: 28000, nextDate: iso(0), category: "Subscriptions" }),
          rule({ id: "abanca", name: "Abanca payment", amountMinor: 1035, nextDate: iso(9), category: "Housing" }),
        ],
    budgets: empty ? [] : [budget],
    fx: { asOf: todayIso(), fetchedMs: Date.now(), stale: false, perEur: [{ currency: "EUR", perEur: 1 }, { currency: "CHF", perEur: 0.95 }], manual: [] },
  };
  const noop = () => undefined;
  return {
    snapshot,
    error: null,
    busy: false,
    inApp: false,
    load: noop,
    saveAccount: noop,
    saveTransaction: noop,
    saveRule: noop,
    saveBudget: noop,
    removeAccount: noop,
    removeTransaction: noop,
    removeRule: noop,
    setDefaultCurrency: noop,
    renameCategory: noop,
    editCategory: noop,
    markPaid: noop,
    scheduleAction: noop,
    refreshRates: noop,
    refreshPrices: async () => "",
    refreshHolding: async () => "",
    setHoldingProvider: noop,
    setManualRate: noop,
    setPriceProvider: noop,
    exportPdf: async () => ({ fileName: "", dataB64: "", shared: false, pages: 0 }),
    setError: noop,
  } as FinanceState;
}

function PlanShot({ empty }: { empty: boolean }) {
  const finance = useMemo(() => planFinance(empty), [empty]);
  return (
    <div className="finance min-h-screen bg-[var(--surface-0)] px-4 py-6 text-[var(--text-primary)]">
      <div className="plan-board" data-shot="plan">
        <BudgetSection finance={finance} />
        <FinanceDashboard finance={finance} accounts={[]} place="plan" onProgram={() => undefined} onIncome={() => undefined} onFocus={() => undefined} />
      </div>
    </div>
  );
}

export function ShotPage({ kind }: { kind: string }) {
  if (kind === "flow-empty") return <FlowShot empty />;
  if (kind === "plan-full") return <PlanShot empty={false} />;
  if (kind === "plan-empty") return <PlanShot empty />;
  return <FlowShot empty={false} />;
}
