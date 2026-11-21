export const FINANCE_TABS = ["overview", "activity", "accounts", "holdings", "plan"] as const;
export type FinanceTab = (typeof FINANCE_TABS)[number];

export function readFinanceTab(saved: string | null): FinanceTab {
  return (FINANCE_TABS as readonly string[]).includes(saved || "") ? (saved as FinanceTab) : "overview";
}

export type FinancePlace = "overview" | "expenses" | "income" | "activity" | "plan";

/** Overview leads with net worth, then this month, then card debt. */
export function financePanels(place: FinancePlace): string[] {
  if (place === "expenses" || place === "activity") return ["expenses", "income", "activity"];
  if (place === "income") return ["income", "activity", "coming-income", "payouts"];
  if (place === "plan") return ["coming-expenses", "coming-income", "payouts"];
  return ["net-worth", "expenses", "income", "card-debt", "result", "upcoming"];
}
