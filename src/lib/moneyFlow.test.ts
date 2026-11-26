import assert from "node:assert/strict";
import { buildMoneyFlow, buildSankeyOption, describeFlow, flowAmountLabel, flowConserved, flowSlices, labelsInside, labelsOverlap, layoutSankey, leafOffsets, placeFlowLabels, portraitLevels, sankeyFrame, sankeyValue, spreadColumn, spreadLevel } from "./moneyFlow.ts";
import { migrateCategories, mergeCategory, reorderCategories, UNCATEGORIZED } from "./financeCategories.ts";
import { boundsOf } from "./chartPeriod.ts";
import type { RateBook } from "./financeMath.ts";

const now = Date.UTC(2026, 10, 21, 12, 0);
const book: RateBook = {
  asOf: "2026-11-21",
  fetchedMs: now,
  perEur: { EUR: 1, CHF: 0.95, USD: 1.08 },
  manual: [],
};

const flow = buildMoneyFlow(
  [
    { amountMinor: 350000, currency: "CHF", direction: "income", category: "Salary", date: "2026-11-01" },
    { amountMinor: 50000, currency: "EUR", direction: "income", category: "Freelance", date: "2026-11-02" },
    { amountMinor: 120000, currency: "EUR", direction: "expense", category: "Housing", date: "2026-11-02", accountKind: "bank" },
    { amountMinor: 4299, currency: "USD", direction: "expense", category: "Groceries", date: "2026-11-04", accountKind: "credit_card" },
    { amountMinor: 20000, currency: "CHF", direction: "payment", category: "Swisscard", date: "2026-11-06", accountKind: "credit_card", fromKind: "bank" },
    { amountMinor: 5000, currency: "CHF", direction: "transfer", category: "Move", date: "2026-11-07", accountKind: "bank", fromKind: "bank" },
    { amountMinor: 900, currency: "CHF", direction: "expense", category: "Coffee", date: "2026-10-02" },
  ],
  { display: "CHF", book, nowMs: now, start: "2026-11-01", end: "2026-11-30", threshold: 0.03 },
);
assert.equal(flow.state, "ready");
if (flow.state !== "ready") throw new Error("flow");
const salary = flow.nodes.find((node) => node.label === "Salary");
const freelance = flow.nodes.find((node) => node.label === "Freelance");
const housing = flow.nodes.find((node) => node.label === "Housing");
const card = flow.nodes.find((node) => node.label === "Swisscard");
const pool = flow.nodes.find((node) => node.id === "pool");
assert.ok(salary && freelance && housing && card && pool);
assert.equal(salary.amountMinor, 350000);
assert.equal(freelance.originals.find((row) => row.currency === "EUR")?.amountMinor, 50000);
assert.notEqual(freelance.amountMinor, 50000);
assert.equal(housing.originals.find((row) => row.currency === "EUR")?.amountMinor, 120000);
assert.equal(card.kind, "debt");
const expenseSum = flow.nodes.filter((node) => node.kind === "expense").reduce((sum, node) => sum + node.amountMinor, 0);
const debtSum = flow.nodes.filter((node) => node.kind === "debt").reduce((sum, node) => sum + node.amountMinor, 0);
assert.equal(debtSum, 20000);
assert.ok(expenseSum > 0);
assert.equal(flow.nodes.some((node) => node.label === "Move"), false);
assert.equal(flow.nodes.some((node) => node.label === "Coffee"), false);
assert.equal(pool.amountMinor, flow.nodes.filter((node) => node.role === "source").reduce((sum, node) => sum + node.amountMinor, 0));
assert.equal(pool.amountMinor, flow.nodes.filter((node) => node.role === "hub").reduce((sum, node) => sum + node.amountMinor, 0));
const spent = expenseSum + debtSum + flow.nodes.filter((node) => node.kind === "other").reduce((sum, node) => sum + node.amountMinor, 0);
assert.ok(spent <= pool.amountMinor);
assert.equal(flow.nodes.some((node) => node.kind === "savings"), true);
assert.equal(flow.nodes.some((node) => node.role === "source" && node.label === "Spending"), false);

