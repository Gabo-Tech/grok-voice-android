import { MoneyFlowPanel } from "@/components/MoneyFlowChart";
import "@/finance.css";
import { BudgetSection } from "@/components/BudgetSection";
import type { FinanceState } from "@/hooks/useFinance";
import type { FinanceAccount, FinanceCategory, FinanceFx, FinanceSnapshot, FinanceTransaction } from "@/lib/api";
import type { BudgetRecord } from "@/lib/budget";

const book: FinanceFx = {
  asOf: "2026-11-21",
  fetchedMs: Date.UTC(2026, 10, 21),
  stale: false,
  perEur: [
    { currency: "EUR", perEur: 1 },
    { currency: "CHF", perEur: 0.95 },
  ],
  manual: [],
};

function tx(partial: Partial<FinanceTransaction> & Pick<FinanceTransaction, "amountMinor" | "direction" | "category" | "date">): FinanceTransaction {
  return {
    id: partial.id || `${partial.category}-${partial.date}-${partial.amountMinor}`,
    amountMinor: partial.amountMinor,
    currency: partial.currency || "EUR",
    direction: partial.direction,
    category: partial.category,
    subcategory: partial.subcategory || "",
    accountId: partial.accountId || "",
    fromAccountId: "",
    date: partial.date,
    notes: partial.notes || "",
    recurrence: "once",
    ruleId: "",
    createdMs: 1,
    updatedMs: 1,
  };
}

const incomeMonth: FinanceTransaction[] = [
  tx({ amountMinor: 220_000, direction: "income", category: "Salary", date: "2026-11-01" }),
  tx({ amountMinor: 162_000, direction: "income", category: "Salary", date: "2026-11-15" }),
  tx({ amountMinor: 90_500, direction: "income", category: "Rent", date: "2026-11-03" }),
  tx({ amountMinor: 1_700, direction: "income", category: "Dividends", date: "2026-11-08" }),
  tx({ amountMinor: 4_410, direction: "income", category: "Interest", date: "2026-11-09" }),
  tx({ amountMinor: 1_380, direction: "income", category: "Cashback", date: "2026-11-10" }),
  tx({ amountMinor: 15_900, direction: "expense", category: "Restaurant", date: "2026-11-04" }),
  tx({ amountMinor: 14_900, direction: "expense", category: "Subscriptions", date: "2026-11-05" }),
  tx({ amountMinor: 14_600, direction: "expense", category: "Travel", date: "2026-11-06" }),
  tx({ amountMinor: 11_100, direction: "expense", category: "Gifts", date: "2026-11-07" }),
  tx({ amountMinor: 9_470, direction: "expense", category: "Transport", date: "2026-11-11" }),
  tx({ amountMinor: 9_110, direction: "expense", category: "Food", date: "2026-11-12" }),
  tx({ amountMinor: 7_860, direction: "expense", category: "Sport", date: "2026-11-13" }),
  tx({ amountMinor: 2_000, direction: "expense", category: "Coffee", date: "2026-11-14" }),
  tx({ amountMinor: 1_500, direction: "expense", category: "Tips", date: "2026-11-16" }),
  tx({ amountMinor: 50_000, direction: "transfer", category: "Funds", date: "2026-11-18", accountId: "fund" }),
];

const spendOnly: FinanceTransaction[] = [
  tx({ amountMinor: 26_186, direction: "payment", category: "", date: "2026-11-04" }),
  tx({ amountMinor: 26_186, direction: "expense", category: "Housing", date: "2026-11-05", accountId: "card" }),
  tx({ amountMinor: 9_110, direction: "expense", category: "Food", date: "2026-11-12" }),
  tx({ amountMinor: 4_200, direction: "expense", category: "Transport", date: "2026-11-13" }),
];

