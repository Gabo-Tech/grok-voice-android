import { useEffect, useMemo, useRef, useState } from "react";
import { EmptyState, Notice, PageChrome, RowButton } from "@/components/PageChrome";
import { Sheet } from "@/components/Sheet";
import { Field } from "@/components/Field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { FinanceCarousel, type ChartPick } from "@/components/FinanceCharts";
import type { FinanceState } from "@/hooks/useFinance";
import type { FinanceAccount, FinanceRule, FinanceTransaction } from "@/lib/api";
import { formatMoney, kindLabel, majorString, typeLabel } from "@/lib/money";
import { displayColor, filterTransactions, groupOf, isDebt, PALETTE, periodBounds, type PeriodId } from "@/lib/financeView";
import { debtPayoff, nextDue, plannedHits } from "@/lib/financeMath";
import { FinanceDashboard, plannedYearBars, todayIso, useFinanceRateWatch, type WorthFocus } from "@/components/FinanceDashboard";
import { FINANCE_TABS, readFinanceTab, type FinanceTab } from "@/lib/financeTabs";
import { ProgramSheet } from "@/components/ProgramSheet";
import { activeLocale, fill, useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import "@/finance.css";

type SheetKind = "transaction" | "account" | "investment" | "rule";

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
      <button type="button" className="fin-chip w-full" onClick={lock === "income" ? openIncome : openAdd}>
        {t(lock === "income" ? "Add income" : "Add expense")}
      </button>
      <button type="button" className="fin-chip w-full" onClick={openBill}>
        {t(lock === "income" ? "Program income" : "Program a bill")}
      </button>
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
              onAdd={openAdd}
              onTransaction={openTransaction}
              onRule={openRule}
              planned={[]}
              accounts={accounts}
            />
            {groupFilter !== "all" ? (
              <button type="button" className="fin-chip mb-2" onClick={() => setGroupFilter("all")}>
                {t("All")}
              </button>
            ) : null}
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
  return (
    <div>
      <div className="mb-2 flex gap-2 overflow-x-auto">
        {PERIODS.map((id) => (
          <Chip key={id} on={period === id} onClick={() => onPeriod(id)}>{t(periodLabel(id))}</Chip>
        ))}
      </div>
      <div className="mb-2 flex gap-2 overflow-x-auto">
        <Chip on={accountFilter === "all"} onClick={() => onAccountFilter("all")}>{t("All accounts")}</Chip>
        {accounts.map((account) => (
          <Chip key={account.id} on={accountFilter === account.id} onClick={() => onAccountFilter(account.id)}>{account.name}</Chip>
        ))}
      </div>
      <div className="mb-2 flex gap-2 overflow-x-auto">
        <Chip on={categoryFilter === "all"} onClick={() => onCategory("all")}>{t("All categories")}</Chip>
        {categories.map((category) => (
          <Chip key={category} on={categoryFilter === category} onClick={() => onCategory(category)}>{category}</Chip>
        ))}
        <Chip on={false} onClick={onEditCategories}>{t("Edit categories")}</Chip>
      </div>
      <div className="mb-2 flex gap-2 overflow-x-auto">
        <Chip on={currencyFilter === "all"} onClick={() => onCurrency("all")}>{t("All currencies")}</Chip>
        {currencies.map((currency) => (
          <Chip key={currency} on={currencyFilter === currency} onClick={() => onCurrency(currency)}>{currency}</Chip>
        ))}
      </div>
      <div className="mb-2 flex flex-wrap gap-2">
        <Chip on={classFilter === "all"} onClick={() => onClass("all")}>{t("All")}</Chip>
        <Chip on={classFilter === "recurring"} onClick={() => onClass("recurring")}>{t("Recurring")}</Chip>
        <Chip on={classFilter === "once"} onClick={() => onClass("once")}>{t("Once")}</Chip>
        <Chip on={classFilter === "irregular"} onClick={() => onClass("irregular")}>{t("Irregular")}</Chip>
      </div>
      {directionLock === "income" ? null : (
        <div className="mb-4 flex flex-wrap gap-2">
          {directionLock === "any" ? <Chip on={directionFilter === "all"} onClick={() => onDirection("all")}>{t("All")}</Chip> : null}
          <Chip on={directionFilter === "expense"} onClick={() => onDirection("expense")}>{t("Expense")}</Chip>
          {directionLock === "any" ? <Chip on={directionFilter === "income"} onClick={() => onDirection("income")}>{t("Income")}</Chip> : null}
          <Chip on={directionFilter === "payment"} onClick={() => onDirection("payment")}>{t("Payment")}</Chip>
        </div>
      )}
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
}) {
  if (rows.length === 0) return <EmptyState title={emptyTitle} body={emptyBody} actionLabel={action} onAction={onAdd} />;
  return <AccountRows rows={rows} onOpen={onOpen} transactions={transactions} rules={rules} onPaid={onPaid} />;
}

