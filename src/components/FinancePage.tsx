import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { EmptyState, Notice, PageChrome, RowButton } from "@/components/PageChrome";
import { Sheet } from "@/components/Sheet";
import { Field } from "@/components/Field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { FinanceCarousel, type ChartPick } from "@/components/FinanceCharts";
import { HoldingsChart } from "@/components/HoldingsChart";
import { PriceSources } from "@/components/PriceSources";
import type { FinanceState } from "@/hooks/useFinance";
import type { FinanceAccount, FinanceRule, FinanceTransaction } from "@/lib/api";
import { formatQuantity, isHoldingAccount, originalHolding, positionOf } from "@/lib/financeLedger";
import { formatMoney, kindLabel, majorString, typeLabel } from "@/lib/money";
import { displayColor, filterTransactions, groupOf, isDebt, PALETTE, periodBounds, type PeriodId } from "@/lib/financeView";
import { appliedFilters, clearedFilters, withoutFilter, type FilterId, type LedgerFilters } from "@/lib/financeFilters";
import { bookFromStatus, cardDisplayMinor, cardDueDate, cardPosture, debtPayoff, headlineOf, nextDue, plannedHits, utilization } from "@/lib/financeMath";
import { FinanceDashboard, todayIso, useFinanceRateWatch, type WorthFocus } from "@/components/FinanceDashboard";
import { FinanceOverview } from "@/components/FinanceOverview";
import { FINANCE_TABS, readFinanceTab, type FinanceTab } from "@/lib/financeTabs";
import { ProgramSheet } from "@/components/ProgramSheet";
import { CASH_WALLET, PayWith } from "@/components/PayWith";
import { activeLocale, fill, useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import "@/finance.css";

type SheetKind = "transaction" | "account" | "investment" | "rule" | "pay";

const FINANCE_TAB_KEY = "grok-finance-tab";

function readStoredTab(): string | null {
  try {
    if (typeof window !== "undefined") {
      const asked = new URLSearchParams(window.location.search).get("finance");
      if (asked) return asked;
    }
    return sessionStorage.getItem(FINANCE_TAB_KEY);
  } catch {
    return null;
  }
}

const KINDS = ["bank", "cash", "credit_card", "loan", "other_debt", "other"] as const;
const PERIODS: PeriodId[] = ["this", "last", "six", "year", "all"];
const LEADS = [0, 1, 2, 3, 7] as const;
const CADENCE = ["", "weekly", "monthly", "yearly"] as const;

function tabLabel(tab: FinanceTab): string {
  if (tab === "overview") return "Overview";
  if (tab === "activity") return "Activity";
  if (tab === "accounts") return "Accounts";
  if (tab === "holdings") return "Holdings";
  return "Plan";
}

function swipeBlocked(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(target.closest("[data-swipe-ignore], input, textarea, select, [role='slider'], [role='dialog']"));
}

export function FinancePage({
  finance,
  active = true,
  onOpenCurrency,
}: {
  finance: FinanceState;
  active?: boolean;
  onOpenCurrency?: () => void;
}) {
  const t = useT();
  useFinanceRateWatch(finance);
  const rateDay = useRef(todayIso());
  const [tab, setTab] = useState<FinanceTab>(() => readFinanceTab(readStoredTab()));
  const [sheet, setSheet] = useState<SheetKind | null>(null);
  const [priceAccount, setPriceAccount] = useState<FinanceAccount | null>(null);
  const [editingAccount, setEditingAccount] = useState<FinanceAccount | null>(null);
  const [editingTx, setEditingTx] = useState<FinanceTransaction | null>(null);
  const [editingRule, setEditingRule] = useState<FinanceRule | null>(null);
  const [paying, setPaying] = useState<FinanceAccount | null>(null);
  const [classFilter, setClassFilter] = useState("all");
  const [directionFilter, setDirectionFilter] = useState("all");
  const [query, setQuery] = useState("");
  const [period, setPeriod] = useState<PeriodId>("this");
  const [focusMonth, setFocusMonth] = useState("");
  const [accountFilter, setAccountFilter] = useState("all");
  const [categoryFilter, setCategoryFilter] = useState("all");
  const [currencyFilter, setCurrencyFilter] = useState("all");
  const [groupFilter, setGroupFilter] = useState("all");
  const [exportNote, setExportNote] = useState("");
  const [categorySheet, setCategorySheet] = useState(false);
  const [accountPreset, setAccountPreset] = useState("cash");
  const [txPreset, setTxPreset] = useState("expense");
  const [groupFocus, setGroupFocus] = useState<WorthFocus | "all">("all");

  const accounts = finance.snapshot.accounts;
  const rules = finance.snapshot.rules.filter((rule) => rule.cadence || rule.nextDate);
  const bounds = useMemo(() => periodBounds(period, new Date(), focusMonth), [period, focusMonth]);
  const activity = useMemo(
    () =>
      filterTransactions(finance.snapshot.transactions, {
        start: bounds.start,
        end: bounds.end,
        accountId: accountFilter,
        category: categoryFilter,
        currency: currencyFilter,
        direction: directionFilter,
        recurrence: classFilter,
        group: groupFilter,
        accounts,
      }),
    [finance.snapshot.transactions, bounds, accountFilter, categoryFilter, currencyFilter, directionFilter, classFilter, groupFilter, accounts],
  );
  const today = todayIso();
  const planned = useMemo(() => {
    return plannedHits(finance.snapshot.rules, finance.snapshot.transactions, today).filter((hit) => {
      if (bounds.start && hit.date < bounds.start) return false;
      if (bounds.end && hit.date > bounds.end) return false;
      if (accountFilter !== "all" && hit.accountId !== accountFilter) return false;
      if (categoryFilter !== "all" && hit.category !== categoryFilter) return false;
      if (currencyFilter !== "all" && hit.currency !== currencyFilter) return false;
      if (directionFilter !== "all" && hit.direction !== directionFilter) return false;
      if (classFilter === "once" && hit.purpose !== "one_off") return false;
      if (classFilter === "recurring" && hit.purpose === "one_off") return false;
      return true;
    });
  }, [finance.snapshot.rules, finance.snapshot.transactions, today, bounds, accountFilter, categoryFilter, currencyFilter, directionFilter, classFilter]);
  const chartAccounts = useMemo(
    () =>
      accounts.filter((account) => {
        if (accountFilter !== "all" && account.id !== accountFilter) return false;
        if (currencyFilter !== "all" && account.currency !== currencyFilter) return false;
        if (groupFilter !== "all" && groupOf(account.kind) !== groupFilter) return false;
        return true;
      }),
    [accounts, accountFilter, currencyFilter, groupFilter],
  );
  const currencies = useMemo(() => [...new Set(accounts.map((account) => account.currency).filter(Boolean))], [accounts]);
  const categories = useMemo(() => {
    const names = new Set(finance.snapshot.categories.map((category) => category.name));
    for (const tx of finance.snapshot.transactions) if (tx.category) names.add(tx.category);
    return [...names];
  }, [finance.snapshot.categories, finance.snapshot.transactions]);
  const pickChart = (pick: ChartPick) => {
    if (pick.accountId) setAccountFilter(pick.accountId);
    if (pick.category) setCategoryFilter(pick.category);
    if (pick.currency) setCurrencyFilter(pick.currency);
    if (pick.group) setGroupFilter(pick.group);
    if (pick.month) setFocusMonth(pick.month);
  };
  const chooseTab = (next: FinanceTab, focus?: WorthFocus | "all") => {
    setTab(next);
    try {
      sessionStorage.setItem(FINANCE_TAB_KEY, next);
    } catch {
      /* The tab still changes for this render when storage is blocked. */
    }
    if (next === "activity") setDirectionFilter("all");
    if (next === "accounts") setGroupFocus(focus ?? "all");
    document.getElementById(`finance-tab-${next}`)?.scrollIntoView({ inline: "nearest", block: "nearest" });
  };
  const swipeRef = useRef<HTMLDivElement>(null);
  const dragOrigin = useRef<{ x: number; y: number; locked: "x" | "y" | null } | null>(null);
  const [dragX, setDragX] = useState(0);
  const [dragging, setDragging] = useState(false);
  const tabIndex = Math.max(0, FINANCE_TABS.indexOf(tab));
  const reducedMotion = typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const exportSummary = () => {
    const account = accounts.find((row) => row.id === accountFilter);
    void finance
      .exportPdf({
        period: focusMonth || period,
        account: account?.name || "",
        category: categoryFilter === "all" ? "" : categoryFilter,
        currency: currencyFilter === "all" ? "" : currencyFilter,
        direction: directionFilter === "all" ? "" : directionFilter,
        locale: activeLocale(),
      })
      .then((file) => {
        setExportNote(file.shared ? t("Share sheet opened. Nothing was uploaded.") : t("PDF stays on this phone until you share it."));
        if (!file.shared && file.dataB64) downloadBase64(file.fileName, file.dataB64);
      })
      .catch(() => setExportNote(t("Could not write the PDF.")));
  };

  useEffect(() => {
    const onVoice = (event: Event) => {
      const detail = (event as CustomEvent<{ view?: string }>).detail || {};
      if (!detail.view) return;
      const next = readFinanceTab(detail.view);
      chooseTab(next === "overview" && detail.view === "charts" ? "overview" : next);
    };
    window.addEventListener("grok-finance-voice", onVoice);
    return () => window.removeEventListener("grok-finance-voice", onVoice);
  }, []);

  useEffect(() => {
    const shot = new URLSearchParams(window.location.search).get("shot");
    if (!shot) return;
    const node = document.querySelector(`[data-shot="${shot}"]`);
    node?.scrollIntoView({ block: "start" });
  }, [tab]);

  useEffect(() => {
    if (!active || !finance.inApp) return;
    rateDay.current = todayIso();
    finance.refreshRates();
    const id = window.setInterval(() => {
      const next = todayIso();
      if (next === rateDay.current) return;
      rateDay.current = next;
      finance.refreshRates();
    }, 30_000);
    return () => window.clearInterval(id);
  }, [active, finance.inApp, finance.refreshRates]);

  const openAdd = () => {
    setEditingAccount(null);
    setEditingTx(null);
    setEditingRule(null);
    if (tab === "activity") {
      setTxPreset("expense");
      setSheet("transaction");
    } else if (tab === "holdings") {
      setSheet("investment");
    } else if (tab === "accounts") {
      if (groupFocus === "invested") setSheet("investment");
      else {
        setAccountPreset(groupFocus === "debt" ? "credit_card" : groupFocus === "cash" ? "cash" : "bank");
        setSheet("account");
      }
    } else if (tab === "overview") {
      setSheet("rule");
    }
  };
  const openBank = () => {
    setEditingAccount(null);
    setAccountPreset("bank");
    setSheet("account");
  };
  const openCard = () => {
    setEditingAccount(null);
    setAccountPreset("credit_card");
    setSheet("account");
  };
  const openIncome = () => {
    setEditingTx(null);
    setTxPreset("income");
    setSheet("transaction");
  };
  const openBill = () => {
    setEditingRule(null);
    setSheet("rule");
  };
  const openInvestment = () => {
    setEditingAccount(null);
    setSheet("investment");
  };
  const openAccount = (account: FinanceAccount) => {
    setEditingAccount(account);
    setSheet(isHoldingAccount(account) ? "investment" : "account");
  };
  const openTransaction = (tx: FinanceTransaction) => {
    setEditingTx(tx);
    setSheet("transaction");
  };
  const openRule = (rule: FinanceRule) => {
    setEditingRule(rule);
    setSheet("rule");
  };

  const addLabel = tab === "overview" || tab === "plan" ? "Program" : tab === "accounts" ? (groupFocus === "invested" ? "Add investment" : "Add account") : tab === "holdings" ? "Add investment" : "Add expense";
  const searched = query.trim()
    ? activity.filter((tx) => `${tx.category} ${tx.notes} ${tx.currency} ${tx.direction}`.toLowerCase().includes(query.trim().toLowerCase()))
    : activity;
  const ledgerRules = (classFilter === "all" || classFilter === "recurring" ? rules : []).filter((rule) => rule.purpose !== "investment_payout" || tab !== "activity");
  const empty = accounts.length === 0 && finance.snapshot.transactions.length === 0 && finance.snapshot.rules.length === 0;

  const ledger = (lock: "expense" | "income" | "any") => (
    <div className="space-y-4">
      <input className="mb-3 min-h-12 w-full border border-border bg-transparent px-3 text-base" value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t("Search")} aria-label={t("Search")} />
      <Activity
        rows={searched}
        rules={ledgerRules}
        classFilter={classFilter}
        directionFilter={directionFilter}
        directionLock={lock}
        onClass={setClassFilter}
        onDirection={setDirectionFilter}
        period={period}
        onPeriod={(next) => {
          setFocusMonth("");
          setPeriod(next);
        }}
        accountFilter={accountFilter}
        onAccountFilter={setAccountFilter}
        categoryFilter={categoryFilter}
        onCategory={setCategoryFilter}
        currencyFilter={currencyFilter}
        onCurrency={setCurrencyFilter}
        categories={categories}
        currencies={currencies}
        onEditCategories={() => setCategorySheet(true)}
        onAdd={openAdd}
        onTransaction={openTransaction}
        onRule={openRule}
        planned={planned}
        showPlanned={false}
        heading={t("Activity")}
        accounts={accounts}
      />
    </div>
  );

  return (
    <div className="finance flex min-h-0 flex-1 flex-col">
      <PageChrome
        title={t("Finance")}
        subtitle={t("On this phone. No bank login.")}
        onAdd={openAdd}
        addLabel={t(addLabel)}
        active={active}
      >
        {finance.error && <Notice title={t(finance.error)} onDismiss={() => finance.setError(null)} />}
        <div role="tablist" aria-label={t("Finance")} className="fin-tabs sticky top-0 z-10">
          {FINANCE_TABS.map((id) => (
            <button
              key={id}
              type="button"
              role="tab"
              id={`finance-tab-${id}`}
              aria-selected={tab === id}
              aria-controls={`finance-panel-${id}`}
              className="fin-tab"
              onClick={() => chooseTab(id)}
            >
              {t(tabLabel(id))}
            </button>
          ))}
          <span
            className="fin-tab-indicator"
            style={{
              transform: `translateX(${(tabIndex - dragX / (swipeRef.current?.clientWidth || 1)) * 100}%)`,
              transition: dragging || reducedMotion ? "none" : "transform 180ms var(--ease, ease)",
            }}
          />
        </div>
        <div
          ref={swipeRef}
          className="fin-swipe"
          onPointerDown={(event) => {
            if (event.clientX < 24 || event.clientX > window.innerWidth - 24) return;
            if (swipeBlocked(event.target)) return;
            dragOrigin.current = { x: event.clientX, y: event.clientY, locked: null };
          }}
          onPointerMove={(event) => {
            const origin = dragOrigin.current;
            if (!origin) return;
            const dx = event.clientX - origin.x;
            const dy = event.clientY - origin.y;
            if (!origin.locked) {
              if (Math.abs(dx) < 12 && Math.abs(dy) < 12) return;
              origin.locked = Math.abs(dx) > Math.abs(dy) ? "x" : "y";
              if (origin.locked === "y") {
                dragOrigin.current = null;
                return;
              }
              setDragging(true);
            }
            const width = swipeRef.current?.clientWidth || 1;
            const atStart = tabIndex === 0 && dx > 0;
            const atEnd = tabIndex === FINANCE_TABS.length - 1 && dx < 0;
            setDragX(atStart || atEnd ? dx * 0.15 : Math.max(-width, Math.min(width, dx)));
          }}
          onPointerUp={() => {
            const origin = dragOrigin.current;
            dragOrigin.current = null;
            if (!origin || origin.locked !== "x") {
              setDragging(false);
              setDragX(0);
              return;
            }
            const width = swipeRef.current?.clientWidth || 1;
            if (dragX <= -width * 0.22 && tabIndex < FINANCE_TABS.length - 1) chooseTab(FINANCE_TABS[tabIndex + 1]);
            else if (dragX >= width * 0.22 && tabIndex > 0) chooseTab(FINANCE_TABS[tabIndex - 1]);
            setDragging(false);
            setDragX(0);
          }}
          onPointerCancel={() => {
            dragOrigin.current = null;
            setDragging(false);
            setDragX(0);
          }}
        >
          <div
            className="fin-swipe-track"
            style={{
              transform: `translate3d(calc(${-tabIndex * 100}% + ${dragX}px), 0, 0)`,
              transition: dragging || reducedMotion ? "none" : "transform 180ms var(--ease, ease)",
            }}
          >
          <div role="tabpanel" id="finance-panel-overview" aria-labelledby="finance-tab-overview" data-finance-tabpanel="overview" className="fin-swipe-page" inert={tab === "overview" ? undefined : true}>
            <FinanceOverview
              finance={finance}
              accounts={accounts}
              onFocus={(focus) => (focus === "invested" ? chooseTab("holdings") : chooseTab("accounts", focus))}
              onOpenActivity={() => chooseTab("activity")}
              onRefreshPrices={() => {
                const book = bookFromStatus(finance.snapshot.fx);
                const quotes = finance.snapshot.prices?.quotes || [];
                const display = (finance.snapshot.defaultCurrency || "").toUpperCase();
                for (const account of accounts) {
                  if (!isHoldingAccount(account)) continue;
                  const position = positionOf(account, display, book, quotes, Date.now());
                  if (position.unpriced) finance.refreshHolding(account.id);
                }
              }}
            />
            {empty ? (
              <div className="fin-stack mt-4 w-full min-w-0 space-y-3">
                <StartCard title={t("Add a bank")} body={t("A bank or wallet you spend from.")} onClick={openBank} />
                <StartCard title={t("Add a card")} body={t("A card or loan. Payments reduce what you owe.")} onClick={openCard} />
                <StartCard title={t("Add income")} body={t("Money coming in.")} onClick={openIncome} />
                <StartCard title={t("Add a bill")} body={t("Rent, a subscription, or anything with a date.")} onClick={openBill} />
              </div>
            ) : null}
            <div className="mt-4 flex flex-col gap-2">
              <button
                type="button"
                className="fin-primary"
                onClick={() => {
                  setTxPreset("expense");
                  setEditingTx(null);
                  setSheet("transaction");
                }}
              >
                {t("Add expense")}
              </button>
              <div className="flex flex-wrap gap-x-4">
                <button type="button" className="fin-chip" onClick={openIncome}>{t("Add income")}</button>
                <button type="button" className="fin-chip" onClick={openBank}>{t("Add account")}</button>
                <button type="button" className="fin-chip" onClick={openBill}>{t("Program a bill")}</button>
              </div>
            </div>
            <button type="button" className="fin-chip mt-4 w-full" onClick={exportSummary}>{t("Export PDF")}</button>
            <div className="mt-6" data-finance-charts data-shot="donut">
              <FinanceCarousel
                accounts={chartAccounts}
                transactions={finance.snapshot.transactions}
                rules={finance.snapshot.rules}
                display={finance.snapshot.defaultCurrency}
                fx={finance.snapshot.fx}
                prices={finance.snapshot.prices}
                busy={finance.busy}
                onOpenSettings={onOpenCurrency}
                onPick={pickChart}
              />
            </div>
          </div>
          <div role="tabpanel" id="finance-panel-activity" aria-labelledby="finance-tab-activity" data-finance-tabpanel="activity" className="fin-swipe-page" inert={tab === "activity" ? undefined : true}>
            <FinanceDashboard
              finance={finance}
              accounts={accounts}
              place="activity"
              onProgram={openBill}
              onIncome={openIncome}
              onFocus={(focus) => (focus === "invested" ? chooseTab("holdings") : chooseTab("accounts", focus))}
            >
              {ledger("any")}
            </FinanceDashboard>
          </div>
          <div role="tabpanel" id="finance-panel-accounts" aria-labelledby="finance-tab-accounts" data-finance-tabpanel="accounts" className="fin-swipe-page" inert={tab === "accounts" ? undefined : true}>
            <AccountsBoard
              accounts={accounts}
              focus={groupFocus}
              transactions={finance.snapshot.transactions}
              rules={finance.snapshot.rules}
              onPaid={finance.markPaid}
              onOpen={openAccount}
              onPay={(account) => {
                setPaying(account);
                setSheet("pay");
              }}
              onAddAccount={() => {
                setEditingAccount(null);
                setAccountPreset("bank");
                setSheet("account");
              }}
              onAddCash={() => {
                setEditingAccount(null);
                setAccountPreset("cash");
                setSheet("account");
              }}
              onAddDebt={() => {
                setEditingAccount(null);
                setAccountPreset("credit_card");
                setSheet("account");
              }}
              onAddLoan={() => {
                setEditingAccount(null);
                setAccountPreset("loan");
                setSheet("account");
              }}
              display={finance.snapshot.defaultCurrency}
              fx={finance.snapshot.fx}
              quotes={finance.snapshot.prices?.quotes || []}
            />
          </div>
          <div role="tabpanel" id="finance-panel-holdings" aria-labelledby="finance-tab-holdings" data-finance-tabpanel="holdings" className="fin-swipe-page" inert={tab === "holdings" ? undefined : true}>
            <AccountList
              rows={accounts.filter((account) => isHoldingAccount(account))}
              transactions={finance.snapshot.transactions}
              rules={finance.snapshot.rules}
              onPaid={finance.markPaid}
              emptyTitle={t("Add investment")}
              emptyBody={t("A coin, a metal, a fund, or something you value yourself.")}
              action={t("Add investment")}
              onAdd={openInvestment}
              onOpen={openAccount}
              onRefreshPrice={(account) => finance.refreshHolding(account.id)}
              onDiagnose={setPriceAccount}
              priceNote={finance.snapshot.prices?.lastError || ""}
              display={finance.snapshot.defaultCurrency}
              fx={finance.snapshot.fx}
              quotes={finance.snapshot.prices?.quotes || []}
            />
            <div className="mt-6" data-shot="holdings-chart">
              <HoldingsChart
                accounts={accounts}
                display={finance.snapshot.defaultCurrency}
                fx={finance.snapshot.fx}
                prices={finance.snapshot.prices}
                history={finance.snapshot.history || []}
                onRecolor={(account, color) => finance.saveAccount({ id: account.id, name: account.name, kind: account.kind, color })}
                onUnpriced={setPriceAccount}
              />
            </div>
          </div>
          <div role="tabpanel" id="finance-panel-plan" aria-labelledby="finance-tab-plan" data-finance-tabpanel="plan" className="fin-swipe-page" inert={tab === "plan" ? undefined : true}>
            <FinanceDashboard finance={finance} accounts={accounts} place="plan" onProgram={openBill} onIncome={openIncome} onPayout={openBill} onFocus={(focus) => (focus === "invested" ? chooseTab("holdings") : chooseTab("accounts", focus))} />
          </div>
          </div>
        </div>
        {exportNote ? <p className="fin-muted mt-3 text-sm leading-5">{exportNote}</p> : null}
      </PageChrome>
      {tab === "activity" || tab === "plan" ? (
        <div className="fin-actions">
          <button type="button" className="fin-chip" onClick={openAdd}>
            {t("Add expense")}
          </button>
          <button type="button" className="fin-chip" onClick={openIncome}>
            {t("Add income")}
          </button>
          <button type="button" className="fin-chip" onClick={openBill}>
            {t("Program a bill")}
          </button>
        </div>
      ) : null}
      <TransactionSheet
        open={sheet === "transaction"}
        finance={finance}
        editing={editingTx}
        onClose={() => setSheet(null)}
        presetDirection={txPreset}
      />
      <AccountSheet
        open={sheet === "account" || sheet === "investment"}
        investment={sheet === "investment"}
        finance={finance}
        editing={editingAccount}
        presetKind={accountPreset}
        onClose={() => setSheet(null)}
      />
      <ProgramSheet open={sheet === "rule"} finance={finance} editing={editingRule} onClose={() => setSheet(null)} />
      <PriceSources
        open={priceAccount != null}
        account={priceAccount ? finance.snapshot.accounts.find((row) => row.id === priceAccount.id) || priceAccount : null}
        attempts={finance.snapshot.prices?.attempts || []}
        onClose={() => setPriceAccount(null)}
        onRefresh={(account) => finance.refreshHolding(account.id)}
        onSave={(account, coingeckoId, paprikaId) => finance.setHoldingProvider(account.id, coingeckoId, paprikaId)}
      />
      <PayCardSheet open={sheet === "pay"} finance={finance} card={paying} onClose={() => setSheet(null)} />
      <CategorySheet open={categorySheet} finance={finance} onClose={() => setCategorySheet(false)} />
    </div>
  );
}

