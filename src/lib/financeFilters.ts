export type FilterId = "account" | "category" | "currency" | "recurrence" | "direction" | "group";
export type DirectionLock = "expense" | "income" | "any";

export interface LedgerFilters {
  account: string;
  category: string;
  currency: string;
  recurrence: string;
  direction: string;
  group: string;
}

export interface AppliedFilter {
  id: FilterId;
  label: string;
}

/** Sheet filters that are not the default. The date range stays on the scrolling row. */
export function appliedFilters(filters: LedgerFilters, labels: Partial<Record<FilterId, string>>, directionLock: DirectionLock): AppliedFilter[] {
  const chips: AppliedFilter[] = [];
  const push = (id: FilterId, on: boolean) => {
    const label = labels[id];
    if (on && label) chips.push({ id, label });
  };
  push("account", filters.account !== "all");
  push("category", filters.category !== "all");
  push("currency", filters.currency !== "all");
  push("recurrence", filters.recurrence !== "all");
  push("direction", filters.direction !== directionDefault(directionLock));
  push("group", filters.group !== "all");
  return chips;
}

export function directionDefault(directionLock: DirectionLock): string {
  return directionLock === "any" ? "all" : directionLock;
}

export function withoutFilter(filters: LedgerFilters, id: FilterId, directionLock: DirectionLock): LedgerFilters {
  if (id === "account") return { ...filters, account: "all" };
  if (id === "category") return { ...filters, category: "all" };
  if (id === "currency") return { ...filters, currency: "all" };
  if (id === "recurrence") return { ...filters, recurrence: "all" };
  if (id === "group") return { ...filters, group: "all" };
  return { ...filters, direction: directionDefault(directionLock) };
}

export function clearedFilters(directionLock: DirectionLock): LedgerFilters {
  return {
    account: "all",
    category: "all",
    currency: "all",
    recurrence: "all",
    direction: directionDefault(directionLock),
    group: "all",
  };
}