function AccountRows({
  rows,
  onOpen,
  transactions,
  rules,
  onPaid,
}: {
  rows: FinanceAccount[];
  onOpen: (account: FinanceAccount) => void;
  transactions: FinanceTransaction[];
  rules: FinanceState["snapshot"]["rules"];
  onPaid: (id: string) => void;
}) {
  const t = useT();
  return (
    <div className="space-y-3">
      {rows.map((account, index) => {
        const money = account.shownBalanceMinor == null ? t("Not set") : formatMoney(account.shownBalanceMinor, account.currency);
        const owed = isDebt(account.kind);
        const payoff = debtPayoff(account, transactions);
        const due = nextDue(account, rules);
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
  const [accountName, setAccountName] = useState("");
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
    setAccountName(finance.snapshot.accounts.find((account) => account.id === editing?.accountId)?.name || "");
    setDate(editing?.date || "");
    setRecurrence(editing?.recurrence || "once");
    setCadence("");
    setNotes(editing?.notes || "");
    setConfirming(false);
  }
  const save = () => {
    if (!amount.trim()) return;
    finance.saveTransaction({
      id: editing?.id,
      amount,
      currency,
      direction,
      category,
      accountName,
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
        <Field label={t("Amount")}>
          <Input inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} className="h-16 text-3xl tabular-nums" autoFocus />
        </Field>
        <Field label={t("Direction")}>
          <div className="flex gap-2 overflow-x-auto">
            <Chip on={direction === "expense"} onClick={() => setDirection("expense")}>{t("Expense")}</Chip>
            <Chip on={direction === "income"} onClick={() => setDirection("income")}>{t("Income")}</Chip>
            <Chip on={direction === "payment"} onClick={() => setDirection("payment")}>{t("Payment")}</Chip>
          </div>
        </Field>
        <Field label={t("Currency")} hint={t("Three letters, such as EUR or USD.")}>
          <Input value={currency} onChange={(e) => setCurrency(e.target.value.toUpperCase())} maxLength={3} autoCapitalize="characters" />
        </Field>
        <Field label={t("Category or payee")}>
          <Input value={category} onChange={(e) => setCategory(e.target.value)} placeholder={t("Lunch, rent, salary")} />
        </Field>
        <Field label={t("Account")} hint={t("Nickname")}>
          <Input value={accountName} onChange={(e) => setAccountName(e.target.value)} placeholder={t("Cash, Visa travel, car loan")} list="finance-accounts" />
          <datalist id="finance-accounts">
            {finance.snapshot.accounts.map((account) => (
              <option key={account.id} value={account.name} />
            ))}
          </datalist>
        </Field>
        <Field label={t("Date")}>
          <Input type="date" value={date} onChange={(e) => setDate(e.target.value)} />
        </Field>
        <Field label={t("How often")}>
          <div className="flex gap-2 overflow-x-auto">
            <Chip on={recurrence === "once"} onClick={() => setRecurrence("once")}>{t("Once")}</Chip>
            <Chip on={recurrence === "recurring"} onClick={() => setRecurrence("recurring")}>{t("Recurring")}</Chip>
            <Chip on={recurrence === "irregular"} onClick={() => setRecurrence("irregular")}>{t("Irregular")}</Chip>
          </div>
        </Field>
        {recurrence === "recurring" && (
          <div className="flex gap-2 overflow-x-auto">
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
    });
    onClose();
  };
  const cardOrLoan = isDebt(kind);
  return (
    <Sheet
      open={open}
      title={investment ? (editing ? t("Edit investment") : t("New investment")) : editing ? t("Edit account") : t("New account")}
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
            <div className="flex flex-wrap gap-2">
              {KINDS.map((id) => (
                <Chip key={id} on={kind === id} onClick={() => setKind(id)}>{t(kindLabel(id))}</Chip>
              ))}
            </div>
          </Field>
        )}
        <Field label={investment ? t("Worth") : t("Balance")} hint={t("Leave blank until you have checked it.")}>
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
              <div className="flex flex-wrap gap-2">
                {["stock", "fund", "crypto", "custom"].map((id) => (
                  <Chip key={id} on={investmentType === id} onClick={() => setInvestmentType(id)}>{t(typeLabel(id))}</Chip>
                ))}
              </div>
            </Field>
            <Field label={t("Value")}>
              <div className="flex gap-2">
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
          <div className="flex gap-2 overflow-x-auto">
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
      <div className="flex gap-2 overflow-x-auto">
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
  onAddInvestment,
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
  onAddInvestment: () => void;
}) {
  const t = useT();
  const bank = accounts.filter((account) => account.kind === "bank" || account.kind === "other");
  const cash = accounts.filter((account) => account.kind === "cash");
  const invested = accounts.filter((account) => account.kind === "investment");
  const debt = accounts.filter((account) => isDebt(account.kind));
  const show = (section: "accounts" | "cash" | "invested" | "debt") => {
    if (focus === "all") return true;
    if (focus === "owned") return section !== "debt";
    if (focus === "cash") return section === "cash";
    if (focus === "invested") return section === "invested";
    return section === "debt";
  };
  const block = (title: string, rows: FinanceAccount[], emptyTitle: string, emptyBody: string, action: string, onAdd: () => void) => (
    <section key={title}>
      <h2 className="fin-kicker mb-2">{title}</h2>
      <AccountList rows={rows} transactions={transactions} rules={rules} onPaid={onPaid} emptyTitle={emptyTitle} emptyBody={emptyBody} action={action} onAdd={onAdd} onOpen={onOpen} />
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
        ? block(t("You owe"), debt, t("No debts saved"), t("A card or loan. Payments reduce what you owe."), t("Add account"), onAddDebt)
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

function downloadBase64(name: string, data: string) {
  const bytes = Uint8Array.from(atob(data), (char) => char.charCodeAt(0));
  const url = URL.createObjectURL(new Blob([bytes], { type: "application/pdf" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  URL.revokeObjectURL(url);
}