const deficit = buildMoneyFlow(
  [
    { amountMinor: 10000, currency: "CHF", direction: "income", category: "Salary", date: "2026-11-01" },
    { amountMinor: 40000, currency: "CHF", direction: "expense", category: "Housing", date: "2026-11-02" },
  ],
  { display: "CHF", book, nowMs: now, start: "2026-11-01", end: "2026-11-30" },
);
assert.equal(deficit.state, "ready");
if (deficit.state === "ready") {
  const drawn = deficit.nodes.find((node) => node.kind === "drawn");
  assert.ok(drawn);
  assert.equal(drawn.amountMinor, 30000);
  assert.equal(deficit.nodes.find((node) => node.id === "pool")?.amountMinor, 10000);
  assert.equal(deficit.links.find((link) => link.source === "in:shortfall")?.target, "spend");
  assert.equal(deficit.links.find((link) => link.source === "pool" && link.target === "spend")?.amountMinor, 10000);
  assert.equal(deficit.links.some((link) => link.source === "in:shortfall" && link.target === "pool"), false);
  assert.equal(deficit.nodes.some((node) => node.kind === "surplus"), false);
}

const surplus = buildMoneyFlow(
  [
    { amountMinor: 80000, currency: "CHF", direction: "income", category: "", date: "2026-11-01" },
    { amountMinor: 10000, currency: "CHF", direction: "expense", category: "Food", date: "2026-11-03" },
    { amountMinor: 5000, currency: "CHF", direction: "transfer", category: "Investing", date: "2026-11-04", accountKind: "investment" },
  ],
  { display: "CHF", book, nowMs: now, start: "2026-11-01", end: "2026-11-30", categories: [{ name: "Investing", color: "#4C78D8", kind: "investment" }] },
);
assert.equal(surplus.state, "ready");
if (surplus.state === "ready") {
  assert.equal(surplus.nodes.find((node) => node.label === "Uncategorized" && node.role === "source")?.amountMinor, 80000);
  assert.equal(surplus.nodes.find((node) => node.kind === "investment" && node.role === "hub")?.amountMinor, 5000);
  assert.equal(surplus.nodes.find((node) => node.kind === "savings")?.amountMinor, 65000);
  assert.equal(surplus.nodes.filter((node) => node.kind === "expense").reduce((sum, node) => sum + node.amountMinor, 0), 10000);
  assert.equal(surplus.nodes.some((node) => node.kind === "surplus"), false);
}

const tiny = buildMoneyFlow(
  [
    { amountMinor: 100000, currency: "CHF", direction: "income", category: "Salary", date: "2026-11-01" },
    { amountMinor: 50000, currency: "CHF", direction: "expense", category: "Housing", date: "2026-11-02" },
    { amountMinor: 1000, currency: "CHF", direction: "expense", category: "Coffee", date: "2026-11-03" },
    { amountMinor: 800, currency: "CHF", direction: "expense", category: "Tips", date: "2026-11-04" },
  ],
  { display: "CHF", book, nowMs: now, start: "2026-11-01", end: "2026-11-30", threshold: 0.03 },
);
assert.equal(tiny.state, "ready");
if (tiny.state === "ready") {
  const other = tiny.nodes.find((node) => node.id === "out:rest");
  assert.ok(other);
  assert.equal(other.italic, true);
  assert.equal(other.label, "Rest");
  assert.equal(other.amountMinor, 1800);
  assert.equal(other.members.length, 2);
  assert.equal(tiny.nodes.some((node) => node.label === "Coffee"), false);
  const open = buildMoneyFlow(
    [
      { amountMinor: 100000, currency: "CHF", direction: "income", category: "Salary", date: "2026-11-01" },
      { amountMinor: 50000, currency: "CHF", direction: "expense", category: "Housing", date: "2026-11-02" },
      { amountMinor: 1000, currency: "CHF", direction: "expense", category: "Coffee", date: "2026-11-03" },
      { amountMinor: 800, currency: "CHF", direction: "expense", category: "Tips", date: "2026-11-04" },
    ],
    { display: "CHF", book, nowMs: now, start: "2026-11-01", end: "2026-11-30", threshold: 0.03, expandedOther: true },
  );
  assert.equal(open.state, "ready");
  if (open.state === "ready") assert.equal(open.nodes.some((node) => node.label === "Coffee"), true);
}

const blocked = buildMoneyFlow(
  [{ amountMinor: 100, currency: "JPY", direction: "income", category: "Salary", date: "2026-11-01" }],
  { display: "CHF", book, nowMs: now, start: "2026-11-01", end: "2026-11-30" },
);
assert.equal(blocked.state, "needs-rate");
if (blocked.state === "needs-rate") assert.deepEqual(blocked.missing, ["JPY"]);

assert.equal(flowConserved(flow), true);
assert.equal(flowConserved(deficit), true);
assert.equal(flowConserved(surplus), true);
assert.equal(flowConserved(tiny), true);

