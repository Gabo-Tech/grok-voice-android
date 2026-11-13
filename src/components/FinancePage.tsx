import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { EmptyState, Notice, PageChrome, RowButton } from "@/components/PageChrome";
import { Sheet } from "@/components/Sheet";
import { Field } from "@/components/Field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { FinanceCarousel, type ChartPick } from "@/components/FinanceCharts";
import type { FinanceState } from "@/hooks/useFinance";
import type { FinanceAccount, FinanceRule, FinanceTransaction } from "@/lib/api";
import { formatMoney, kindLabel, majorString, minorExponent, typeLabel } from "@/lib/money";
import { displayColor, filterTransactions, groupOf, isDebt, PALETTE, periodBounds, type PeriodId } from "@/lib/financeView";
import { appliedFilters, clearedFilters, withoutFilter, type FilterId, type LedgerFilters } from "@/lib/financeFilters";
import { cardDueDate, debtPayoff, nextDue, paymentFits, plannedHits, utilization } from "@/lib/financeMath";
import { FinanceDashboard, plannedYearBars, todayIso, useFinanceRateWatch, type WorthFocus } from "@/components/FinanceDashboard";
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
  if (tab === "expenses") return "Expenses";
  if (tab === "income") return "Income";
  if (tab === "accounts") return "Accounts";
  return "Charts";
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
  const [editingAccount, setEditingAccount] = useState<FinanceAccount | null>(null);
  const [editingTx, setEditingTx] = useState<FinanceTransaction | null>(null);
  const [editingRule, setEditingRule] = useState<FinanceRule | null>(null);
  const [paying, setPaying] = useState<FinanceAccount | null>(null);
  const [classFilter, setClassFilter] = useState("all");
  const [directionFilter, setDirectionFilter] = useState(() => {
    const saved = readFinanceTab(readStoredTab());
    if (saved === "expenses") return "expense";
    if (saved === "income") return "income";
    return "all";
  });
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
  const recent = finance.snapshot.transactions.slice(0, 5);
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
  const chartRules = useMemo(
    () =>
      finance.snapshot.rules.filter((rule) => {
        if (accountFilter !== "all" && rule.accountId !== accountFilter) return false;
        if (categoryFilter !== "all" && rule.category !== categoryFilter) return false;
        if (currencyFilter !== "all" && rule.currency !== currencyFilter) return false;
        return true;
      }),
    [finance.snapshot.rules, accountFilter, categoryFilter, currencyFilter],
  );
  const bars = useMemo(() => plannedYearBars(chartRules, finance.snapshot.transactions, today), [chartRules, finance.snapshot.transactions, today]);
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
    if (next === "expenses") setDirectionFilter("expense");
    if (next === "income") setDirectionFilter("income");
    if (next === "charts") setDirectionFilter("all");
    if (next === "accounts") setGroupFocus(focus ?? "all");
  };
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
    if (tab === "income") {
      setTxPreset("income");
      setSheet("transaction");
    } else if (tab === "expenses") {
      setTxPreset("expense");
      setSheet("transaction");
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
    setSheet(account.kind === "investment" ? "investment" : "account");
  };
  const openTransaction = (tx: FinanceTransaction) => {
    setEditingTx(tx);
    setSheet("transaction");
  };
  const openRule = (rule: FinanceRule) => {
    setEditingRule(rule);
    setSheet("rule");
  };

  const addLabel = tab === "overview" ? "Program" : tab === "accounts" ? (groupFocus === "invested" ? "Add investment" : "Add account") : tab === "income" ? "Add income" : "Add expense";
  const ledgerRules = (classFilter === "all" || classFilter === "recurring" ? rules : []).filter((rule) => {
    if (tab === "expenses") return rule.direction !== "income";
    if (tab === "income") return rule.direction === "income" && rule.purpose !== "investment_payout";
    return true;
  });
  const empty = accounts.length === 0 && finance.snapshot.transactions.length === 0 && finance.snapshot.rules.length === 0;

  const ledger = (lock: "expense" | "income") => (
    <div className="space-y-4">
      <Activity
        rows={activity}
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
        onAdd={tab === "charts" ? undefined : openAdd}
        addLabel={t(addLabel)}
        active={active}
      >
        {finance.error && <Notice title={t(finance.error)} onDismiss={() => finance.setError(null)} />}
        {tab === "overview" ? (
          <div role="tabpanel" id="finance-panel-overview" aria-labelledby="finance-tab-overview" data-finance-tabpanel="overview">
            <Overview
              finance={finance}
              empty={empty}
              accounts={accounts}
              recent={recent}
              onBank={openBank}
              onCard={openCard}
              onIncome={openIncome}
              onBill={openBill}
              onFocus={(focus) => chooseTab("accounts", focus)}
              onOpenExpenses={() => chooseTab("expenses")}
              onOpenIncome={() => chooseTab("income")}
              onTransaction={openTransaction}
            />
          </div>
        ) : null}
        {tab === "expenses" ? (
          <div role="tabpanel" id="finance-panel-expenses" aria-labelledby="finance-tab-expenses" data-finance-tabpanel="expenses">
            <FinanceDashboard
              finance={finance}
              accounts={accounts}
              place="expenses"
              onProgram={openBill}
              onIncome={openIncome}
              onFocus={(focus) => chooseTab("accounts", focus)}
            >
              {ledger("expense")}
            </FinanceDashboard>
          </div>
        ) : null}
        {tab === "income" ? (
          <div role="tabpanel" id="finance-panel-income" aria-labelledby="finance-tab-income" data-finance-tabpanel="income">
            <FinanceDashboard
              finance={finance}
              accounts={accounts}
              place="income"
              onProgram={openBill}
              onIncome={openBill}
              onPayout={openBill}
              onFocus={(focus) => chooseTab("accounts", focus)}
            >
              {ledger("income")}
            </FinanceDashboard>
          </div>
        ) : null}
        {tab === "accounts" ? (
          <div role="tabpanel" id="finance-panel-accounts" aria-labelledby="finance-tab-accounts" data-finance-tabpanel="accounts">
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
              onAddInvestment={openInvestment}
            />
          </div>
        ) : null}
        {tab === "charts" ? (
          <div role="tabpanel" id="finance-panel-charts" aria-labelledby="finance-tab-charts" data-finance-tabpanel="charts">
            <Activity
              rows={activity}
              rules={[]}
              classFilter={classFilter}
              directionFilter={directionFilter}
              directionLock="any"
              filtersOnly
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
              groupFilter={groupFilter}
              onGroup={setGroupFilter}
              onAdd={openAdd}
              onTransaction={openTransaction}
              onRule={openRule}
              planned={[]}
              accounts={accounts}
            />
            <button type="button" className="fin-chip mb-4 w-full" onClick={exportSummary}>
              {t("Export PDF")}
            </button>
            {exportNote ? <p className="fin-muted mb-4 text-sm leading-5">{exportNote}</p> : null}
            {chartAccounts.length === 0 && activity.length === 0 && bars.every((row) => !row.income && !row.expense) ? (
              <p className="fin-muted text-sm leading-5">{t("Nothing in this filter")}</p>
            ) : null}
            <FinanceCarousel
              accounts={chartAccounts}
              transactions={activity}
              planned={bars}
              display={finance.snapshot.defaultCurrency}
              fx={finance.snapshot.fx}
              busy={finance.busy}
              onOpenSettings={onOpenCurrency}
              onPick={pickChart}
            />
          </div>
        ) : null}
      </PageChrome>
      {tab === "expenses" || tab === "income" ? (
        <div className="fin-actions">
          <button type="button" className="fin-chip" onClick={tab === "income" ? openIncome : openAdd}>
            {t(tab === "income" ? "Add income" : "Add expense")}
          </button>
          <button type="button" className="fin-chip" onClick={openBill}>
            {t(tab === "income" ? "Program income" : "Program a bill")}
          </button>
        </div>
      ) : null}
      <div role="tablist" aria-label={t("Finance")} className="fin-tabs">
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
      </div>
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
      <PayCardSheet open={sheet === "pay"} finance={finance} card={paying} onClose={() => setSheet(null)} />
      <CategorySheet open={categorySheet} finance={finance} onClose={() => setCategorySheet(false)} />
    </div>
  );
}

