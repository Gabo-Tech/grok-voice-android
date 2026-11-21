import { PALETTE } from "./financeView.ts";

export type CategoryKind = "income" | "expense" | "savings" | "investment" | "debt";

export interface CategoryRecord {
  name: string;
  color: string;
  kind?: string;
  icon?: string;
  order?: number;
}

export interface CategoryUse {
  category: string;
  direction: string;
}

export const UNCATEGORIZED = "Uncategorized";

export const SUGGESTED_CATEGORIES: Array<{ name: string; kind: CategoryKind; icon: string }> = [
  { name: "Salary", kind: "income", icon: "briefcase" },
  { name: "Freelance", kind: "income", icon: "spark" },
  { name: "Dividends", kind: "income", icon: "trending" },
  { name: "Interest", kind: "income", icon: "percent" },
  { name: "Gifts", kind: "income", icon: "gift" },
  { name: "Refunds", kind: "income", icon: "undo" },
  { name: "Other income", kind: "income", icon: "wallet" },
  { name: "Housing", kind: "expense", icon: "home" },
  { name: "Food", kind: "expense", icon: "utensils" },
  { name: "Groceries", kind: "expense", icon: "utensils" },
  { name: "Transport", kind: "expense", icon: "bus" },
  { name: "Subscriptions", kind: "expense", icon: "repeat" },
  { name: "Health", kind: "expense", icon: "heart" },
  { name: "Savings", kind: "savings", icon: "piggy" },
  { name: "Investing", kind: "investment", icon: "trending" },
  { name: "Debt payments", kind: "debt", icon: "card" },
];

function fold(value: string): string {
  return value.trim().toLowerCase();
}

function kindOf(name: string, uses: CategoryUse[]): CategoryKind {
  const suggested = SUGGESTED_CATEGORIES.find((row) => fold(row.name) === fold(name));
  if (suggested) return suggested.kind;
  const rows = uses.filter((row) => fold(row.category) === fold(name));
  if (rows.length && rows.every((row) => row.direction === "income")) return "income";
  if (rows.length && rows.every((row) => row.direction === "payment")) return "debt";
  return "expense";
}

/**
 * Keeps saved colors. Blank categories become Uncategorized.
 * Kind is filled in from how the name was used, without inventing a second copy of a name.
 */
export function migrateCategories(categories: CategoryRecord[], uses: CategoryUse[]): { categories: CategoryRecord[]; rewrites: Array<{ from: string; to: string }> } {
  const rewrites: Array<{ from: string; to: string }> = [];
  const seen = new Set<string>();
  const next: CategoryRecord[] = [];
  categories.forEach((category, index) => {
    const name = category.name.trim();
    if (!name || seen.has(fold(name))) return;
    seen.add(fold(name));
    next.push({
      name,
      color: /^#[0-9A-Fa-f]{6}$/.test(category.color || "") ? category.color : PALETTE[index % PALETTE.length],
      kind: category.kind || kindOf(name, uses),
      icon: category.icon || SUGGESTED_CATEGORIES.find((row) => fold(row.name) === fold(name))?.icon || "",
      order: category.order ?? index,
    });
  });
  const blanks = uses.some((row) => !row.category.trim());
  if (blanks && !seen.has(fold(UNCATEGORIZED))) {
    next.push({
      name: UNCATEGORIZED,
      color: PALETTE[next.length % PALETTE.length],
      kind: "expense",
      icon: "wallet",
      order: next.length,
    });
    rewrites.push({ from: "", to: UNCATEGORIZED });
  }
  next.sort((a, b) => (a.order || 0) - (b.order || 0) || a.name.localeCompare(b.name));
  return { categories: next, rewrites };
}

export function withSuggested(categories: CategoryRecord[]): CategoryRecord[] {
  const next = categories.map((row) => ({ ...row }));
  const seen = new Set(next.map((row) => fold(row.name)));
  for (const suggestion of SUGGESTED_CATEGORIES) {
    if (seen.has(fold(suggestion.name))) continue;
    next.push({
      name: suggestion.name,
      color: PALETTE[next.length % PALETTE.length],
      kind: suggestion.kind,
      icon: suggestion.icon,
      order: next.length,
    });
    seen.add(fold(suggestion.name));
  }
  return next;
}

export function mergeCategory(categories: CategoryRecord[], from: string, into: string): CategoryRecord[] {
  const target = categories.find((row) => fold(row.name) === fold(into));
  if (!target || fold(from) === fold(into)) return categories;
  return categories.filter((row) => fold(row.name) !== fold(from));
}

export function reorderCategories(categories: CategoryRecord[], name: string, direction: -1 | 1): CategoryRecord[] {
  const rows = [...categories].sort((a, b) => (a.order || 0) - (b.order || 0) || a.name.localeCompare(b.name));
  const index = rows.findIndex((row) => fold(row.name) === fold(name));
  const swap = index + direction;
  if (index < 0 || swap < 0 || swap >= rows.length) return rows;
  const copy = [...rows];
  const [item] = copy.splice(index, 1);
  copy.splice(swap, 0, item);
  return copy.map((row, order) => ({ ...row, order }));
}