const nested = buildMoneyFlow(
  [
    { amountMinor: 20000, currency: "CHF", direction: "income", category: "Salary", date: "2026-11-01" },
    { amountMinor: 8000, currency: "CHF", direction: "expense", category: "Housing", subcategory: "Rent", date: "2026-11-02" },
    { amountMinor: 2000, currency: "CHF", direction: "expense", category: "Vacuum", date: "2026-11-03" },
    { amountMinor: 30000, currency: "CHF", direction: "expense", category: "Food", date: "2026-11-04" },
  ],
  {
    display: "CHF",
    book,
    nowMs: now,
    start: "2026-11-01",
    end: "2026-11-30",
    threshold: 1,
    categories: [
      { name: "Housing", color: "#112233", kind: "expense" },
      { name: "Vacuum", color: "#445566", kind: "expense", parent: "Housing" },
      { name: "Food", color: "#778899", kind: "expense" },
    ],
  },
);
assert.equal(nested.state, "ready");
if (nested.state === "ready") {
  assert.equal(flowConserved(nested), true);
  const housing = nested.nodes.find((node) => node.label === "Housing" && node.role === "sink");
  assert.ok(housing);
  assert.equal(housing.amountMinor, 10000);
  assert.equal(housing.children.length, 2);
  assert.equal(nested.nodes.some((node) => node.label === "Vacuum" && node.role === "child"), true);
  assert.equal(nested.nodes.some((node) => node.label === "Drawn from savings"), false);
  const drawn = nested.nodes.find((node) => node.kind === "drawn");
  assert.equal(drawn?.label, "Shortfall / From savings");
  assert.equal(drawn?.amountMinor, 20000);
  const amounts: Record<string, number> = {};
  for (const node of nested.nodes) amounts[node.id] = node.amountMinor;
  const columns = [
    nested.nodes.filter((node) => node.role === "source").map((node) => node.id),
    ["pool"],
    nested.nodes.filter((node) => node.role === "hub").map((node) => node.id),
    nested.nodes.filter((node) => node.role === "sink").map((node) => node.id),
  ];
  const laid = layoutSankey(columns, amounts, nested.links, 320);
  const poolBox = laid.nodes.find((node) => node.id === "pool");
  assert.ok(poolBox);
  const intoPool = laid.ribbons.filter((ribbon) => ribbon.target === "pool").reduce((sum, ribbon) => sum + ribbon.amountMinor, 0);
  const outOfPool = laid.ribbons.filter((ribbon) => ribbon.source === "pool").reduce((sum, ribbon) => sum + ribbon.amountMinor, 0);
  assert.equal(intoPool, poolBox.w > 0 ? nested.nodes.find((node) => node.id === "pool")?.amountMinor : 0);
  assert.equal(outOfPool, intoPool);
  for (const ribbon of laid.ribbons) {
    const width = ribbon.d.match(/L ([0-9.]+) /);
    assert.ok(width, ribbon.d);
  }
}

const saved = migrateCategories(
  [{ name: "Groceries", color: "#112233" }, { name: "Salary", color: "nope" }],
  [
    { category: "", direction: "expense" },
    { category: "Groceries", direction: "expense" },
    { category: "Salary", direction: "income" },
  ],
);
assert.equal(saved.categories.find((row) => row.name === "Groceries")?.color, "#112233");
assert.equal(saved.categories.find((row) => row.name === "Salary")?.kind, "income");
assert.equal(saved.categories.some((row) => row.name === UNCATEGORIZED), true);
assert.deepEqual(saved.rewrites, [{ from: "", to: UNCATEGORIZED }]);
const merged = mergeCategory(saved.categories, "Groceries", UNCATEGORIZED);
assert.equal(merged.some((row) => row.name === "Groceries"), false);
assert.equal(merged.some((row) => row.name === UNCATEGORIZED), true);
const ordered = reorderCategories(
  [
    { name: "A", color: "#112233", order: 0 },
    { name: "B", color: "#223344", order: 1 },
  ],
  "B",
  -1,
);
assert.equal(ordered[0].name, "B");
assert.equal(ordered[0].order, 0);

const november = boundsOf({ preset: "this", start: "", end: "" }, new Date(2026, 10, 21));
assert.equal(november.start, "2026-11-01");
assert.equal(november.end, "2026-11-30");
const last = boundsOf({ preset: "last", start: "", end: "" }, new Date(2026, 10, 21));
assert.equal(last.start, "2026-10-01");
assert.equal(last.end, "2026-10-31");
const custom = boundsOf({ preset: "custom", start: "2026-01-05", end: "2026-02-02" }, new Date(2026, 10, 21));
assert.equal(custom.start, "2026-01-05");
assert.equal(custom.end, "2026-02-02");