function Overview({
  finance,
  empty,
  accounts,
  recent,
  onBank,
  onCard,
  onIncome,
  onBill,
  onFocus,
  onOpenExpenses,
  onOpenIncome,
  onTransaction,
}: {
  finance: FinanceState;
  empty: boolean;
  accounts: FinanceAccount[];
  recent: FinanceTransaction[];
  onBank: () => void;
  onCard: () => void;
  onIncome: () => void;
  onBill: () => void;
  onFocus: (focus: WorthFocus) => void;
  onOpenExpenses: () => void;
  onOpenIncome: () => void;
  onTransaction: (tx: FinanceTransaction) => void;
}) {
  const t = useT();
  return (
    <div className="fin-stack w-full min-w-0 space-y-6">
      <FinanceDashboard
        finance={finance}
        accounts={accounts}
        place="overview"
        onProgram={onBill}
        onIncome={onIncome}
        onFocus={onFocus}
        onOpenExpenses={onOpenExpenses}
        onOpenIncome={onOpenIncome}
      />
      {empty ? (
        <div className="fin-stack w-full min-w-0 space-y-3">
          <div className="mag-panel w-full">
            <p className="masthead text-xl">{t("No money saved yet")}</p>
            <p className="mt-2 text-base leading-6 text-muted-foreground">{t("Start with one. A nickname is enough. No bank login.")}</p>
          </div>
          <StartCard title={t("Add a bank")} body={t("A bank or wallet you spend from.")} onClick={onBank} />
          <StartCard title={t("Add a card")} body={t("A card or loan. Payments reduce what you owe.")} onClick={onCard} />
          <StartCard title={t("Add income")} body={t("Money coming in.")} onClick={onIncome} />
          <StartCard title={t("Add a bill")} body={t("Rent, a subscription, or anything with a date.")} onClick={onBill} />
        </div>
      ) : null}
      <p className="fin-muted text-sm leading-5">{t("Balances are what you last said. Nothing here connects to a bank.")}</p>
      {recent.length > 0 ? (
        <section>
          <h2 className="fin-kicker mb-2">{t("Recent")}</h2>
          <TxRows rows={recent} accounts={accounts} onOpen={onTransaction} />
        </section>
      ) : null}
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
                    {rule.cadence ? t(cadenceLabel(rule.cadence)) : t("Once")}
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
}) {
  if (rows.length === 0) return <EmptyState title={emptyTitle} body={emptyBody} actionLabel={action} onAction={onAdd} />;
  return <AccountRows rows={rows} onOpen={onOpen} transactions={transactions} rules={rules} onPaid={onPaid} onPay={onPay} />;
}