function StartCard({ title, body, onClick }: { title: string; body: string; onClick: () => void }) {
  return (
    <button type="button" className="fin-panel w-full min-h-28 text-left" onClick={onClick}>
      <p className="text-base font-extrabold tracking-wide uppercase">{title}</p>
      <p className="mt-2 text-sm leading-5 text-muted-foreground">{body}</p>
    </button>
  );
}

function Activity({
  rows,
  rules,
  classFilter,
  directionFilter,
  onClass,
  onDirection,
  period,
  onPeriod,
  accountFilter,
  onAccountFilter,
  categoryFilter,
  onCategory,
  currencyFilter,
  onCurrency,
  groupFilter = "all",
  onGroup,
  categories,
  currencies,
  onEditCategories,
  onAdd,
  onTransaction,
  onRule,
  planned,
  accounts,
  directionLock = "any",
  filtersOnly = false,
  showPlanned = true,
  heading = "",
}: {
  rows: FinanceTransaction[];
  rules: FinanceRule[];
  classFilter: string;
  directionFilter: string;
  onClass: (value: string) => void;
  onDirection: (value: string) => void;
  directionLock?: "expense" | "income" | "any";
  filtersOnly?: boolean;
  showPlanned?: boolean;
  heading?: string;
  period: PeriodId;
  onPeriod: (value: PeriodId) => void;
  accountFilter: string;
  onAccountFilter: (value: string) => void;
  categoryFilter: string;
  onCategory: (value: string) => void;
  currencyFilter: string;
  onCurrency: (value: string) => void;
  groupFilter?: string;
  onGroup?: (value: string) => void;
  categories: string[];
  currencies: string[];
  onEditCategories: () => void;
  onAdd: () => void;
  onTransaction: (tx: FinanceTransaction) => void;
  onRule: (rule: FinanceRule) => void;
  planned: { scheduleId: string; name: string; date: string; amountMinor: number | null; currency: string }[];
  accounts: FinanceAccount[];
}) {
  const t = useT();
  const [filtersOpen, setFiltersOpen] = useState(false);
  const filters: LedgerFilters = {
    account: accountFilter,
    category: categoryFilter,
    currency: currencyFilter,
    recurrence: classFilter,
    direction: directionFilter,
    group: groupFilter,
  };
  const applied = appliedFilters(
    filters,
    {
      account: accounts.find((account) => account.id === accountFilter)?.name || accountFilter,
      category: categoryFilter,
      currency: currencyFilter,
      recurrence: t(classLabel(classFilter)),
      direction: t(directionLabel(directionFilter)),
      group: t(groupLabel(groupFilter)),
    },
    directionLock,
  );
  const clearOne = (id: FilterId) => {
    const next = withoutFilter(filters, id, directionLock);
    if (id === "account") onAccountFilter(next.account);
    if (id === "category") onCategory(next.category);
    if (id === "currency") onCurrency(next.currency);
    if (id === "recurrence") onClass(next.recurrence);
    if (id === "direction") onDirection(next.direction);
    if (id === "group") onGroup?.(next.group);
  };
  const clearAll = () => {
    const next = clearedFilters(directionLock);
    onAccountFilter(next.account);
    onCategory(next.category);
    onCurrency(next.currency);
    onClass(next.recurrence);
    onDirection(next.direction);
    onGroup?.("all");
  };
  return (
    <div className="min-w-0">
      <div className="chip-scroller mb-2">
        {PERIODS.map((id) => (
          <Chip key={id} on={period === id} onClick={() => onPeriod(id)}>{t(periodLabel(id))}</Chip>
        ))}
      </div>
      <div className="mb-2 flex min-w-0 items-center gap-2">
        <button type="button" className="fin-chip shrink-0" aria-expanded={filtersOpen} onClick={() => setFiltersOpen(true)}>
          {applied.length > 0 ? `${t("Filters")} ${applied.length}` : t("Filters")}
        </button>
      </div>
      {applied.length > 0 ? (
        <div className="chip-scroller mb-3">
          {applied.map((chip) => (
            <button key={chip.id} type="button" className="fin-chip" onClick={() => clearOne(chip.id)} aria-label={`${t("Remove filter")}: ${chip.label}`}>
              {chip.label} ×
            </button>
          ))}
        </div>
      ) : null}
      {typeof document !== "undefined"
        ? createPortal(
      <Sheet open={filtersOpen} title={t("Filters")} onClose={() => setFiltersOpen(false)} className="finance" footer={
        <Button type="button" variant="outline" className="w-full" onClick={clearAll}>{t("Clear filters")}</Button>
      }>
        <div className="space-y-5">
          <FilterGroup label={t("Accounts")}>
            <Chip on={accountFilter === "all"} onClick={() => onAccountFilter("all")}>{t("All accounts")}</Chip>
            {accounts.map((account) => (
              <Chip key={account.id} on={accountFilter === account.id} onClick={() => onAccountFilter(account.id)}>{account.name}</Chip>
            ))}
          </FilterGroup>
          <FilterGroup label={t("Categories")}>
            <Chip on={categoryFilter === "all"} onClick={() => onCategory("all")}>{t("All categories")}</Chip>
            {categories.map((category) => (
              <Chip key={category} on={categoryFilter === category} onClick={() => onCategory(category)}>{category}</Chip>
            ))}
            <Chip on={false} onClick={onEditCategories}>{t("Edit categories")}</Chip>
          </FilterGroup>
          <FilterGroup label={t("Currencies")}>
            <Chip on={currencyFilter === "all"} onClick={() => onCurrency("all")}>{t("All currencies")}</Chip>
            {currencies.map((currency) => (
              <Chip key={currency} on={currencyFilter === currency} onClick={() => onCurrency(currency)}>{currency}</Chip>
            ))}
          </FilterGroup>
          <FilterGroup label={t("How often")}>
            <Chip on={classFilter === "all"} onClick={() => onClass("all")}>{t("All")}</Chip>
            <Chip on={classFilter === "recurring"} onClick={() => onClass("recurring")}>{t("Recurring")}</Chip>
            <Chip on={classFilter === "once"} onClick={() => onClass("once")}>{t("Once")}</Chip>
            <Chip on={classFilter === "irregular"} onClick={() => onClass("irregular")}>{t("Irregular")}</Chip>
          </FilterGroup>
          {directionLock === "income" ? null : (
            <FilterGroup label={t("Type")}>
              {directionLock === "any" ? <Chip on={directionFilter === "all"} onClick={() => onDirection("all")}>{t("All")}</Chip> : null}
              <Chip on={directionFilter === "expense"} onClick={() => onDirection("expense")}>{t("Expense")}</Chip>
              {directionLock === "any" ? <Chip on={directionFilter === "income"} onClick={() => onDirection("income")}>{t("Income")}</Chip> : null}
              <Chip on={directionFilter === "payment"} onClick={() => onDirection("payment")}>{t("Payment")}</Chip>
            </FilterGroup>
          )}
          {onGroup ? (
            <FilterGroup label={t("Holdings")}>
              <Chip on={groupFilter === "all"} onClick={() => onGroup("all")}>{t("All")}</Chip>
              <Chip on={groupFilter === "money"} onClick={() => onGroup("money")}>{t("Money")}</Chip>
              <Chip on={groupFilter === "debt"} onClick={() => onGroup("debt")}>{t("You owe")}</Chip>
              <Chip on={groupFilter === "investment"} onClick={() => onGroup("investment")}>{t("Investments")}</Chip>
            </FilterGroup>
          ) : null}
        </div>
      </Sheet>,
            document.body,
          )
        : null}
      {filtersOnly ? null : rules.length > 0 && (
        <section className="mb-6">
          <h2 className="mb-2 text-sm text-muted-foreground">{t("Rules")}</h2>
          <div className="divide-y divide-border border-y border-border">
            {rules.map((rule) => (
              <RowButton key={rule.id} onClick={() => onRule(rule)}>
                <div className="min-w-0 flex-1">
                  <p className="truncate text-base">{rule.name}</p>
                  <p className="text-sm text-muted-foreground">
                    {rule.cadence ? scheduleLabel(rule.cadence, rule.intervalDays, t) : t("Once")}
                    {rule.nextDate ? ` · ${rule.nextDate}` : ` · ${t("No date yet")}`}
                    {rule.calendarEventId ? ` · ${t("On Calendar")}` : ""}
                  </p>
                </div>
                <p className="shrink-0 text-lg tabular-nums">{rule.amountMinor != null ? formatMoney(rule.amountMinor, rule.currency) : ""}</p>
              </RowButton>
            ))}
          </div>
        </section>
      )}
      {filtersOnly || !showPlanned || planned.length === 0 ? null : (
        <section className="mb-6">
          <h2 className="fin-kicker mb-2">{t("Planned")}</h2>
          <p className="fin-muted mb-2 text-sm">{t("Not actual")}</p>
          <div className="space-y-2">
            {planned.map((hit) => (
              <button key={`${hit.scheduleId}-${hit.date}`} type="button" className="fin-planned flex min-h-12 w-full items-baseline justify-between gap-3 px-3 py-3 text-left" onClick={() => {
                const rule = rules.find((row) => row.id === hit.scheduleId);
                if (rule) onRule(rule);
              }}>
                <span>
                  <span className="block text-base">{hit.name}</span>
                  <span className="fin-muted text-sm">{hit.date}</span>
                </span>
                <span className="fin-num">{hit.amountMinor != null ? formatMoney(hit.amountMinor, hit.currency) : ""}</span>
              </button>
            ))}
          </div>
        </section>
      )}
      {filtersOnly || directionLock !== "any" || rows.length > 0 || rules.length > 0 || (showPlanned && planned.length > 0) ? null : (
        <EmptyState title={t("Nothing in this filter")} body={t("Try another filter, or log it in Talk.")} actionLabel={t("Add transaction")} onAction={onAdd} />
      )}
      {filtersOnly || rows.length === 0 ? null : (
        <section>
          {heading ? <h2 className="fin-kicker mb-2">{heading}</h2> : null}
          <TxRows rows={rows} accounts={accounts} onOpen={onTransaction} />
        </section>
      )}
    </div>
  );
}