const described = buildMoneyFlow(
  [
    { amountMinor: 17_900, currency: "CHF", direction: "expense", category: "vacuum cleaner", date: "2026-11-03", accountKind: "credit_card" },
    { amountMinor: 12_000, currency: "CHF", direction: "expense", category: "Housing", subcategory: "Rent", date: "2026-11-04", accountKind: "bank" },
  ],
  {
    display: "CHF",
    book,
    nowMs: now,
    start: "2026-11-01",
    end: "2026-11-30",
    threshold: 0,
    categories: [
      { name: "Housing", parent: "", color: "" },
      { name: "Rent", parent: "Housing", color: "" },
      { name: "Uncategorized", parent: "", color: "" },
    ],
    labels: { uncategorized: "Uncategorized", total: "Total", drawn: "From savings", income: "Income", surplus: "Surplus", other: "Other", debt: "Debt payments", savings: "Savings", investments: "Investing" },
  },
);
assert.equal(described.state, "ready");
if (described.state === "ready") {
  assert.equal(described.nodes.some((node) => node.label === "vacuum cleaner"), false);
  assert.ok(described.nodes.some((node) => node.label === "Housing"));
  assert.equal(described.nodes.find((node) => node.label === "Housing" && node.role === "sink")?.amountMinor, 17_900 + 12_000);
  assert.ok(flowConserved(described));
}

const describedEmpty = buildMoneyFlow(
  [
    { amountMinor: 35_800, currency: "CHF", direction: "expense", category: "vacuum cleaner", notes: "vacuum cleaner", date: "2026-11-03", accountName: "Swisscard", accountKind: "credit_card" },
    { amountMinor: 12_792, currency: "CHF", direction: "expense", category: "air purifier", date: "2026-11-04", accountName: "Swisscard", accountKind: "credit_card" },
    { amountMinor: 1_690, currency: "CHF", direction: "expense", category: "", notes: "mystery", date: "2026-11-05", accountName: "Cash", accountKind: "cash" },
    { amountMinor: 2_090, currency: "CHF", direction: "expense", category: "Tips", date: "2026-11-06", accountName: "Cash", accountKind: "cash" },
  ],
  { display: "CHF", book, nowMs: now, start: "2026-11-01", end: "2026-11-30", threshold: 0.03 },
);
assert.equal(describedEmpty.state, "ready");
if (describedEmpty.state === "ready") {
  assert.equal(describedEmpty.nodes.some((node) => node.label === "vacuum cleaner" || node.label === "air purifier"), false);
  assert.ok(describedEmpty.nodes.some((node) => node.label === "Housing"));
  assert.equal(describedEmpty.nodes.filter((node) => node.label === "Uncategorized" || node.label === "Rest").length <= 2, true);
  assert.equal(describedEmpty.nodes.some((node) => node.role === "source" && (node.label === "Swisscard" || node.label === "Cash" || node.label === "Spending")), false);
  assert.equal(describedEmpty.noIncome, true);
  assert.equal(describedEmpty.nodes.some((node) => node.id === "pool" || node.label === "Total income"), false);
  const gap = describedEmpty.nodes.find((node) => node.kind === "drawn");
  assert.ok(gap);
  assert.equal(gap.amountMinor, 35_800 + 12_792 + 1_690 + 2_090);
  assert.ok(describedEmpty.links.some((link) => link.source === "in:shortfall" && link.target === "spend"));
  assert.equal(describedEmpty.links.some((link) => link.target === "pool"), false);
  assert.ok(flowConserved(describedEmpty));
}

const byEntry = buildMoneyFlow(
  [
    { amountMinor: 191_000, currency: "EUR", direction: "income", category: "Salary", date: "2026-11-01" },
    { amountMinor: 191_000, currency: "EUR", direction: "income", category: "Salary", date: "2026-11-15" },
    { amountMinor: 90_500, currency: "EUR", direction: "income", category: "Rent", date: "2026-11-03" },
    { amountMinor: 1_700, currency: "EUR", direction: "income", category: "Dividends", date: "2026-11-08" },
    { amountMinor: 15_900, currency: "EUR", direction: "expense", category: "Restaurant", date: "2026-11-04" },
    { amountMinor: 800, currency: "EUR", direction: "expense", category: "Coffee", date: "2026-11-05" },
    { amountMinor: 50_000, currency: "EUR", direction: "transfer", category: "Funds", date: "2026-11-06", accountKind: "investment" },
  ],
  {
    display: "CHF",
    book,
    nowMs: now,
    start: "2026-11-01",
    end: "2026-11-30",
    group: "entry",
    threshold: 0.03,
    categories: [{ name: "Funds", color: "#4C78D8", kind: "investment" }],
  },
);
assert.equal(byEntry.state, "ready");
if (byEntry.state === "ready") {
  const salaries = byEntry.nodes.filter((node) => node.role === "source" && node.label.startsWith("Salary"));
  assert.equal(salaries.length, 2);
  assert.ok(salaries.some((node) => node.label.includes("1 Nov")));
  assert.ok(salaries.some((node) => node.label.includes("15 Nov")));
  assert.notEqual(salaries[0].amountMinor, 191_000);
  assert.ok(byEntry.nodes.some((node) => node.label.startsWith("Rent") && node.role === "source"));
  assert.ok(byEntry.nodes.some((node) => node.kind === "investment" && node.role === "hub"));
  assert.ok(byEntry.nodes.some((node) => node.kind === "savings"));
  assert.equal(byEntry.savingsMinor, byEntry.incomeMinor - byEntry.spendingMinor - byEntry.investmentMinor);
  assert.equal(byEntry.nodes.some((node) => node.role === "source" && node.label === "Spending"), false);
  assert.ok(flowConserved(byEntry));
  assert.match(describeFlow(byEntry), /Income /);
  assert.match(describeFlow(byEntry), /Spending /);
}

