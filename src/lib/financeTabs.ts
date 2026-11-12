export const FINANCE_TABS = ["overview", "expenses", "income", "accounts", "charts"] as const;
export type FinanceTab = (typeof FINANCE_TABS)[number];

export function readFinanceTab(saved: string | null): FinanceTab {
  return (FINANCE_TABS as readonly string[]).includes(saved || "") ? (saved as FinanceTab) : "overview";
}

export type FinancePlace = "overview" | "expenses" | "income";

/** Overview leads with net worth, then this month's expenses, then income, then the month result. */
export function financePanels(place: FinancePlace): string[] {
  if (place === "expenses") return ["expenses", "activity", "coming-expenses"];
  if (place === "income") return ["income", "activity", "coming-income", "payouts"];
  return ["net-worth", "expenses", "income", "result", "upcoming"];
}
