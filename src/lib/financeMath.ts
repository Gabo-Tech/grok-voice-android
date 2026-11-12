export interface FlowInput {
  currency: string;
  direction: string;
  amountMinor: number;
  date: string;
}

export interface FlowRow {
  currency: string;
  incoming: number;
  outgoing: number;
  left: number;
}

/** Incoming, outgoing, and leftover for one period. Currencies are never combined. */
export function cashFlow(rows: FlowInput[], start: string, end: string): FlowRow[] {
  const by = new Map<string, FlowRow>();
  for (const tx of rows) {
    if (!tx.currency) continue;
    if (start && tx.date < start) continue;
    if (end && tx.date > end) continue;
    const row = by.get(tx.currency) ?? { currency: tx.currency, incoming: 0, outgoing: 0, left: 0 };
    if (tx.direction === "income") row.incoming += tx.amountMinor;
    else if (tx.direction === "expense" || tx.direction === "payment") row.outgoing += tx.amountMinor;
    row.left = row.incoming - row.outgoing;
    by.set(tx.currency, row);
  }
  return [...by.values()].sort((a, b) => a.currency.localeCompare(b.currency));
}

export interface PayoffAccount {
  id: string;
  kind: string;
  currency: string;
  shownBalanceMinor: number | null;
  balanceUpdatedMs: number;
  balanceAsOf: string;
}

export interface PayoffTx {
  accountId: string;
  fromAccountId: string;
  currency: string;
  direction: string;
  amountMinor: number;
  createdMs: number;
  date: string;
}

export interface Payoff {
  remaining: number;
  paid: number;
  ratio: number;
}

const DEBT = new Set(["credit_card", "loan", "other_debt"]);

/** Paid-down share of a debt. Payments after the stated balance count. No invented principal. */
export function debtPayoff(account: PayoffAccount, transactions: PayoffTx[]): Payoff | null {
  if (!DEBT.has(account.kind) || account.shownBalanceMinor == null) return null;
  const remaining = Math.max(0, account.shownBalanceMinor);
  let paid = 0;
  for (const tx of transactions) {
    const hits = tx.accountId === account.id || tx.fromAccountId === account.id;
    if (!hits || tx.direction !== "payment") continue;
    if (account.currency && tx.currency && tx.currency !== account.currency) continue;
    if (account.balanceUpdatedMs > 0 && tx.createdMs <= account.balanceUpdatedMs) continue;
    if (account.balanceAsOf && tx.date < account.balanceAsOf) continue;
    paid += tx.amountMinor;
  }
  const started = remaining + paid;
  const ratio = started <= 0 ? 0 : Math.min(1, paid / started);
  return { remaining, paid, ratio };
}

export interface DueRule {
  id: string;
  accountId: string;
  nextDate: string;
  amountMinor: number | null;
  currency: string;
}

export interface NextDue {
  id: string;
  date: string;
  amount: number | null;
  currency: string;
}

export function nextDue(
  account: { id: string; currency: string; dueDate: string; paymentMinor: number | null },
  rules: DueRule[],
): NextDue | null {
  const linked = rules
    .filter((rule) => rule.accountId === account.id && rule.nextDate)
    .sort((a, b) => a.nextDate.localeCompare(b.nextDate))[0];
  if (linked) {
    return { id: linked.id, date: linked.nextDate, amount: linked.amountMinor, currency: linked.currency || account.currency };
  }
  if (account.dueDate || account.paymentMinor != null) {
    return { id: "", date: account.dueDate, amount: account.paymentMinor, currency: account.currency };
  }
  return null;
}

/** Minor units. Crypto keeps extra places so a small coin amount is not rounded away. */
export function minorExp(currency: string): number {
  const code = currency.toUpperCase();
  if (code === "JPY" || code === "KRW" || code === "VND" || code === "CLP") return 0;
  if (code === "BHD" || code === "KWD" || code === "OMR") return 3;
  if (code === "BTC" || code === "ETH" || code === "XMR" || code === "SOL" || code === "LTC") return 8;
  return 2;
}