function AccountRows({
  rows,
  onOpen,
  transactions,
  rules,
  onPaid,
  onPay,
}: {
  rows: FinanceAccount[];
  onOpen: (account: FinanceAccount) => void;
  transactions: FinanceTransaction[];
  rules: FinanceState["snapshot"]["rules"];
  onPaid: (id: string) => void;
  onPay?: (account: FinanceAccount) => void;
}) {
  const t = useT();
  return (
    <div className="space-y-3">
      {rows.map((account, index) => {
        const money = account.shownBalanceMinor == null ? t("Not set") : formatMoney(account.shownBalanceMinor, account.currency);
        const owed = isDebt(account.kind);
        const card = account.kind === "credit_card";
        const payoff = debtPayoff(account, transactions);
        const due = nextDue(account, rules);
        const used = card ? utilization(account.shownBalanceMinor, account.creditLimitMinor) : null;
        const charges = card ? transactions.filter((tx) => tx.accountId === account.id && tx.direction === "expense").slice(0, 3) : [];
        return (
          <div key={account.id} className="border border-border px-3 py-3">
            <RowButton onClick={() => onOpen(account)} className="px-0 py-0">
              <span className="fin-swatch" style={{ background: displayColor(account.color, index) }} />
              <div className="min-w-0 flex-1">
                <p className="truncate text-base">{account.name}</p>
                <p className="text-sm text-muted-foreground">
                  {t(kindLabel(account.kind))}
                  {owed && account.shownBalanceMinor != null ? ` · ${t("Remaining")}` : ""}
                  {account.kind === "investment" && account.shownBalanceMinor != null ? ` · ${t("Worth")}` : ""}
                  {account.calendarEventId ? ` · ${t("On Calendar")}` : ""}
                </p>
              </div>
              <p className={cn("shrink-0 text-right text-xl leading-6 font-medium tabular-nums", account.shownBalanceMinor == null && "text-base font-normal text-muted-foreground")}>{money}</p>
            </RowButton>
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
                  {t("Utilization")} · {Math.round(used * 100)}% · {formatMoney(account.creditLimitMinor, account.currency)}
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
            {owed && due ? (
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
              <p className="truncate text-base">{tx.category || t(directionLabel(tx.direction))}</p>
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
  const [accountId, setAccountId] = useState("");
  const [fromId, setFromId] = useState("");
  const [date, setDate] = useState("");
  const [recurrence, setRecurrence] = useState("once");
  const [cadence, setCadence] = useState("");
  const [notes, setNotes] = useState("");
  const [formError, setFormError] = useState("");
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
    setAccountId(editing?.accountId || "");
    setFromId(editing?.fromAccountId || "");
    setDate(editing?.date || "");
    setRecurrence(editing?.recurrence || "once");
    setCadence("");
    setNotes(editing?.notes || "");
    setFormError("");
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
    if (direction === "payment" && target.kind === "credit_card") {
      const card = finance.snapshot.accounts.find((row) => row.id === target.id);
      const minor = Number(amount.replace(",", "."));
      const pay = card && Number.isFinite(minor) ? Math.round(minor * 10 ** minorExponent(card.currency)) : 0;
      if (card && card.currency === (currency || card.currency).toUpperCase() && paymentFits(card.shownBalanceMinor, pay) === "over") {
        setFormError(t("That is more than this card owes."));
        return;
      }
    }
    finance.saveTransaction({
      id: editing?.id,
      amount,
      currency,
      direction,
      category,
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
        <Field label={t("Category or payee")}>
          <Input value={category} onChange={(e) => setCategory(e.target.value)} placeholder={t("Lunch, rent, salary")} />
        </Field>
        <Field label={t("Amount")}>
          <Input inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} className="h-16 text-3xl tabular-nums" autoFocus />
        </Field>
        <Field label={t("Currency")} hint={t("Three letters, such as EUR or USD.")}>
          <Input value={currency} onChange={(e) => setCurrency(e.target.value.toUpperCase())} maxLength={8} autoCapitalize="characters" />
        </Field>
        <Field label={t("Direction")}>
          <div className="chip-scroller">
            <Chip on={direction === "expense"} onClick={() => setDirection("expense")}>{t("Expense")}</Chip>
            <Chip on={direction === "income"} onClick={() => setDirection("income")}>{t("Income")}</Chip>
            <Chip on={direction === "payment"} onClick={() => setDirection("payment")}>{t("Payment")}</Chip>
          </div>
        </Field>
        {formError ? <p className="text-sm leading-5">{formError}</p> : null}
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
    setKind(editing?.kind || (investment ? "investment" : presetKind || "cash"));
    setBalance(majorString(editing?.balanceMinor, editing?.currency || finance.snapshot.defaultCurrency));
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
      unitPrice: valueMode === "units" ? unitPrice : "",
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
        <Field label={investment ? t("Worth") : kind === "credit_card" ? t("Balance owed") : t("Balance")} hint={t("Leave blank until you have checked it.")}>
          <Input inputMode="decimal" value={balance} onChange={(e) => setBalance(e.target.value)} className="h-16 text-3xl tabular-nums" />
        </Field>
        <Field label={t("Currency")} hint={t("Three letters, such as EUR or USD.")}>
          <Input value={currency} onChange={(e) => setCurrency(e.target.value.toUpperCase())} maxLength={3} autoCapitalize="characters" />
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
                {["stock", "fund", "crypto", "custom"].map((id) => (
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
                <Field label={t("Units")}>
                  <Input inputMode="decimal" value={units} onChange={(e) => setUnits(e.target.value)} />
                </Field>
                <Field label={t("Price each")}>
                  <Input inputMode="decimal" value={unitPrice} onChange={(e) => setUnitPrice(e.target.value)} />
                </Field>
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
  if (cadence === "daily") return "Daily";
  if (cadence === "weekly") return "Weekly";
  if (cadence === "monthly") return "Monthly";
  if (cadence === "yearly") return "Yearly";
  return "Once";
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
  onAddInvestment,
  onPay,
}: {
  accounts: FinanceAccount[];
  focus: WorthFocus | "all";
  transactions: FinanceTransaction[];
  rules: FinanceState["snapshot"]["rules"];
  onPaid: (id: string) => void;
  onOpen: (account: FinanceAccount) => void;
  onAddAccount: () => void;
  onAddCash: () => void;
  onAddDebt: () => void;
  onAddLoan: () => void;
  onAddInvestment: () => void;
  onPay: (account: FinanceAccount) => void;
}) {
  const t = useT();
  const bank = accounts.filter((account) => account.kind === "bank" || account.kind === "other");
  const cash = accounts.filter((account) => account.kind === "cash");
  const invested = accounts.filter((account) => account.kind === "investment");
  const cards = accounts.filter((account) => account.kind === "credit_card");
  const loans = accounts.filter((account) => account.kind === "loan" || account.kind === "other_debt");
  const show = (section: "accounts" | "cash" | "invested" | "debt") => {
    if (focus === "all") return true;
    if (focus === "owned") return section !== "debt";
    if (focus === "cash") return section === "cash";
    if (focus === "invested") return section === "invested";
    return section === "debt";
  };
  const block = (title: string, rows: FinanceAccount[], emptyTitle: string, emptyBody: string, action: string, onAdd: () => void, pay = false) => (
    <section key={title}>
      <h2 className="fin-kicker mb-2">{title}</h2>
      <AccountList rows={rows} transactions={transactions} rules={rules} onPaid={onPaid} emptyTitle={emptyTitle} emptyBody={emptyBody} action={action} onAdd={onAdd} onOpen={onOpen} onPay={pay ? onPay : undefined} />
    </section>
  );
  return (
    <div className="fin-stack w-full min-w-0 space-y-6">
      {show("accounts")
        ? block(t("Accounts"), bank, t("No accounts yet"), t("Add cash, a bank, a card, or a loan. A nickname is enough."), t("Add account"), onAddAccount)
        : null}
      {show("cash")
        ? block(t("Cash & savings"), cash, t("No accounts yet"), t("Add cash, a bank, a card, or a loan. A nickname is enough."), t("Add account"), onAddCash)
        : null}
      {show("invested")
        ? block(t("Investments"), invested, t("No investments yet"), t("Say “I invest 200 euros into VT every month”, or tap +."), t("Add investment"), onAddInvestment)
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
  const names = finance.snapshot.categories.map((category) => category.name);
  return (
    <Sheet open={open} title={t("Edit categories")} onClose={onClose} className="finance">
      <div className="space-y-3">
        {names.length === 0 && <p className="fin-muted text-sm">{t("A category is saved the first time you use it.")}</p>}
        {names.map((name) => (
          <div key={name} className="flex gap-2">
            <Input value={drafts[name] ?? name} onChange={(event) => setDrafts((current) => ({ ...current, [name]: event.target.value }))} />
            <Button type="button" onClick={() => finance.renameCategory(name, drafts[name] ?? name)}>{t("Rename")}</Button>
          </div>
        ))}
      </div>
    </Sheet>
  );
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
    setAmount(majorString(card.shownBalanceMinor, card.currency));
    setFromId("");
    setDate(todayIso());
    setError("");
  }
  const save = () => {
    if (!card || !amount.trim()) return;
    if (!fromId) {
      setError(t("Which account pays this card?"));
      return;
    }
    const pay = Math.round(Number(amount.replace(",", ".")) * 10 ** minorExponent(card.currency));
    if (Number.isFinite(pay) && paymentFits(card.shownBalanceMinor, pay) === "over") {
      setError(t("That is more than this card owes."));
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
  const owed = card?.shownBalanceMinor != null && card ? formatMoney(card.shownBalanceMinor, card.currency) : t("Not set");
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
        <p className="text-sm leading-5 text-muted-foreground">{t("A card payment is not a new expense. Cash and the balance owed both fall. Net worth stays the same.")}</p>
        <p className="fin-num text-3xl">{owed}</p>
        {card && cardDueDate(card, todayIso()) ? (
          <p className="fin-muted text-sm">{t("Next payment")} · {cardDueDate(card, todayIso())}</p>
        ) : null}
        <div className="chip-scroller">
          <Chip on={false} onClick={() => card && setAmount(majorString(card.shownBalanceMinor, card.currency))}>{t("Pay in full")}</Chip>
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