const unlinked = buildMoneyFlow(
  [
    { amountMinor: 26_186, currency: "CHF", direction: "payment", category: "", date: "2026-11-04", accountName: "" },
    { amountMinor: 26_186, currency: "CHF", direction: "expense", category: "Housing", date: "2026-11-05", accountName: "Swisscard", accountKind: "credit_card" },
  ],
  { display: "CHF", book, nowMs: now, start: "2026-11-01", end: "2026-11-30" },
);
assert.equal(unlinked.state, "ready");
if (unlinked.state === "ready") {
  assert.equal(unlinked.noIncome, true);
  assert.equal(unlinked.nodes.some((node) => node.role === "source" && node.label === "Spending"), false);
  assert.equal(unlinked.nodes.some((node) => node.role === "source" && node.label === "Swisscard"), false);
  assert.equal(unlinked.nodes.find((node) => node.kind === "drawn")?.label, "Shortfall / From savings");
  assert.equal(unlinked.nodes.some((node) => node.id === "pool"), false);
  assert.ok(unlinked.nodes.some((node) => node.label === "Housing" && node.role === "sink"));
  assert.ok(unlinked.nodes.some((node) => node.kind === "debt"));
  assert.ok(flowConserved(unlinked));
}

const phone = sankeyFrame(390, [2, 1, 1, 6], true, 720);
assert.equal(phone.nodeWidth, Math.round(Math.min(8, Math.max(3, 390 * 0.011))));
assert.ok(phone.nodeWidth >= 3 && phone.nodeWidth <= 5, `bar ${phone.nodeWidth}`);
assert.ok(phone.nodeGap >= 8, `gap ${phone.nodeGap}`);
assert.equal(phone.linkOpacity, 0.45);
assert.equal(phone.portrait, true);
assert.equal(phone.vertical, true);
assert.equal(phone.columnFill, 0.32);
assert.ok(phone.left <= 16 && phone.right <= 16, `gutters ${phone.left}/${phone.right}`);
assert.equal(phone.flowPx, Math.round(Math.min(48, Math.max(16, 390 * 0.055))));
assert.ok(phone.flowPx < phone.plot * 0.2, `ribbon ${phone.flowPx} plot ${phone.plot}`);
assert.equal(phone.layoutIterations, 0);
assert.equal(phone.compact, true);
assert.equal(phone.fontSize, 13);
assert.ok(phone.width - phone.left - phone.right >= 60);
assert.equal(phone.width, 390);
assert.ok(phone.plot >= 260 && phone.plot <= 440, `plot ${phone.plot}`);
const wide = sankeyFrame(900, [2, 1, 1, 6], true, 500);
assert.equal(wide.portrait, false);
assert.equal(wide.vertical, false);
assert.equal(wide.columnFill, 1);
assert.equal(wide.compact, false);
const landPhone = sankeyFrame(844, [4, 1, 1, 8], true, 390);
assert.equal(landPhone.portrait, false);
assert.equal(landPhone.columnFill, 1);
assert.equal(landPhone.compact, true);
assert.ok(landPhone.plot >= 8 * (landPhone.lineHeight * 3 + 8) - 1, `plot ${landPhone.plot}`);
assert.equal(wide.nodeWidth, 8);
assert.equal(wide.compact, false);
const narrow = sankeyFrame(360, [1, 1, 1, 4], true, 700);
assert.ok(narrow.portrait);
assert.ok(narrow.flowPx <= phone.flowPx, `narrow ribbon ${narrow.flowPx}`);
assert.ok(narrow.nodeWidth >= 3 && narrow.nodeWidth <= 5, `width ${narrow.nodeWidth}`);
assert.ok(narrow.plot >= 260);
const many = sankeyFrame(390, [1, 1, 8], true, 844);
assert.ok(many.plot > phone.plot, "more leaves get a taller plot");
assert.ok(many.nodeGap >= 8);
const spread = spreadColumn([420, 180], 88, 380, 122);
assert.equal(spread.length, 2);
assert.ok(spread[1].y > spread[0].y + spread[0].h, "slots stay in order");
assert.ok(spread[0].h + spread[1].h <= 130);
const one = spreadColumn([600], 88, 380, 122);
assert.equal(one[0].y, 88);
assert.ok(one[0].h <= 122);
const level = spreadLevel([6000, 2500, 1400], 300, 96, "slots");
assert.equal(level.length, 3);
assert.ok(level[1].x > level[0].x + level[0].w);
assert.ok(level.reduce((sum, box) => sum + box.w, 0) <= 100);
const single = spreadLevel([6000], 300, 96, "right");
assert.ok(single[0].x + single[0].w <= 300);
assert.ok(single[0].w <= 96);
assert.ok(single[0].x > 150);
const side = sankeyFrame(390, [2, 1, 1, 6], true, 720, "horizontal");
assert.equal(side.reference, true);
assert.equal(side.vertical, false);
assert.ok(side.width - side.left - side.right >= 90, `plot ${side.width - side.left - side.right}`);
const tablet = sankeyFrame(768, [2, 1, 1, 6], true, 1024);
assert.equal(tablet.portrait, true);
assert.equal(tablet.vertical, false);
assert.equal(flowAmountLabel(600000, "CHF", 1, true), "CHF 6k · 100%");
assert.equal(flowAmountLabel(140000, "CHF", 0.562, true), "CHF 1.4k · 56%");
assert.equal(flowAmountLabel(600000, "CHF", 1, false), "CHF 6,000.00 · 100%");
assert.equal(flowAmountLabel(140000, "eur", 0.56, false), "EUR 1,400.00 · 56%");
assert.ok(!flowAmountLabel(52370, "CHF", 1, false).includes("…"));

