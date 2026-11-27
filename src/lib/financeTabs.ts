export const FINANCE_TABS = ["overview", "activity", "accounts", "holdings", "plan"] as const;
export type FinanceTab = (typeof FINANCE_TABS)[number];

export function readFinanceTab(saved: string | null): FinanceTab {
  return (FINANCE_TABS as readonly string[]).includes(saved || "") ? (saved as FinanceTab) : "overview";
}

export type FinancePlace = "overview" | "expenses" | "income" | "activity" | "plan";

/** Overview: net worth, this month, the next 30 days. Activity is the ledger. Plan is the month's budget. */
export function financePanels(place: FinancePlace): string[] {
  if (place === "expenses" || place === "activity") return ["activity"];
  if (place === "income") return ["activity", "coming-income", "payouts"];
  if (place === "plan") return ["budget"];
  return ["net-worth", "month", "next"];
}