function shownMinor(amount: number, currency: string, display: string, fx: Parameters<typeof bookFromStatus>[0]): number {
  const head = headlineOf([{ amountMinor: amount, currency }], display, bookFromStatus(fx), Date.now());
  return head.state === "ready" ? head.amountMinor : amount;
}

function priceAgo(ms: number): string {
  const minutes = Math.max(0, Math.round((Date.now() - ms) / 60000));
  if (minutes < 1) return fill("just now", {});
  if (minutes < 60) return fill("{n} min ago", { n: minutes });
  const hours = Math.round(minutes / 60);
  if (hours < 48) return fill("{n} h ago", { n: hours });
  return fill("{n} d ago", { n: Math.round(hours / 24) });
}

function AccountList({
  rows,
  transactions,
  rules,
  onPaid,
  emptyTitle,
  emptyBody,
  action,
  onAdd,
  onOpen,
  onPay,
  onRefreshPrice,
  onDiagnose,
  priceNote = "",
  display = "",
  fx,
  quotes = [],
}: {
  rows: FinanceAccount[];
  transactions: FinanceTransaction[];
  rules: FinanceState["snapshot"]["rules"];
  onPaid: (id: string) => void;
  emptyTitle: string;
  emptyBody: string;
  action: string;
  onAdd: () => void;
  onOpen: (account: FinanceAccount) => void;
  onPay?: (account: FinanceAccount) => void;
  onRefreshPrice?: (account: FinanceAccount) => void;
  onDiagnose?: (account: FinanceAccount) => void;
  priceNote?: string;
  display?: string;
  fx?: FinanceState["snapshot"]["fx"];
  quotes?: NonNullable<FinanceState["snapshot"]["prices"]>["quotes"];
}) {
  if (rows.length === 0) return <EmptyState title={emptyTitle} body={emptyBody} actionLabel={action} onAction={onAdd} />;
  return <AccountRows rows={rows} onOpen={onOpen} transactions={transactions} rules={rules} onPaid={onPaid} onPay={onPay} onRefreshPrice={onRefreshPrice} onDiagnose={onDiagnose} priceNote={priceNote} display={display} fx={fx} quotes={quotes} />;
}