const categories: FinanceCategory[] = [
  { name: "Salary", color: "", kind: "income" },
  { name: "Rent", color: "", kind: "income" },
  { name: "Dividends", color: "", kind: "income" },
  { name: "Interest", color: "", kind: "income" },
  { name: "Cashback", color: "", kind: "income" },
  { name: "Restaurant", color: "", kind: "expense" },
  { name: "Subscriptions", color: "", kind: "expense" },
  { name: "Travel", color: "", kind: "expense" },
  { name: "Gifts", color: "", kind: "expense" },
  { name: "Transport", color: "", kind: "expense" },
  { name: "Food", color: "", kind: "expense" },
  { name: "Sport", color: "", kind: "expense" },
  { name: "Housing", color: "", kind: "expense" },
  { name: "Funds", color: "", kind: "investment" },
];

const budget: BudgetRecord = {
  id: "shot",
  period: "2026-11",
  currency: "EUR",
  copyForward: true,
  notify: true,
  warned: [],
  updatedMs: 1,
  lines: [
    { id: "s1", role: "income", category: "Salary", subcategory: "", label: "Salary 1 Nov", amountMinor: 220_000, currency: "EUR" },
    { id: "s2", role: "income", category: "Salary", subcategory: "", label: "Salary 15 Nov", amountMinor: 162_000, currency: "EUR" },
    { id: "r1", role: "income", category: "Rent", subcategory: "", label: "", amountMinor: 90_500, currency: "EUR" },
    { id: "d1", role: "income", category: "Dividends", subcategory: "", label: "", amountMinor: 1_700, currency: "EUR" },
    { id: "e1", role: "expense", category: "Restaurant", subcategory: "", label: "", amountMinor: 20_000, currency: "EUR" },
    { id: "e2", role: "expense", category: "Housing", subcategory: "", label: "", amountMinor: 80_000, currency: "EUR" },
    { id: "e3", role: "expense", category: "Food", subcategory: "", label: "", amountMinor: 12_000, currency: "EUR" },
    { id: "i1", role: "investment", category: "Funds", subcategory: "", label: "", amountMinor: 50_000, currency: "EUR" },
    { id: "v1", role: "savings", category: "Savings", subcategory: "", label: "", amountMinor: 100_000, currency: "EUR" },
  ],
};

function shotFinance(rows: FinanceTransaction[]): FinanceState {
  const snapshot: FinanceSnapshot = {
    accounts: [{ id: "fund", name: "Funds", kind: "investment", currency: "EUR", shownBalanceMinor: null } as FinanceAccount],
    transactions: rows,
    rules: [
      {
        id: "groceries",
        name: "Groceries",
        amountMinor: 4_000,
        currency: "EUR",
        direction: "expense",
        cadence: "monthly",
        weekdays: [],
        nextDate: "2026-11-28",
        category: "Food",
        accountId: "",
        notes: "",
        calendarEventId: "",
        updatedMs: 1,
        fromAccount: false,
        leadDays: 2,
        reminderId: 0,
        purpose: "recurring",
        status: "active",
        startDate: "2026-11-28",
      },
    ],
    totals: [],
    upcoming: [],
    period: "2026-11",
    defaultCurrency: "EUR",
    categories,
    fx: book,
    budgets: [budget],
  };
  return { snapshot, saveBudget: () => undefined, busy: false, inApp: false } as unknown as FinanceState;
}

/** Static preview for screenshots. Not a second finance store. */
export function FlowShot() {
  const params = new URLSearchParams(window.location.search);
  const kind = params.get("flowshot") || "category";
  const rows = kind === "none" ? spendOnly : incomeMonth;
  const showBudget = params.get("budget") === "1";
  return (
    <main className="finance min-h-full bg-[var(--surface-0)] px-4 py-6 text-[var(--text-primary)]">
      {showBudget ? <BudgetSection finance={shotFinance(rows)} /> : null}
      <MoneyFlowPanel transactions={rows} accounts={[{ id: "fund", name: "Funds", kind: "investment", currency: "EUR" } as FinanceAccount, { id: "card", name: "Card", kind: "credit_card", currency: "EUR" } as FinanceAccount]} categories={showBudget ? categories : []} display="EUR" fx={book} budgets={[budget]} />
    </main>
  );
}