if (describedEmpty.state === "ready") {
  const built = buildSankeyOption({
    nodes: describedEmpty.nodes,
    links: describedEmpty.links,
    currency: "CHF",
    width: 390,
    height: 720,
    labels: true,
    values: true,
    colors: {},
    ink: "rgb(240,235,228)",
    muted: "rgb(168,160,150)",
    brand: "rgb(196,176,154)",
    paper: "rgb(23,24,27)",
    line: "rgb(90,84,76)",
    mono: false,
  });
  const series = (built.option.series as Array<Record<string, unknown>>)[0];
  assert.equal(series.nodeWidth, phone.nodeWidth);
  assert.ok(Number(series.nodeGap) >= 8);
  assert.equal((series.lineStyle as { opacity: number }).opacity, built.frame.linkOpacity);
  const labels = (series.data as Array<{ label: { formatter: string; show: boolean } }>).map((row) => row.label.formatter);
  assert.ok(labels.some((line) => line.includes("\n") && line.includes("CHF ") && line.includes("·")));
  assert.ok(labels.every((line) => !line.includes("…")));
  assert.ok(labels.every((line) => !line.includes("Total income")));
}

const sampleBoxes = placeFlowLabels(
  [
    { id: "salary", role: "source", kind: "income", text: "Salary\nCHF 6k · 100%", rect: { x: 112, y: 64, w: 10, h: 360 } },
    { id: "pool", role: "pool", kind: "pool", text: "Total income\nCHF 6k · 100%", rect: { x: 160, y: 64, w: 10, h: 360 } },
    { id: "spend", role: "hub", kind: "spending", text: "Total spending\nCHF 2.5k · 42%", rect: { x: 220, y: 64, w: 10, h: 200 } },
    { id: "save", role: "hub", kind: "savings", text: "Savings\nCHF 3.5k · 58%", rect: { x: 220, y: 288, w: 10, h: 136 } },
    { id: "housing", role: "sink", kind: "expense", text: "Housing\nCHF 1.4k · 56%", rect: { x: 280, y: 64, w: 10, h: 160 } },
    { id: "food", role: "sink", kind: "expense", text: "Food\nCHF 620 · 25%", rect: { x: 280, y: 248, w: 10, h: 70 } },
    { id: "transport", role: "sink", kind: "expense", text: "Transport\nCHF 280 · 11%", rect: { x: 280, y: 342, w: 10, h: 36 } },
    { id: "subs", role: "sink", kind: "expense", text: "Subscriptions\nCHF 190 · 8%", rect: { x: 280, y: 402, w: 10, h: 22 } },
  ],
  { width: 390, height: 460 },
  124,
);
assert.equal(labelsOverlap(sampleBoxes), false, JSON.stringify(sampleBoxes));
assert.equal(labelsInside(sampleBoxes, { width: 390, height: 460 }), true);