export function monthBounds(today: string): { start: string; end: string; prevStart: string; prevEnd: string } {
  const start = `${today.slice(0, 7)}-01`;
  const next = addMonths(start, 1);
  const end = addDays(next, -1);
  const prevStart = addMonths(start, -1);
  const prevEnd = addDays(start, -1);
  return { start, end, prevStart, prevEnd };
}

export function addDays(iso: string, days: number): string {
  const date = utcDate(iso);
  if (!date) return iso;
  date.setUTCDate(date.getUTCDate() + days);
  return isoFrom(date);
}

export function addMonths(iso: string, months: number): string {
  const part = parts(iso);
  if (!part) return iso;
  const index = part.y * 12 + (part.m - 1) + months;
  const year = Math.floor(index / 12);
  const month = ((index % 12) + 12) % 12;
  const dim = new Date(Date.UTC(year, month + 1, 0)).getUTCDate();
  return isoFrom(new Date(Date.UTC(year, month, Math.min(part.d, dim))));
}

function parts(iso: string): { y: number; m: number; d: number } | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (!match) return null;
  const y = Number(match[1]);
  const m = Number(match[2]);
  const d = Number(match[3]);
  const date = new Date(Date.UTC(y, m - 1, d));
  if (date.getUTCFullYear() !== y || date.getUTCMonth() !== m - 1 || date.getUTCDate() !== d) return null;
  return { y, m, d };
}

function utcDate(iso: string): Date | null {
  const part = parts(iso);
  return part ? new Date(Date.UTC(part.y, part.m - 1, part.d)) : null;
}