function AccountRows({
  rows,
  onOpen,
  transactions,
  rules,
  onPaid,
  onPay,
  onRefreshPrice,
  onDiagnose,
  priceNote = "",
  display = "",
  fx,
  quotes = [],
}: {
  rows: FinanceAccount[];
  onOpen: (account: FinanceAccount) => void;
  transactions: FinanceTransaction[];
  rules: FinanceState["snapshot"]["rules"];
  onPaid: (id: string) => void;
  onPay?: (account: FinanceAccount) => void;
  onRefreshPrice?: (account: FinanceAccount) => void;
  onDiagnose?: (account: FinanceAccount) => void;
  priceNote?: string;
  display?: string;
  fx?: FinanceState["snapshot"]["fx"];
  quotes?: NonNullable<FinanceState["snapshot"]["prices"]>["quotes"];
}) {
  const t = useT();
  return (
    <div className="space-y-3">
      {rows.map((account, index) => {
        const posture = cardPosture(account.kind === "credit_card" ? account.shownBalanceMinor : null);
        const displayMinor = account.kind === "credit_card" ? cardDisplayMinor(account.shownBalanceMinor) : account.shownBalanceMinor;
        const position = positionOf(account, display, bookFromStatus(fx), quotes, Date.now());
        const original = originalHolding(account, position, display);
        const quantity = original.quantity || (account.units ? formatQuantity(account.units) : "");
        const holding = isHoldingAccount(account);
        const money = holding
          ? position.unpriced
            ? fill("No price for {name}", { name: account.name })
            : formatMoney(position.amountMinor, position.currency)
          : displayMinor == null
            ? t("Not set")
            : formatMoney(shownMinor(displayMinor, account.currency, display, fx), (display || account.currency).toUpperCase());
        const nativeLine = holding
          ? [quantity, original.nativeMinor != null ? formatMoney(original.nativeMinor, original.nativeCurrency) : ""].filter(Boolean).join(" · ")
          : display && account.currency && account.currency.toUpperCase() !== display.toUpperCase() && displayMinor != null
            ? formatMoney(displayMinor, account.currency)
            : "";
        const owed = isDebt(account.kind);
        const card = account.kind === "credit_card";
        const payoff = debtPayoff(account, transactions);
        const due = nextDue(account, rules);
        const used = card ? utilization(account.shownBalanceMinor, account.creditLimitMinor) : null;
        const charges = card ? transactions.filter((tx) => tx.accountId === account.id && tx.direction === "expense").slice(0, 3) : [];
        const billDue = owed && due && !(card && (posture === "credit" || posture === "paid"));
        return (
          <div key={account.id} className="border border-border px-3 py-3">
            <RowButton onClick={() => onOpen(account)} className="px-0 py-0">
              <span className="fin-swatch" style={{ background: displayColor(account.color, index) }} />
              <div className="min-w-0 flex-1">
                <p className="truncate text-base">{account.name}</p>
                <p className="text-sm text-muted-foreground">
                  {t(kindLabel(account.kind))}
                  {posture === "credit" ? ` · ${t("Credit")}` : ""}
                  {owed && posture !== "credit" && account.shownBalanceMinor != null && account.shownBalanceMinor > 0 ? ` · ${t("Remaining")}` : ""}
                  {account.kind === "investment" && account.investmentType ? ` · ${t(typeLabel(account.investmentType))}` : ""}
                  {account.kind === "investment" && account.shownBalanceMinor != null ? ` · ${t("Worth")}` : ""}
                  {account.calendarEventId ? ` · ${t("On Calendar")}` : ""}
                </p>
              </div>
              <span className="shrink-0 text-right">
                <span className={cn("block text-xl leading-6 font-medium tabular-nums", (account.marketWorthMinor ?? displayMinor) == null && position.unpriced && "text-base font-normal text-muted-foreground")}>{money}</span>
                {nativeLine ? <span className="fin-muted block text-[13px] leading-4">{nativeLine}</span> : null}
              </span>
            </RowButton>
            {account.marketPrice != null ? (
              <p className="mt-1 text-sm leading-5 text-muted-foreground">
                {account.marketPrice} {account.marketPriceCurrency}
                {account.marketEstimated ? ` · ${t("AI estimate")}` : account.marketSource === "manual" ? ` · ${t("Your price")}` : account.marketSource ? ` · ${account.marketSource}` : ""}
                {account.marketStale ? ` · ${t("Rates are stale")}` : ""}
                {account.marketFetchedMs ? ` · ${fill("Updated {ago}", { ago: priceAgo(account.marketFetchedMs) })}` : ""}
              </p>
            ) : null}
            {holding && position.unpriced ? (
              <button type="button" className="fin-textbtn mt-1 underline" onClick={() => onDiagnose?.(account)}>
                {t("No price")}
              </button>
            ) : null}
            {position.unpriced && priceNote ? <p className="mt-1 text-sm leading-5 text-muted-foreground">{t(priceNote)}</p> : null}
            {onRefreshPrice ? (
              <button type="button" className="fin-chip mt-3 min-h-12 w-full" onClick={() => onRefreshPrice(account)}>{t("Refresh price now")}</button>
            ) : null}
            {owed && payoff && payoff.paid > 0 ? (
              <div className="mt-2">
                <span className="fin-track">
                  <span style={{ width: `${Math.round(payoff.ratio * 100)}%` }} />
                </span>
                <p className="fin-muted mt-1 text-sm">{fill("{paid} paid down", { paid: formatMoney(payoff.paid, account.currency) })}</p>
              </div>
            ) : null}
            {card && account.creditLimitMinor != null && used != null ? (
              <div className="mt-2">
                <span className="fin-track">
                  <span style={{ width: `${Math.min(100, Math.round(used * 100))}%` }} />
                </span>
                <p className="fin-muted mt-1 text-sm">
                  {t("Utilization")} · {posture === "credit" ? t("In credit") : `${Math.round(used * 100)}%`} · {formatMoney(account.creditLimitMinor, account.currency)}
                </p>
              </div>
            ) : null}
            {card && charges.length > 0 ? (
              <div className="mt-2 space-y-1">
                <p className="stamp">{t("Recent charges")}</p>
                {charges.map((tx) => (
                  <p key={tx.id} className="text-sm text-muted-foreground">
                    {tx.date} · {tx.category || t("Expense")} · {formatMoney(tx.amountMinor, tx.currency)}
                  </p>
                ))}
              </div>
            ) : null}
            {card && onPay ? (
              <button type="button" className="fin-chip mt-3 min-h-12 w-full" onClick={() => onPay(account)}>{t("Pay card")}</button>
            ) : null}
            {billDue && due ? (
              <div className="mt-2 flex items-center justify-between gap-3">
                <p className="text-sm text-muted-foreground">
                  {t("Next payment")}
                  {due.date ? ` · ${due.date}` : ""}
                  {due.amount != null ? ` · ${formatMoney(due.amount, due.currency || account.currency)}` : ""}
                </p>
                {due.id ? (
                  <button type="button" className="stamp min-h-12 shrink-0" onClick={() => onPaid(due.id)}>{t("Mark paid")}</button>
                ) : null}
              </div>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}

function TxRows({
  rows,
  accounts,
  onOpen,
}: {
  rows: FinanceTransaction[];
  accounts: FinanceAccount[];
  onOpen: (tx: FinanceTransaction) => void;
}) {
  const t = useT();
  return (
    <div className="divide-y divide-border border-y border-border">
      {rows.map((tx) => {
        const account = accounts.find((row) => row.id === tx.accountId);
        const sign = tx.direction === "income" ? "+" : tx.direction === "expense" ? "−" : "";
        return (
          <RowButton key={tx.id} onClick={() => onOpen(tx)}>
            <div className="min-w-0 flex-1">
              <p className="truncate text-base">{[tx.category, tx.subcategory].filter(Boolean).join(" · ") || t(directionLabel(tx.direction))}</p>
              <p className="text-sm text-muted-foreground">
                {tx.date} · {t(classLabel(tx.recurrence))}
                {account ? ` · ${account.name}` : ""}
              </p>
            </div>
            <p className="shrink-0 text-xl font-medium tabular-nums">
              {sign}
              {formatMoney(tx.amountMinor, tx.currency)}
            </p>
          </RowButton>
        );
      })}
    </div>
  );
}

function TransactionSheet({
  open,
  finance,
  editing,
  onClose,
  presetDirection,
}: {
  open: boolean;
  finance: FinanceState;
  editing: FinanceTransaction | null;
  onClose: () => void;
  presetDirection: string;
}) {
  const t = useT();
  const [amount, setAmount] = useState("");
  const [currency, setCurrency] = useState(finance.snapshot.defaultCurrency);
  const [direction, setDirection] = useState(presetDirection || "expense");
  const [category, setCategory] = useState("");
  const [subcategory, setSubcategory] = useState("");
  const [accountId, setAccountId] = useState("");
  const [fromId, setFromId] = useState("");
  const [date, setDate] = useState("");
  const [recurrence, setRecurrence] = useState("once");
  const [cadence, setCadence] = useState("");
  const [notes, setNotes] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [seen, setSeen] = useState<string | null>(null);
  const key = editing?.id || `new:${presetDirection}`;
  if (!open && seen !== null) setSeen(null);
  if (open && seen !== key) {
    setSeen(key);
    setAmount(editing ? majorString(editing.amountMinor, editing.currency) : "");
    setCurrency(editing?.currency || finance.snapshot.defaultCurrency);
    setDirection(editing?.direction || presetDirection || "expense");
    setCategory(editing?.category || "");
    setSubcategory(editing?.subcategory || "");
    setAccountId(editing?.accountId || "");
    setFromId(editing?.fromAccountId || "");
    setDate(editing?.date || "");
    setRecurrence(editing?.recurrence || "once");
    setCadence("");
    setNotes(editing?.notes || "");
    setConfirming(false);
  }
  const picked = (id: string) => {
    if (id === CASH_WALLET) return { id: "", name: "Cash", kind: "cash" };
    const account = finance.snapshot.accounts.find((row) => row.id === id);
    return { id: account?.id || "", name: account?.name || "", kind: account?.kind || "" };
  };
  const save = () => {
    if (!amount.trim()) return;
    const target = picked(accountId);
    const source = picked(fromId);
    finance.saveTransaction({
      id: editing?.id,
      amount,
      currency,
      direction,
      category,
      subcategory,
      accountId: target.id,
      accountName: target.name,
      accountKind: target.kind,
      fromAccountId: direction === "payment" ? source.id : "",
      fromAccountName: direction === "payment" ? source.name : "",
      fromAccountKind: direction === "payment" ? source.kind : "",
      date,
      notes,
      recurrence,
      cadence: recurrence === "recurring" ? cadence : "",
    });
    onClose();
  };
  return (
    <Sheet
      open={open}
      title={editing ? t("Edit transaction") : t("New transaction")}
      onClose={onClose}
      className="finance"
      footer={
        <div className="flex items-center gap-2">
          {editing && (
            <Button
              variant="ghost"
              onClick={() => {
                if (!confirming) {
                  setConfirming(true);
                  return;
                }
                finance.removeTransaction(editing.id);
                onClose();
              }}
            >
              {confirming ? t("Delete this") : t("Delete")}
            </Button>
          )}
          <div className="flex-1" />
          <Button variant="ghost" onClick={onClose}>{t("Cancel")}</Button>
          <Button disabled={finance.busy || !amount.trim()} onClick={save}>{t("Save")}</Button>
        </div>
      }
    >
      <div className="space-y-4">
        <Field label={t("Category")}>
          <Input value={category} onChange={(e) => { setCategory(e.target.value); setSubcategory(""); }} placeholder={t("Housing, food, salary")} list="finance-categories" />
          <datalist id="finance-categories">
            {finance.snapshot.categories.filter((row) => !row.parent).map((row) => (
              <option key={row.name} value={row.name} />
            ))}
          </datalist>
        </Field>
        <Field label={t("Subcategory")}>
          <Input value={subcategory} onChange={(e) => setSubcategory(e.target.value)} placeholder={t("Rent, groceries")} list="finance-subcategories" />
          <datalist id="finance-subcategories">
            {finance.snapshot.categories
              .filter((row) => row.parent && row.parent.toLowerCase() === category.trim().toLowerCase())
              .map((row) => (
                <option key={row.name} value={row.name} />
              ))}
          </datalist>
        </Field>
        <Field label={t("Amount")}>
          <Input inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} className="h-16 text-3xl tabular-nums" autoFocus />
        </Field>
        <Field label={t("Currency")} hint={t("EUR, USD, BTC, or another code.")}>
          <Input value={currency} onChange={(e) => setCurrency(e.target.value.toUpperCase())} maxLength={12} autoCapitalize="characters" />
        </Field>
        <Field label={t("Direction")}>
          <div className="chip-scroller">
            <Chip on={direction === "expense"} onClick={() => setDirection("expense")}>{t("Expense")}</Chip>
            <Chip on={direction === "income"} onClick={() => setDirection("income")}>{t("Income")}</Chip>
            <Chip on={direction === "payment"} onClick={() => setDirection("payment")}>{t("Payment")}</Chip>
          </div>
        </Field>
        {direction === "payment" ? (
          <div className="space-y-3">
            <p className="text-sm leading-5 text-muted-foreground">{t("Cards")}</p>
            <div className="chip-scroller">
              {finance.snapshot.accounts.filter((account) => account.kind === "credit_card" || account.kind === "loan" || account.kind === "other_debt").map((account) => (
                <Chip key={account.id} on={accountId === account.id} onClick={() => setAccountId(account.id)}>{account.name}</Chip>
              ))}
            </div>
            <PayWith accounts={finance.snapshot.accounts} value={fromId} onChange={setFromId} mode="fund" />
          </div>
        ) : (
          <PayWith accounts={finance.snapshot.accounts} value={accountId} onChange={setAccountId} mode={direction === "income" ? "deposit" : "pay"} />
        )}
        <Field label={t("Date")}>
          <Input type="date" value={date} onChange={(e) => setDate(e.target.value)} />
        </Field>
        <Field label={t("How often")}>
          <div className="chip-scroller">
            <Chip on={recurrence === "once"} onClick={() => setRecurrence("once")}>{t("Once")}</Chip>
            <Chip on={recurrence === "recurring"} onClick={() => setRecurrence("recurring")}>{t("Recurring")}</Chip>
            <Chip on={recurrence === "irregular"} onClick={() => setRecurrence("irregular")}>{t("Irregular")}</Chip>
          </div>
        </Field>
        {recurrence === "recurring" && (
          <div className="chip-scroller">
            {CADENCE.filter(Boolean).map((id) => (
              <Chip key={id} on={cadence === id} onClick={() => setCadence(id)}>{t(cadenceLabel(id))}</Chip>
            ))}
          </div>
        )}
        <Field label={t("Notes")}>
          <Textarea value={notes} onChange={(e) => setNotes(e.target.value)} />
        </Field>
      </div>
    </Sheet>
  );
}

function AccountSheet({
  open,
  investment,
  finance,
  editing,
  presetKind,
  onClose,
}: {
  open: boolean;
  investment: boolean;
  finance: FinanceState;
  editing: FinanceAccount | null;
  presetKind: string;
  onClose: () => void;
}) {
  const t = useT();
  const [name, setName] = useState("");
  const [kind, setKind] = useState(investment ? "investment" : "cash");
  const [balance, setBalance] = useState("");
  const [currency, setCurrency] = useState(finance.snapshot.defaultCurrency);
  const [institution, setInstitution] = useState("");
  const [dueDate, setDueDate] = useState("");
  const [payment, setPayment] = useState("");
  const [cadence, setCadence] = useState("");
  const [ticker, setTicker] = useState("");
  const [contribution, setContribution] = useState("");
  const [contributionCadence, setContributionCadence] = useState("");
  const [contributionDate, setContributionDate] = useState("");
  const [notes, setNotes] = useState("");
  const [color, setColor] = useState(PALETTE[0]);
  const [investmentType, setInvestmentType] = useState("custom");
  const [valueMode, setValueMode] = useState("stated");
  const [units, setUnits] = useState("");
  const [measureUnit, setMeasureUnit] = useState("");
  const [priceOverride, setPriceOverride] = useState(false);
  const [unitPrice, setUnitPrice] = useState("");
  const [leadDays, setLeadDays] = useState(2);
  const [creditLimit, setCreditLimit] = useState("");
  const [dueDay, setDueDay] = useState("");
  const [statementDay, setStatementDay] = useState("");
  const [lastFour, setLastFour] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [seen, setSeen] = useState<string | null>(null);
  const key = `${investment ? "i" : "a"}:${editing?.id || presetKind || "new"}`;
  if (!open && seen !== null) setSeen(null);
  if (open && seen !== key) {
    setSeen(key);
    setName(editing?.name || "");
    const openingKind = editing?.kind || (investment ? "investment" : presetKind || "cash");
    setKind(openingKind);
    const openingBalance = openingKind === "credit_card" ? (editing?.shownBalanceMinor ?? editing?.balanceMinor) : editing?.balanceMinor;
    setBalance(majorString(openingBalance, editing?.currency || finance.snapshot.defaultCurrency));
    setCurrency(editing?.currency || finance.snapshot.defaultCurrency);
    setInstitution(editing?.institution || "");
    setDueDate(editing?.dueDate || "");
    setPayment(majorString(editing?.paymentMinor, editing?.currency || finance.snapshot.defaultCurrency));
    setCadence(editing?.cadence || "");
    setTicker(editing?.ticker || "");
    setContribution(majorString(editing?.contributionMinor, editing?.currency || finance.snapshot.defaultCurrency));
    setContributionCadence(editing?.contributionCadence || "");
    setContributionDate(editing?.contributionDate || "");
    setNotes(editing?.notes || "");
    setColor(editing?.color || PALETTE[0]);
    setInvestmentType(editing?.investmentType || "custom");
    setValueMode(editing?.units ? "units" : "stated");
    setUnits(editing?.units || "");
    setMeasureUnit(editing?.measureUnit || "");
    setPriceOverride(Boolean(editing?.priceOverride));
    setUnitPrice(majorString(editing?.priceMinor, editing?.currency || finance.snapshot.defaultCurrency || ""));
    setLeadDays(editing?.leadDays ?? 2);
    setCreditLimit(majorString(editing?.creditLimitMinor, editing?.currency || finance.snapshot.defaultCurrency));
    setDueDay(editing?.dueDay ? String(editing.dueDay) : "");
    setStatementDay(editing?.statementDay ? String(editing.statementDay) : "");
    setLastFour(editing?.lastFour || "");
    setConfirming(false);
  }
  const save = () => {
    if (!name.trim()) return;
    finance.saveAccount({
      id: editing?.id,
      name,
      kind: investment ? "investment" : kind,
      balance: valueMode === "units" ? "" : balance,
      currency,
      institution,
      dueDate: investment ? "" : dueDate,
      payment: investment ? "" : payment,
      cadence: investment ? "" : cadence,
      ticker,
      contribution: investment ? contribution : "",
      contributionCadence: investment ? contributionCadence : "",
      contributionDate: investment ? contributionDate : "",
      notes,
      color,
      investmentType: investment ? investmentType : "",
      units: valueMode === "units" ? units : "",
      unit: valueMode === "units" ? measureUnit : "",
      unitPrice: valueMode === "units" ? unitPrice : "",
      priceOverride: valueMode === "units" ? priceOverride : false,
      touchOverride: valueMode === "units",
      valueMode,
      leadDays,
      changeKind: true,
      creditLimit: kind === "credit_card" ? creditLimit : "",
      dueDay: kind === "credit_card" && dueDay.trim() ? Number(dueDay) : null,
      statementDay: kind === "credit_card" && statementDay.trim() ? Number(statementDay) : null,
      lastFour: kind === "credit_card" ? lastFour : "",
      editLimit: kind === "credit_card",
      editDueDay: kind === "credit_card",
      editStatementDay: kind === "credit_card",
      editLastFour: kind === "credit_card",
    });
    onClose();
  };
  const cardOrLoan = isDebt(kind);
  return (
    <Sheet
      open={open}
      title={investment ? (editing ? t("Edit investment") : t("New investment")) : kind === "credit_card" ? (editing ? t("Edit card") : t("New card")) : editing ? t("Edit account") : t("New account")}
      onClose={onClose}
      className="finance"
      footer={
        <div className="flex items-center gap-2">
          {editing && (
            <Button
              variant="ghost"
              onClick={() => {
                if (!confirming) {
                  setConfirming(true);
                  return;
                }
                finance.removeAccount(editing.id);
                onClose();
              }}
            >
              {confirming ? t("Delete this") : t("Delete")}
            </Button>
          )}
          <div className="flex-1" />
          <Button variant="ghost" onClick={onClose}>{t("Cancel")}</Button>
          <Button disabled={finance.busy || !name.trim()} onClick={save}>{t("Save")}</Button>
        </div>
      }
    >
      <div className="space-y-4">
        {confirming && <p className="text-sm leading-5 text-muted-foreground">{t("Transactions stay in Activity.")}</p>}
        <Field label={t("Nickname")}>
          <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={t("Cash, Visa travel, car loan")} className="text-lg" autoFocus />
        </Field>
        {!investment && (
          <Field label={t("Kind")}>
            <div className="chip-scroller">
              {KINDS.map((id) => (
                <Chip key={id} on={kind === id} onClick={() => setKind(id)}>{t(kindLabel(id))}</Chip>
              ))}
            </div>
          </Field>
        )}
        <Field
          label={investment ? t("Worth") : kind === "credit_card" ? t("Balance owed") : t("Balance")}
          hint={
            kind === "credit_card"
              ? editing
                ? t("Update the amount owed anytime. A negative amount is a credit.")
                : t("Leave blank until you have checked it. A negative amount is a credit.")
              : t("Leave blank until you have checked it.")
          }
        >
          <Input inputMode="decimal" value={balance} onChange={(e) => setBalance(e.target.value)} className="h-16 text-3xl tabular-nums" />
        </Field>
        {kind === "credit_card" ? (
          <div className="chip-scroller">
            <Chip on={!balance.trim().startsWith("-")} onClick={() => setBalance((value) => value.replace(/^-/, ""))}>{t("Balance owed")}</Chip>
            <Chip
              on={balance.trim().startsWith("-")}
              onClick={() =>
                setBalance((value) => {
                  const trimmed = value.trim().replace(/^-/, "");
                  return trimmed ? `-${trimmed}` : "-";
                })
              }
            >
              {t("Credit")}
            </Chip>
          </div>
        ) : null}
        <Field label={t("Currency")} hint={t("EUR, USD, BTC, or another code.")}>
          <Input value={currency} onChange={(e) => setCurrency(e.target.value.toUpperCase())} maxLength={12} autoCapitalize="characters" />
        </Field>
        <Field label={t("Institution")} hint={t("A bank name or label. Not an account number.")}>
          <Input value={institution} onChange={(e) => setInstitution(e.target.value)} />
        </Field>
        {investment && (
          <>
            <Field label={t("Ticker")}>
              <Input value={ticker} onChange={(e) => setTicker(e.target.value)} />
            </Field>
            <Field label={t("Contribution")} hint={t("Amount added on a schedule. Leave blank if you do not know it.")}>
              <Input inputMode="decimal" value={contribution} onChange={(e) => setContribution(e.target.value)} />
            </Field>
            <CadenceChips value={contributionCadence} onChange={setContributionCadence} />
            <Field label={t("Next contribution")}>
              <Input type="date" value={contributionDate} onChange={(e) => setContributionDate(e.target.value)} />
            </Field>
          </>
        )}
        {kind === "credit_card" && (
          <>
            <Field label={t("Credit limit")} hint={t("Optional.")}>
              <Input inputMode="decimal" value={creditLimit} onChange={(e) => setCreditLimit(e.target.value)} />
            </Field>
            <Field label={t("Due day")} hint={t("Day of the month, 1 to 31.")}>
              <Input inputMode="numeric" value={dueDay} onChange={(e) => setDueDay(e.target.value)} />
            </Field>
            <Field label={t("Statement day")} hint={t("Day of the month, 1 to 31.")}>
              <Input inputMode="numeric" value={statementDay} onChange={(e) => setStatementDay(e.target.value)} />
            </Field>
            <Field label={t("Last four")} hint={t("Optional. Four digits only.")}>
              <Input inputMode="numeric" maxLength={4} value={lastFour} onChange={(e) => setLastFour(e.target.value.replace(/\D/g, "").slice(0, 4))} />
            </Field>
          </>
        )}
        {cardOrLoan && (
          <>
            <Field label={t("Payment amount")} hint={t("A typical or minimum payment. Leave blank if you do not know it.")}>
              <Input inputMode="decimal" value={payment} onChange={(e) => setPayment(e.target.value)} />
            </Field>
            <CadenceChips value={cadence} onChange={setCadence} />
            <Field label={t("Due date")}>
              <Input type="date" value={dueDate} onChange={(e) => setDueDate(e.target.value)} />
            </Field>
          </>
        )}
        {investment && (
          <>
            <Field label={t("Type")}>
              <div className="chip-scroller">
                {["stock", "fund", "crypto", "metal", "property", "collectible", "stake", "custom"].map((id) => (
                  <Chip key={id} on={investmentType === id} onClick={() => setInvestmentType(id)}>{t(typeLabel(id))}</Chip>
                ))}
              </div>
            </Field>
            <Field label={t("Value")}>
              <div className="fin-filter-list">
                <Chip on={valueMode === "stated"} onClick={() => setValueMode("stated")}>{t("Stated worth")}</Chip>
                <Chip on={valueMode === "units"} onClick={() => setValueMode("units")}>{t("Units and price")}</Chip>
              </div>
            </Field>
            {valueMode === "units" && (
              <>
                <Field label={t("Quantity")}>
                  <Input inputMode="decimal" value={units} onChange={(e) => setUnits(e.target.value)} />
                </Field>
                <Field label={t("Unit")}>
                  <div className="chip-scroller">
                    {[
                      ["troy_oz", "Troy ounce"],
                      ["gram", "Gram"],
                      ["kilogram", "Kilogram"],
                      ["share", "One share"],
                      ["coin", "Coin"],
                    ].map(([id, label]) => (
                      <Chip key={id} on={measureUnit === id} onClick={() => setMeasureUnit(id)}>{t(label)}</Chip>
                    ))}
                  </div>
                </Field>
                <Field label={t("Price each")}>
                  <Input inputMode="decimal" value={unitPrice} onChange={(e) => setUnitPrice(e.target.value)} />
                </Field>
                <label className="mt-2 flex items-start gap-2 text-sm leading-5">
                  <input type="checkbox" className="mt-1 size-4" checked={priceOverride} onChange={(event) => setPriceOverride(event.target.checked)} />
                  <span>{t("Use this instead of the live price")}</span>
                </label>
              </>
            )}
          </>
        )}
        <ColorField color={color} onChange={setColor} />
        <Field label={t("Remind me")}>
          <div className="chip-scroller">
            {LEADS.map((days) => (
              <Chip key={days} on={leadDays === days} onClick={() => setLeadDays(days)}>{t(leadLabel(days))}</Chip>
            ))}
          </div>
        </Field>
        <Field label={t("Notes")}>
          <Textarea value={notes} onChange={(e) => setNotes(e.target.value)} />
        </Field>
      </div>
    </Sheet>
  );
}


function CadenceChips({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  const t = useT();
  return (
    <Field label={t("How often")}>
      <div className="chip-scroller">
        {CADENCE.map((id) => (
          <Chip key={id || "none"} on={value === id} onClick={() => onChange(id)}>
            {id ? t(cadenceLabel(id)) : t("Once")}
          </Chip>
        ))}
      </div>
    </Field>
  );
}

function Chip({ on, onClick, wide, children }: { on: boolean; onClick: () => void; wide?: boolean; children: string }) {
  return (
    <button
      type="button"
      aria-pressed={on}
      onClick={onClick}
      className={cn("fin-chip", wide ? "w-full" : "shrink-0")}
    >
      {children}
    </button>
  );
}

function cadenceLabel(cadence: string): string {
  if (cadence === "every_n_days") return "Every N days";
  if (cadence === "every_n_weeks") return "Every N weeks";
  if (cadence === "every_n_months") return "Every N months";
  if (cadence === "every_n_years") return "Every N years";
  if (cadence === "daily") return "Daily";
  if (cadence === "weekly") return "Weekly";
  if (cadence === "monthly") return "Monthly";
  if (cadence === "yearly") return "Yearly";
  return "Once";
}

function scheduleLabel(cadence: string, interval: number | undefined, t: (text: string) => string): string {
  const n = interval && interval > 0 ? String(interval) : "1";
  if (cadence === "every_n_days") return `${t("Every")} ${n} ${t("Days")}`;
  if (cadence === "every_n_weeks") return `${t("Every")} ${n} ${t("Weeks")}`;
  if (cadence === "every_n_months") return `${t("Every")} ${n} ${t("Months")}`;
  if (cadence === "every_n_years") return `${t("Every")} ${n} ${t("Years")}`;
  return t(cadenceLabel(cadence));
}

function directionLabel(direction: string): string {
  if (direction === "income") return "Income";
  if (direction === "payment") return "Payment";
  return "Expense";
}

function classLabel(recurrence: string): string {
  if (recurrence === "recurring") return "Recurring";
  if (recurrence === "irregular") return "Irregular";
  return "Once";
}

function groupLabel(group: string): string {
  if (group === "debt") return "You owe";
  if (group === "investment") return "Investments";
  if (group === "money") return "Money";
  return "All";
}

function FilterGroup({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <p className="fin-kicker mb-2">{label}</p>
      <div className="fin-filter-list">{children}</div>
    </div>
  );
}

function periodLabel(period: PeriodId): string {
  if (period === "last") return "Last month";
  if (period === "six") return "Last 6 months";
  if (period === "year") return "This year";
  if (period === "all") return "All time";
  return "This month";
}

function leadLabel(days: number): string {
  if (days === 0) return "On the day";
  if (days === 1) return "1 day before";
  if (days === 2) return "2 days before";
  if (days === 3) return "3 days before";
  return "A week before";
}

function AccountsBoard({
  accounts,
  focus,
  transactions,
  rules,
  onPaid,
  onOpen,
  onAddAccount,
  onAddCash,
  onAddDebt,
  onAddLoan,
  onPay,
  display = "",
  fx,
  quotes = [],
}: {
  accounts: FinanceAccount[];
  focus: WorthFocus | "all";
  transactions: FinanceTransaction[];
  rules: FinanceState["snapshot"]["rules"];
  onPaid: (id: string) => void;
  display?: string;
  fx?: FinanceState["snapshot"]["fx"];
  quotes?: NonNullable<FinanceState["snapshot"]["prices"]>["quotes"];
  onOpen: (account: FinanceAccount) => void;
  onAddAccount: () => void;
  onAddCash: () => void;
  onAddDebt: () => void;
  onAddLoan: () => void;
  onPay: (account: FinanceAccount) => void;
}) {
  const t = useT();
  const bank = accounts.filter((account) => account.kind === "bank");
  const cash = accounts.filter((account) => account.kind === "cash");
  const cards = accounts.filter((account) => account.kind === "credit_card");
  const loans = accounts.filter((account) => account.kind === "loan" || account.kind === "other_debt");
  const show = (section: "accounts" | "cash" | "debt") => {
    if (focus === "all" || focus === "owned") return section !== "debt" || focus === "all";
    if (focus === "cash") return section === "cash";
    if (focus === "invested") return false;
    return section === "debt";
  };
  const block = (title: string, rows: FinanceAccount[], emptyTitle: string, emptyBody: string, action: string, onAdd: () => void, pay = false) => (
    <section key={title}>
      <h2 className="fin-kicker mb-2">{title}</h2>
      <AccountList rows={rows} transactions={transactions} rules={rules} onPaid={onPaid} emptyTitle={emptyTitle} emptyBody={emptyBody} action={action} onAdd={onAdd} onOpen={onOpen} onPay={pay ? onPay : undefined} display={display} fx={fx} quotes={quotes} />
    </section>
  );
  return (
    <div className="fin-stack w-full min-w-0 space-y-6">
      {show("accounts")
        ? block(t("Accounts"), bank, t("No accounts yet"), t("Banks, cash, cards, and loans. Investments are in Holdings."), t("Add account"), onAddAccount)
        : null}
      {show("cash") && (cash.length > 0 || focus === "cash")
        ? block(t("Cash & savings"), cash, t("No accounts yet"), t("Banks, cash, cards, and loans. Investments are in Holdings."), t("Add account"), onAddCash)
        : null}
      {show("debt")
        ? block(t("Credit cards"), cards, t("No credit card is saved."), t("A card you pay with. Charges increase what you owe."), t("Add a card"), onAddDebt, true)
        : null}
      {show("debt")
        ? block(t("Loans and other debts"), loans, t("No debts saved"), t("A loan. Payments reduce what you owe."), t("Add account"), onAddLoan)
        : null}
    </div>
  );
}

function ColorField({ color, onChange }: { color: string; onChange: (color: string) => void }) {
  const t = useT();
  return (
    <Field label={t("Color")}>
      <div className="flex flex-wrap gap-2">
        {PALETTE.map((swatch) => (
          <button key={swatch} type="button" aria-label={swatch} className="fin-swatch h-11 w-11" style={{ background: swatch, outline: color === swatch ? "2px solid var(--fin-gold)" : undefined }} onClick={() => onChange(swatch)} />
        ))}
        <Input aria-label={t("Custom color")} value={color} onChange={(event) => onChange(event.target.value)} className="w-28" />
      </div>
    </Field>
  );
}

function CategorySheet({ open, finance, onClose }: { open: boolean; finance: FinanceState; onClose: () => void }) {
  const t = useT();
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [name, setName] = useState("");
  const [parent, setParent] = useState("");
  const [kind, setKind] = useState("expense");
  const [mergeInto, setMergeInto] = useState<Record<string, string>>({});
  const rows = [...finance.snapshot.categories].sort((a, b) => (a.order || 0) - (b.order || 0) || a.name.localeCompare(b.name));
  const kinds = ["income", "expense", "savings", "investment", "debt"] as const;
  return (
    <Sheet open={open} title={t("Edit categories")} onClose={onClose} className="finance">
      <div className="space-y-3">
        <div className="flex flex-wrap gap-2">
          <Input aria-label={t("Name")} value={name} onChange={(event) => setName(event.target.value)} placeholder={t("Name")} />
          <select aria-label={t("Kind")} className="h-11 border border-[var(--fin-line)] bg-transparent px-2 text-[15px]" value={kind} onChange={(event) => setKind(event.target.value)}>
            {kinds.map((item) => (
              <option key={item} value={item}>{t(categoryKindLabel(item))}</option>
            ))}
          </select>
          <Input aria-label={t("Parent")} value={parent} onChange={(event) => setParent(event.target.value)} placeholder={t("Parent")} list="finance-parents" />
          <datalist id="finance-parents">
            {rows.filter((row) => !row.parent).map((row) => (
              <option key={row.name} value={row.name} />
            ))}
          </datalist>
          <Button type="button" onClick={() => { if (!name.trim()) return; finance.editCategory({ action: "create", name: name.trim(), kind, parent: parent.trim(), order: rows.length }); setName(""); }}>{parent.trim() ? t("Add subcategory") : t("Add")}</Button>
        </div>
        <Button type="button" variant="ghost" onClick={() => {
          for (const suggestion of ["Salary", "Freelance", "Housing", "Food", "Transport", "Subscriptions", "Health", "Savings", "Investing"]) {
            const itemKind = suggestion === "Salary" || suggestion === "Freelance" ? "income" : suggestion === "Savings" ? "savings" : suggestion === "Investing" ? "investment" : "expense";
            finance.editCategory({ action: "create", name: suggestion, kind: itemKind });
          }
        }}>{t("Add suggested categories")}</Button>
        {rows.length === 0 && <p className="fin-muted text-[13px] leading-4">{t("A category is saved the first time you use it.")}</p>}
        {rows.map((category, index) => (
          <div key={category.name} className="border border-[var(--fin-line)] p-2">
            <div className="flex gap-2">
              <span className="fin-swatch mt-2 shrink-0" style={{ background: category.color || "#6E8B74" }} />
              <Input value={drafts[category.name] ?? category.name} onChange={(event) => setDrafts((current) => ({ ...current, [category.name]: event.target.value }))} />
              <Button type="button" onClick={() => finance.editCategory({ action: "rename", name: category.name, to: drafts[category.name] ?? category.name })}>{t("Rename")}</Button>
            </div>
            <div className="mt-2 flex flex-wrap gap-1">
              {PALETTE.map((swatch) => (
                <button key={swatch} type="button" className="fin-icon" aria-label={swatch} onClick={() => finance.editCategory({ action: "color", name: category.name, color: swatch })}>
                  <span className="fin-swatch" style={{ background: swatch }} />
                </button>
              ))}
            </div>
            <div className="mt-2 flex flex-wrap gap-1">
              {kinds.map((item) => (
                <button key={item} type="button" className="fin-chip" aria-pressed={(category.kind || "expense") === item} onClick={() => finance.editCategory({ action: "kind", name: category.name, kind: item })}>{t(categoryKindLabel(item))}</button>
              ))}
              <button type="button" className="fin-chip" onClick={() => finance.editCategory({ action: "order", name: category.name, order: Math.max(0, index - 1) })}>{t("Up")}</button>
              <button type="button" className="fin-chip" onClick={() => finance.editCategory({ action: "order", name: category.name, order: index + 1 })}>{t("Down")}</button>
              <button type="button" className="fin-chip" onClick={() => finance.editCategory({ action: "delete", name: category.name })}>{t("Delete")}</button>
            </div>
            <div className="mt-2 flex gap-2">
              <select aria-label={t("Merge")} className="h-11 min-w-0 flex-1 border border-[var(--fin-line)] bg-transparent px-2 text-[15px]" value={mergeInto[category.name] || ""} onChange={(event) => setMergeInto((current) => ({ ...current, [category.name]: event.target.value }))}>
                <option value="">{t("Merge")}</option>
                {rows.filter((row) => row.name !== category.name).map((row) => (
                  <option key={row.name} value={row.name}>{row.name}</option>
                ))}
              </select>
              <Button type="button" onClick={() => { if (mergeInto[category.name]) finance.editCategory({ action: "merge", name: category.name, to: mergeInto[category.name] }); }}>{t("Merge")}</Button>
            </div>
          </div>
        ))}
      </div>
    </Sheet>
  );
}

function categoryKindLabel(kind: string): string {
  if (kind === "income") return "Income";
  if (kind === "savings") return "Savings";
  if (kind === "investment") return "Investing";
  if (kind === "debt") return "Debt payments";
  return "Expenses";
}

function PayCardSheet({
  open,
  finance,
  card,
  onClose,
}: {
  open: boolean;
  finance: FinanceState;
  card: FinanceAccount | null;
  onClose: () => void;
}) {
  const t = useT();
  const [amount, setAmount] = useState("");
  const [fromId, setFromId] = useState("");
  const [date, setDate] = useState("");
  const [error, setError] = useState("");
  const [seen, setSeen] = useState<string | null>(null);
  const key = card?.id || "none";
  if (!open && seen !== null) setSeen(null);
  if (open && card && seen !== key) {
    setSeen(key);
    setAmount(card.shownBalanceMinor != null && card.shownBalanceMinor > 0 ? majorString(card.shownBalanceMinor, card.currency) : "");
    setFromId("");
    setDate(todayIso());
    setError("");
  }
  const save = () => {
    if (!card || !amount.trim() || amount.trim() === "-") return;
    if (!fromId) {
      setError(t("Which account pays this card?"));
      return;
    }
    const source = fromId === CASH_WALLET
      ? { id: "", name: "Cash", kind: "cash" }
      : finance.snapshot.accounts.find((row) => row.id === fromId);
    finance.saveTransaction({
      amount,
      currency: card.currency,
      direction: "payment",
      category: cardPaymentLabel(card.name),
      accountId: card.id,
      accountName: card.name,
      accountKind: "credit_card",
      fromAccountId: source?.id || "",
      fromAccountName: source?.name || "",
      fromAccountKind: source?.kind || "cash",
      date: date || todayIso(),
      notes: "",
      recurrence: "once",
    });
    onClose();
  };
  const posture = cardPosture(card?.shownBalanceMinor ?? null);
  const display = card ? cardDisplayMinor(card.shownBalanceMinor) : null;
  const owed = display != null && card ? formatMoney(display, card.currency) : t("Not set");
  return (
    <Sheet
      open={open && Boolean(card)}
      title={card ? card.name : t("Pay card")}
      onClose={onClose}
      className="finance"
      footer={
        <div className="flex flex-wrap items-center gap-2">
          <Button variant="ghost" className="min-h-12" onClick={onClose}>{t("Back")}</Button>
          <div className="flex-1" />
          <Button className="min-h-12" disabled={finance.busy || !amount.trim() || !fromId} onClick={save}>{t("Pay card")}</Button>
        </div>
      }
    >
      <div className="space-y-4">
        <p className="text-sm leading-5 text-muted-foreground">{t("A card payment is not a new expense. Cash and the balance owed both fall. Paying more than you owe leaves a credit. Net worth stays the same.")}</p>
        {posture === "credit" ? <p className="text-sm leading-5">{t("This card is already in credit. Enter an amount only if you still want to pay more.")}</p> : null}
        {posture === "paid" ? <p className="text-sm leading-5">{t("This card is already paid. Enter an amount only if you want to pay more.")}</p> : null}
        <p className="fin-num text-3xl">{posture === "credit" ? `${owed} ${t("Credit")}` : owed}</p>
        {card && posture === "owed" && cardDueDate(card, todayIso()) ? (
          <p className="fin-muted text-sm">{t("Next payment")} · {cardDueDate(card, todayIso())}</p>
        ) : null}
        <div className="chip-scroller">
          <Chip
            on={false}
            onClick={() => {
              if (!card || card.shownBalanceMinor == null || card.shownBalanceMinor <= 0) return;
              setAmount(majorString(card.shownBalanceMinor, card.currency));
            }}
          >
            {t("Pay in full")}
          </Chip>
          <Chip on={false} onClick={() => setAmount("")}>{t("Custom amount")}</Chip>
        </div>
        <Field label={t("Amount")}>
          <Input inputMode="decimal" value={amount} onChange={(event) => setAmount(event.target.value)} className="h-16 text-3xl tabular-nums" />
        </Field>
        {card ? <PayWith accounts={finance.snapshot.accounts.filter((row) => row.id !== card.id)} value={fromId} onChange={setFromId} mode="fund" /> : null}
        <Field label={t("Date")}>
          <Input type="date" value={date} onChange={(event) => setDate(event.target.value)} />
        </Field>
        {error ? <p className="text-sm leading-5">{error}</p> : null}
      </div>
    </Sheet>
  );
}

function cardPaymentLabel(name: string): string {
  return /payment$/i.test(name.trim()) ? name.trim() : `${name.trim()} payment`;
}

function downloadBase64(name: string, data: string) {
  const bytes = Uint8Array.from(atob(data), (char) => char.charCodeAt(0));
  const url = URL.createObjectURL(new Blob([bytes], { type: "application/pdf" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  URL.revokeObjectURL(url);
}