const lifted = sankeyValue(10, 10_000, phone.floorShare);
assert.ok(lifted > 10);
assert.equal(sankeyValue(9_000, 10_000, phone.floorShare), 9_000);

const slices = flowSlices([420, 180, 45], 64);
assert.equal(slices.length, 3);
assert.ok(slices[1].start >= slices[0].start + slices[0].extent - 0.01);
assert.ok(slices[2].start >= slices[1].start + slices[1].extent - 0.01);
assert.ok(Math.abs(slices.reduce((sum, slice) => sum + slice.extent, 0) - 64) < 1);
const leaves = leafOffsets(6, 140, 48, 520, 4);
assert.equal(leaves.length, 6);
for (let index = 1; index < leaves.length; index += 1) {
  assert.ok(leaves[index] >= leaves[index - 1] + 40, `leaf ${index} ${leaves[index]}`);
}
const landscape = sankeyFrame(844, [4, 1, 8], true, 390, "vertical");
assert.equal(landscape.portrait, false);
assert.equal(landscape.vertical, false);

const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const richRows = [
  { amountMinor: 420000, currency: "CHF", direction: "income", category: "Salary", date: "2026-11-01" },
  { amountMinor: 180000, currency: "CHF", direction: "income", category: "Salary", date: "2026-11-15" },
  { amountMinor: 45000, currency: "CHF", direction: "income", category: "Dividends", date: "2026-11-03" },
  { amountMinor: 8000, currency: "CHF", direction: "income", category: "Interest", date: "2026-11-04" },
  { amountMinor: 32000, currency: "CHF", direction: "income", category: "Side project", date: "2026-11-20" },
  { amountMinor: 140000, currency: "CHF", direction: "expense", category: "Housing", date: "2026-11-02" },
  { amountMinor: 62000, currency: "CHF", direction: "expense", category: "Food", date: "2026-11-05" },
  { amountMinor: 28000, currency: "CHF", direction: "expense", category: "Transport", date: "2026-11-06" },
  { amountMinor: 19000, currency: "CHF", direction: "expense", category: "Subscriptions", date: "2026-11-08" },
  { amountMinor: 24000, currency: "CHF", direction: "expense", category: "Health", date: "2026-11-09" },
  { amountMinor: 15000, currency: "CHF", direction: "expense", category: "Dining", date: "2026-11-11" },
  { amountMinor: 2000, currency: "CHF", direction: "expense", category: "Coffee", date: "2026-11-12" },
  { amountMinor: 1500, currency: "CHF", direction: "expense", category: "Tips", date: "2026-11-13" },
  { amountMinor: 80000, currency: "CHF", direction: "expense", category: "Funds", date: "2026-11-18" },
];
const richCategories = [
  { name: "Salary", color: "#3caa68", kind: "income" },
  { name: "Side project", color: "#125a36", kind: "income" },
  { name: "Funds", color: "#6a420c", kind: "investment" },
];
const deficitRows = [
  { amountMinor: 200000, currency: "CHF", direction: "income", category: "Salary", date: "2026-11-01" },
  { amountMinor: 15000, currency: "CHF", direction: "income", category: "Dividends", date: "2026-11-03" },
  { amountMinor: 140000, currency: "CHF", direction: "expense", category: "Housing", date: "2026-11-02" },
  { amountMinor: 80000, currency: "CHF", direction: "expense", category: "Food", date: "2026-11-05" },
  { amountMinor: 40000, currency: "CHF", direction: "expense", category: "Transport", date: "2026-11-06" },
  { amountMinor: 30000, currency: "CHF", direction: "expense", category: "Health", date: "2026-11-09" },
  { amountMinor: 22000, currency: "CHF", direction: "expense", category: "Dining", date: "2026-11-11" },
  { amountMinor: 25000, currency: "CHF", direction: "expense", category: "Funds", date: "2026-11-18" },
];