function isoFrom(date: Date): string {
  const y = date.getUTCFullYear();
  const m = String(date.getUTCMonth() + 1).padStart(2, "0");
  const d = String(date.getUTCDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

function daysBetween(start: string, end: string): number {
  const a = utcDate(start);
  const b = utcDate(end);
  if (!a || !b) return 0;
  return Math.round((b.getTime() - a.getTime()) / 86_400_000);
}

export type SchedulePurpose = "one_off" | "recurring" | "subscription" | "investment_payout";
export type AmountMode = "fixed" | "quote";

export interface ScheduleShape {
  id: string;
  name?: string;
  direction: string;
  purpose?: string;
  amountMinor: number | null;
  currency: string;
  amountMode?: string;
  payoutCurrency?: string;
  cadence: string;
  intervalDays?: number;
  startDate?: string;
  nextDate?: string;
  endKind?: string;
  endCount?: number | null;
  endDate?: string;
  status?: string;
  skippedDates?: string[];
  category?: string;
  accountId?: string;
}

export interface LinkedTx {
  ruleId?: string;
  occurrenceDate?: string;
  date: string;
  amountMinor: number;
  currency: string;
  direction: string;
}

interface Norm {
  id: string;
  name: string;
  direction: string;
  purpose: SchedulePurpose;
  amountMinor: number | null;
  currency: string;
  amountMode: AmountMode;
  payoutCurrency: string;
  cadence: string;
  intervalDays: number;
  start: string;
  endKind: "none" | "after_count" | "on_date";
  endCount: number | null;
  endDate: string;
  status: string;
  skipped: Set<string>;
  category: string;
  accountId: string;
}

function normSchedule(schedule: ScheduleShape): Norm {
  const cadence = (schedule.cadence || "").toLowerCase();
  const interval = schedule.intervalDays && schedule.intervalDays > 0 ? schedule.intervalDays : 0;
  const repeating = cadence === "daily" || cadence === "weekly" || cadence === "monthly" || cadence === "yearly" || (cadence === "every_n_days" && interval > 0);
  const purpose = (schedule.purpose || (repeating ? "recurring" : schedule.startDate || schedule.nextDate ? "one_off" : "recurring")) as SchedulePurpose;
  const start = schedule.startDate || schedule.nextDate || "";
  let endKind = (schedule.endKind || "") as Norm["endKind"];
  let endCount = schedule.endCount ?? null;
  if (!schedule.endKind) {
    if (purpose === "one_off" || !repeating) {
      endKind = "after_count";
      endCount = endCount && endCount > 0 ? endCount : 1;
    } else {
      endKind = "none";
    }
  }
  return {
    id: schedule.id,
    name: schedule.name || "",
    direction: schedule.direction || "expense",
    purpose: purpose === "one_off" || purpose === "subscription" || purpose === "investment_payout" ? purpose : "recurring",
    amountMinor: schedule.amountMinor,
    currency: (schedule.currency || "").toUpperCase(),
    amountMode: schedule.amountMode === "quote" ? "quote" : "fixed",
    payoutCurrency: (schedule.payoutCurrency || "").toUpperCase(),
    cadence: repeating ? cadence : "",
    intervalDays: interval,
    start,
    endKind: endKind === "after_count" || endKind === "on_date" ? endKind : "none",
    endCount,
    endDate: schedule.endDate || "",
    status: schedule.status || "active",
    skipped: new Set(schedule.skippedDates || []),
    category: schedule.category || "",
    accountId: schedule.accountId || "",
  };
}

function stepDate(iso: string, cadence: string, interval: number): string | null {
  if (cadence === "daily") return addDays(iso, 1);
  if (cadence === "weekly") return addDays(iso, 7);
  if (cadence === "monthly") return addMonths(iso, 1);
  if (cadence === "yearly") return addMonths(iso, 12);
  if (cadence === "every_n_days" && interval > 0) return addDays(iso, interval);
  return null;
}

function jumpTo(start: string, target: string, cadence: string, interval: number): string {
  if (start >= target) return start;
  if (cadence === "daily" || cadence === "weekly" || cadence === "every_n_days") {
    const span = cadence === "weekly" ? 7 : cadence === "daily" ? 1 : interval;
    if (span <= 0) return start;
    const steps = Math.floor(daysBetween(start, target) / span);
    const landed = addDays(start, steps * span);
    return landed < target ? addDays(landed, span) : landed;
  }
  let cursor = start;
  for (let i = 0; i < 800 && cursor < target; i += 1) {
    const next = stepDate(cursor, cadence, interval);
    if (!next || next <= cursor) return cursor;
    cursor = next;
  }
  return cursor;
}

/** Occurrence dates. Open-ended series stop at a 24-month window. Fixed runs include every remaining date. */
export function occurrenceDates(schedule: ScheduleShape, today: string): string[] {
  const spec = normSchedule(schedule);
  if (!spec.start) return [];
  if (!spec.cadence) return [spec.start];
  if (spec.endKind === "after_count" && spec.endCount && spec.endCount > 0) {
    const dates: string[] = [];
    let cursor = spec.start;
    for (let i = 0; i < Math.min(spec.endCount, 600); i += 1) {
      dates.push(cursor);
      const next = stepDate(cursor, spec.cadence, spec.intervalDays);
      if (!next || next <= cursor) break;
      cursor = next;
    }
    return dates;
  }
  const horizon = addMonths(today, 24);
  const floor = addMonths(today, -24);
  let end = horizon;
  if (spec.endKind === "on_date" && spec.endDate) end = spec.endDate;
  const begin = jumpTo(spec.start, spec.start > floor ? spec.start : floor, spec.cadence, spec.intervalDays);
  const dates: string[] = [];
  let cursor = spec.start < begin ? begin : spec.start;
  if (spec.start < floor && spec.endKind !== "on_date") cursor = begin;
  for (let i = 0; i < 800 && cursor <= end; i += 1) {
    if (cursor >= floor || spec.endKind === "on_date") dates.push(cursor);
    const next = stepDate(cursor, spec.cadence, spec.intervalDays);
    if (!next || next <= cursor) break;
    cursor = next;
  }
  return dates;
}

export interface Settlement {
  date: string;
  amountMinor: number;
  currency: string;
}

export function settlementsFor(schedule: ScheduleShape, txs: LinkedTx[], today: string): Settlement[] {
  const spec = normSchedule(schedule);
  const dates = occurrenceDates(schedule, today);
  const explicit = txs.filter((tx) => tx.ruleId === spec.id && tx.occurrenceDate);
  const used = new Set(explicit.map((tx) => tx.occurrenceDate as string));
  const rows: Settlement[] = explicit.map((tx) => ({
    date: tx.occurrenceDate as string,
    amountMinor: tx.amountMinor,
    currency: (tx.currency || spec.currency).toUpperCase(),
  }));
  const loose = txs.filter((tx) => tx.ruleId === spec.id && !tx.occurrenceDate);
  for (const tx of loose) {
    const open = dates.find((date) => !used.has(date));
    const date = open || tx.date;
    if (open) used.add(open);
    rows.push({ date, amountMinor: tx.amountMinor, currency: (tx.currency || spec.currency).toUpperCase() });
  }
  return rows;
}

export interface ScheduleProgress {
  mode: "fixed" | "open" | "once";
  paidCount: number;
  totalCount: number | null;
  remainingCount: number | null;
  remainingMinor: number | null;
  perPeriodMinor: number | null;
  currency: string;
  nearTermMinor: number | null;
  nearTermCount: number;
}

export function scheduleProgress(schedule: ScheduleShape, txs: LinkedTx[], today: string): ScheduleProgress {
  const spec = normSchedule(schedule);
  const dates = occurrenceDates(schedule, today);
  const settled = new Set(settlementsFor(schedule, txs, today).map((row) => row.date));
  const skipped = [...spec.skipped].filter((date) => dates.includes(date) && !settled.has(date));
  const paid = dates.filter((date) => settled.has(date)).length;
  const per = spec.amountMinor;
  const active = spec.status === "active" || spec.status === "";
  const horizon = addMonths(today, 12);
  const nearDates = active ? dates.filter((date) => date >= today && date < horizon && !settled.has(date) && !spec.skipped.has(date)) : [];
  if (spec.endKind === "after_count" && spec.endCount && spec.endCount > 0 && spec.cadence) {
    const remaining = Math.max(0, spec.endCount - paid - skipped.length);
    return {
      mode: "fixed",
      paidCount: paid,
      totalCount: spec.endCount,
      remainingCount: remaining,
      remainingMinor: per == null ? null : remaining * per,
      perPeriodMinor: per,
      currency: spec.currency,
      nearTermMinor: null,
      nearTermCount: 0,
    };
  }
  if (!spec.cadence) {
    const done = paid > 0 || skipped.length > 0;
    const remaining = done ? 0 : 1;
    return {
      mode: "once",
      paidCount: paid > 0 ? 1 : 0,
      totalCount: 1,
      remainingCount: remaining,
      remainingMinor: per == null ? null : remaining * per,
      perPeriodMinor: per,
      currency: spec.currency,
      nearTermMinor: null,
      nearTermCount: 0,
    };
  }
  return {
    mode: "open",
    paidCount: paid,
    totalCount: null,
    remainingCount: null,
    remainingMinor: null,
    perPeriodMinor: per,
    currency: spec.currency,
    nearTermMinor: !active || per == null ? (active ? null : 0) : nearDates.length * per,
    nearTermCount: nearDates.length,
  };
}

export interface PayoutTrack {
  nextDate: string | null;
  expectedCount: number;
  receivedCount: number;
  expectedMinor: number | null;
  expectedCurrency: string;
  payoutCurrency: string;
  received: { currency: string; amountMinor: number }[];
}

export function payoutTrack(schedule: ScheduleShape, txs: LinkedTx[], today: string): PayoutTrack {
  const spec = normSchedule(schedule);
  const dates = occurrenceDates(schedule, today);
  const rows = settlementsFor(schedule, txs, today);
  const settled = new Set(rows.map((row) => row.date));
  const expectedDates = dates.filter((date) => date <= today && !spec.skipped.has(date));
  const next = spec.status === "active" || spec.status === "" ? dates.find((date) => !settled.has(date) && !spec.skipped.has(date)) || null : null;
  const by = new Map<string, number>();
  for (const row of rows) by.set(row.currency, (by.get(row.currency) || 0) + row.amountMinor);
  return {
    nextDate: next,
    expectedCount: expectedDates.length,
    receivedCount: rows.length,
    expectedMinor: spec.amountMinor == null ? null : expectedDates.length * spec.amountMinor,
    expectedCurrency: spec.currency,
    payoutCurrency: spec.payoutCurrency,
    received: [...by.entries()].map(([currency, amountMinor]) => ({ currency, amountMinor })).sort((a, b) => a.currency.localeCompare(b.currency)),
  };
}

export interface PlannedHit {
  scheduleId: string;
  name: string;
  date: string;
  amountMinor: number | null;
  currency: string;
  direction: string;
  purpose: string;
  payoutCurrency: string;
  amountMode: AmountMode;
  category: string;
  accountId: string;
}

/** Unsettled occurrences inside the generation window. Settled dates are omitted so a marked payment is not counted twice. */
export function plannedHits(schedules: ScheduleShape[], txs: LinkedTx[], today: string): PlannedHit[] {
  const hits: PlannedHit[] = [];
  for (const schedule of schedules) {
    const spec = normSchedule(schedule);
    if (spec.status === "paused" || spec.status === "stopped" || spec.status === "completed") continue;
    const settled = new Set(settlementsFor(schedule, txs, today).map((row) => row.date));
    for (const date of occurrenceDates(schedule, today)) {
      if (settled.has(date) || spec.skipped.has(date)) continue;
      hits.push({
        scheduleId: spec.id,
        name: spec.name,
        date,
        amountMinor: spec.amountMinor,
        currency: spec.currency,
        direction: spec.direction,
        purpose: spec.purpose,
        payoutCurrency: spec.payoutCurrency,
        amountMode: spec.amountMode,
        category: spec.category,
        accountId: spec.accountId,
      });
    }
  }
  return hits.sort((a, b) => a.date.localeCompare(b.date) || a.name.localeCompare(b.name));
}

export interface ComingRow {
  id: string;
  name: string;
  date: string;
  amountMinor: number | null;
  currency: string;
  direction: string;
  purpose: string;
  payoutCurrency: string;
  amountMode: AmountMode;
  bucket: "overdue" | "week" | "month" | "later";
  progress: ScheduleProgress;
  track: PayoutTrack | null;
  category: string;
  accountId: string;
}

export function comingRows(schedules: ScheduleShape[], txs: LinkedTx[], today: string, side: "in" | "out"): ComingRow[] {
  const monthEnd = monthBounds(today).end;
  const weekEnd = addDays(today, 6);
  const rows: ComingRow[] = [];
  for (const schedule of schedules) {
    const spec = normSchedule(schedule);
    const outgoing = spec.direction === "expense" || spec.direction === "payment";
    if (side === "out" ? !outgoing : outgoing) continue;
    if (spec.status === "paused" || spec.status === "stopped" || spec.status === "completed") continue;
    const progress = scheduleProgress(schedule, txs, today);
    const track = spec.purpose === "investment_payout" ? payoutTrack(schedule, txs, today) : null;
    const hits = plannedHits([schedule], txs, today);
    const overdue = hits.filter((hit) => hit.date < today).slice(-6);
    const future = hits.filter((hit) => hit.date >= today).slice(0, spec.endKind === "after_count" ? 24 : 6);
    for (const hit of [...overdue, ...future]) {
      const bucket = hit.date < today ? "overdue" : hit.date <= weekEnd ? "week" : hit.date <= monthEnd ? "month" : "later";
      rows.push({
        id: spec.id,
        name: spec.name,
        date: hit.date,
        amountMinor: hit.amountMinor,
        currency: hit.currency,
        direction: spec.direction,
        purpose: spec.purpose,
        payoutCurrency: spec.payoutCurrency,
        amountMode: spec.amountMode,
        bucket,
        progress,
        track,
        category: spec.category,
        accountId: spec.accountId,
      });
    }
  }
  return rows.sort((a, b) => a.date.localeCompare(b.date) || a.name.localeCompare(b.name));
}

export interface Projection {
  currency: string;
  actualLeft: number;
  comingIn: number;
  comingOut: number;
  projected: number;
}

/**
 * Actual leftover plus planned items inside the window.
 * Overdue items are included only when asked. Dates outside the window are ignored, so an open-ended rent is not a lifetime sum.
 */
export function projectWindow(
  actual: { currency: string; left: number }[],
  planned: { currency: string; direction: string; amountMinor: number | null; date: string }[],
  from: string,
  to: string,
  today: string,
  includeOverdue = false,
): Projection[] {
  const by = new Map<string, Projection>();
  const ensure = (currency: string) => {
    const row = by.get(currency) ?? { currency, actualLeft: 0, comingIn: 0, comingOut: 0, projected: 0 };
    by.set(currency, row);
    return row;
  };
  for (const row of actual) {
    if (!row.currency) continue;
    ensure(row.currency).actualLeft += row.left;
  }
  for (const hit of planned) {
    if (!hit.currency || hit.amountMinor == null) continue;
    const overdue = hit.date < today;
    const inside = hit.date >= from && hit.date <= to;
    if (!inside && !(includeOverdue && overdue)) continue;
    const row = ensure(hit.currency);
    if (hit.direction === "income") row.comingIn += hit.amountMinor;
    else row.comingOut += hit.amountMinor;
  }
  for (const row of by.values()) row.projected = row.actualLeft + row.comingIn - row.comingOut;
  return [...by.values()].sort((a, b) => a.currency.localeCompare(b.currency));
}

export interface WorthAccount {
  kind: string;
  currency: string;
  shownBalanceMinor: number | null;
}

export interface WorthRow {
  currency: string;
  cashSavings: number;
  invested: number;
  owed: number;
  owned: number;
  net: number;
}

function worthBucket(kind: string): "cash" | "invest" | "debt" {
  if (kind === "credit_card" || kind === "loan" || kind === "other_debt" || kind === "card") return "debt";
  if (kind === "investment") return "invest";
  return "cash";
}

/** Point-in-time net worth from stated balances. Currencies stay apart. Investments use the entered value. */
export function netWorth(accounts: WorthAccount[]): WorthRow[] {
  const by = new Map<string, WorthRow>();
  for (const account of accounts) {
    if (account.shownBalanceMinor == null || !account.currency) continue;
    const row = by.get(account.currency) ?? { currency: account.currency, cashSavings: 0, invested: 0, owed: 0, owned: 0, net: 0 };
    const bucket = worthBucket(account.kind);
    if (bucket === "debt") row.owed += account.shownBalanceMinor;
    else if (bucket === "invest") row.invested += account.shownBalanceMinor;
    else row.cashSavings += account.shownBalanceMinor;
    row.owned = row.cashSavings + row.invested;
    row.net = row.owned - row.owed;
    by.set(account.currency, row);
  }
  return [...by.values()].sort((a, b) => a.currency.localeCompare(b.currency));
}

export interface WorthMove {
  currency: string;
  direction: string;
  amountMinor: number;
  date: string;
  accountKind: string;
  fromKind?: string;
}

/** How recorded activity moved net worth. Stated investment prices do not change unless a balance edit says so. */
export function netWorthChange(moves: WorthMove[], start: string, end: string): { currency: string; change: number }[] {
  const by = new Map<string, number>();
  for (const move of moves) {
    if (!move.currency) continue;
    if (start && move.date < start) continue;
    if (end && move.date > end) continue;
    let delta = 0;
    const primary = worthBucket(move.accountKind || "cash");
    if (primary === "cash" && move.direction === "income") delta += move.amountMinor;
    if (primary === "cash" && (move.direction === "expense" || move.direction === "payment")) delta -= move.amountMinor;
    if (primary === "debt" && move.direction === "expense") delta -= move.amountMinor;
    if (primary === "debt" && move.direction === "payment") delta += move.amountMinor;
    if (move.direction === "payment" && move.fromKind) {
      const source = worthBucket(move.fromKind);
      if (source === "cash" || source === "debt") delta -= move.amountMinor;
    }
    by.set(move.currency, (by.get(move.currency) || 0) + delta);
  }
  return [...by.entries()].map(([currency, change]) => ({ currency, change })).sort((a, b) => a.currency.localeCompare(b.currency));
}

export const STALE_AFTER_MS = 48 * 60 * 60 * 1000;

export interface RateBook {
  asOf: string;
  fetchedMs: number;
  perEur: Record<string, number>;
  manual: { base: string; quote: string; rate: number; asOf: string }[];
}

export function bookFromStatus(fx?: {
  asOf: string;
  fetchedMs: number;
  perEur?: { currency: string; perEur: number }[];
  manual?: { base: string; quote: string; rate: number; asOf: string }[];
} | null): RateBook | null {
  if (!fx) return null;
  const perEur: Record<string, number> = {};
  for (const leg of fx.perEur || []) {
    if (leg.currency && leg.perEur > 0) perEur[leg.currency.toUpperCase()] = leg.perEur;
  }
  if (Object.keys(perEur).length === 0 && (fx.manual || []).length === 0) return null;
  return { asOf: fx.asOf || "", fetchedMs: fx.fetchedMs || 0, perEur, manual: fx.manual || [] };
}

export interface Converted {
  amountMinor: number;
  currency: string;
  asOf: string;
  stale: boolean;
  source: "identity" | "manual" | "cached";
}

function bookStale(book: RateBook, nowMs: number): boolean {
  if (!book.fetchedMs) return Object.keys(book.perEur).length > 0;
  return nowMs - book.fetchedMs > STALE_AFTER_MS;
}

/** Display conversion. Missing rates return null. A stale cache is labeled and still usable. */
export function convertMinor(amountMinor: number, from: string, to: string, book: RateBook | null, nowMs: number): Converted | null {
  const sourceCode = from.toUpperCase();
  const target = to.toUpperCase();
  if (!sourceCode || !target) return null;
  if (sourceCode === target) {
    return { amountMinor, currency: target, asOf: "", stale: false, source: "identity" };
  }
  if (!book) return null;
  const manual = book.manual.find((row) => row.base.toUpperCase() === sourceCode && row.quote.toUpperCase() === target && row.rate > 0);
  const inverse = book.manual.find((row) => row.base.toUpperCase() === target && row.quote.toUpperCase() === sourceCode && row.rate > 0);
  let rate: number | null = null;
  let source: Converted["source"] = "cached";
  let asOf = book.asOf;
  if (manual) {
    rate = manual.rate;
    source = "manual";
    asOf = manual.asOf || book.asOf;
  } else if (inverse) {
    rate = 1 / inverse.rate;
    source = "manual";
    asOf = inverse.asOf || book.asOf;
  } else {
    const fromPer = sourceCode === "EUR" ? 1 : book.perEur[sourceCode];
    const toPer = target === "EUR" ? 1 : book.perEur[target];
    if (!fromPer || !toPer || fromPer <= 0 || toPer <= 0) return null;
    rate = toPer / fromPer;
  }
  if (rate == null || !Number.isFinite(rate) || rate <= 0) return null;
  const major = amountMinor / 10 ** minorExp(sourceCode);
  const minor = Math.round(major * rate * 10 ** minorExp(target));
  if (!Number.isFinite(minor)) return null;
  return { amountMinor: minor, currency: target, asOf, stale: source === "cached" && bookStale(book, nowMs), source };
}

export interface MoneyPart {
  amountMinor: number;
  currency: string;
}

/** Combined display total. One missing rate drops the total instead of inventing a 1:1. */
export function combinedMinor(parts: MoneyPart[], display: string, book: RateBook | null, nowMs: number): Converted | null {
  const target = display.toUpperCase();
  if (!target) return null;
  const usable = parts.filter((part) => part.currency && part.amountMinor !== 0);
  if (usable.length === 0) return null;
  let total = 0;
  let stale = false;
  let asOf = "";
  let source: Converted["source"] = "identity";
  for (const part of usable) {
    const converted = convertMinor(part.amountMinor, part.currency, target, book, nowMs);
    if (!converted) return null;
    total += converted.amountMinor;
    stale = stale || converted.stale;
    if (converted.asOf && (!asOf || converted.asOf < asOf)) asOf = converted.asOf;
    if (converted.source === "cached") source = "cached";
    else if (converted.source === "manual" && source !== "cached") source = "manual";
  }
  return { amountMinor: total, currency: target, asOf, stale, source };
}