function assertChart(rows: typeof richRows, group: "category" | "entry", layout: "vertical" | "horizontal", labels: boolean) {
  const builtFlow = buildMoneyFlow(rows, {
    display: "CHF",
    book,
    nowMs: now,
    start: "2026-11-01",
    end: "2026-11-30",
    group,
    months,
    categories: richCategories,
    nodeColors: { Salary: "#3caa68", Funds: "#6a420c" },
  });
  assert.equal(builtFlow.state, "ready");
  if (builtFlow.state !== "ready") return;
  assert.equal(builtFlow.nodes.some((node) => node.kind === "savings" && node.amountMinor < 0), false);
  const expenseShares = builtFlow.nodes.filter((node) => node.role === "sink" && node.kind !== "investment").reduce((sum, node) => sum + node.share, 0);
  const incomeBase = builtFlow.incomeMinor || 1;
  assert.ok(Math.abs(expenseShares - builtFlow.spendingMinor / incomeBase) < 0.02, `shares ${expenseShares}`);
  const graph = portraitLevels(builtFlow.nodes, "CHF", "Spending");
  const hub = graph.nodes.find((node) => node.role === "pool");
  assert.ok(hub);
  if (!hub) return;
  const inbound = graph.links.filter((link) => link.target === hub.id).reduce((sum, link) => sum + link.amountMinor, 0);
  const outbound = graph.links.filter((link) => link.source === hub.id).reduce((sum, link) => sum + link.amountMinor, 0);
  assert.equal(inbound, outbound);
  assert.equal(inbound, hub.amountMinor);
  const option = buildSankeyOption({
    nodes: builtFlow.nodes,
    links: builtFlow.links,
    currency: "CHF",
    width: 390,
    height: 844,
    labels,
    values: true,
    colors: Object.fromEntries(builtFlow.nodes.map((node) => [node.id, node.color])),
    ink: "rgb(240,235,228)",
    muted: "rgb(168,160,150)",
    brand: "rgb(196,176,154)",
    paper: "rgb(23,24,27)",
    line: "rgb(90,84,76)",
    mono: false,
    portraitLayout: layout,
    spendingLabel: "Spending",
  });
  assert.equal(option.frame.vertical, layout === "vertical");
  assert.equal(option.frame.reference, layout === "horizontal");
  const rowsOut = (option.option.series as Array<{ data: Array<{ label: { formatter: string; show: boolean } }> }>)[0].data;
  const texts = rowsOut.map((row) => row.label.formatter);
  assert.ok(texts.every((line) => !line.includes("…")));
  if (labels) assert.ok(texts.some((line) => line.includes("\n") && line.includes("CHF ") && line.includes("·")));
  if (labels && group === "category") {
    assert.ok(texts.some((line) => line.includes("Side project") && line.includes("CHF ")));
    assert.ok(texts.some((line) => line.includes("Other income")));
  }
  if (!labels) assert.ok(rowsOut.every((row) => row.label.show === false));
}

for (const group of ["category", "entry"] as const) {
  for (const layout of ["vertical", "horizontal"] as const) {
    for (const labels of [true, false]) assertChart(richRows, group, layout, labels);
  }
}
const entryRich = buildMoneyFlow(richRows, {
  display: "CHF",
  book,
  nowMs: now,
  start: "2026-11-01",
  end: "2026-11-30",
  group: "entry",
  months,
  categories: richCategories,
  nodeColors: { Salary: "#3caa68" },
});
assert.equal(entryRich.state, "ready");
if (entryRich.state === "ready") {
  const salaries = entryRich.nodes.filter((node) => node.label.startsWith("Salary"));
  assert.equal(salaries.length, 2);
  assert.equal(salaries[0].color, "#3caa68");
  assert.equal(salaries[1].color, salaries[0].color);
}
const deficitFlow = buildMoneyFlow(deficitRows, {
  display: "CHF",
  book,
  nowMs: now,
  start: "2026-11-01",
  end: "2026-11-30",
  categories: richCategories,
  nodeColors: { Funds: "#6a420c" },
});
assert.equal(deficitFlow.state, "ready");
if (deficitFlow.state === "ready") {
  assert.ok(deficitFlow.savingsMinor < 0);
  assert.equal(deficitFlow.nodes.some((node) => node.kind === "savings"), false);
  assert.ok(deficitFlow.nodes.some((node) => node.kind === "drawn"));
  assert.ok(flowConserved(deficitFlow));
  const graph = portraitLevels(deficitFlow.nodes, "CHF", "Spending");
  const hub = graph.nodes.find((node) => node.role === "pool");
  assert.ok(hub);
  if (hub) {
    const inbound = graph.links.filter((link) => link.target === hub.id).reduce((sum, link) => sum + link.amountMinor, 0);
    const outbound = graph.links.filter((link) => link.source === hub.id).reduce((sum, link) => sum + link.amountMinor, 0);
    assert.equal(inbound, outbound);
    assert.equal(inbound, hub.amountMinor);
  }
}

console.log("money flow ok");
