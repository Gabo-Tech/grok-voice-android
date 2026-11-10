import { useMemo, useState } from "react";
import { EmptyState, Notice, PageChrome, RowButton } from "@/components/PageChrome";
import { Sheet } from "@/components/Sheet";
import { Field } from "@/components/Field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import type { FinanceState } from "@/hooks/useFinance";
import type { FinanceAccount, FinanceRule, FinanceTransaction } from "@/lib/api";
import { formatMoney, kindLabel, majorString } from "@/lib/money";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

type Segment = "overview" | "activity" | "accounts" | "investments";
type SheetKind = "transaction" | "account" | "investment" | "rule";

const SEGMENTS: Segment[] = ["overview", "activity", "accounts", "investments"];
const KINDS = ["bank", "cash", "credit_card", "loan", "other"] as const;
const CADENCE = ["", "weekly", "monthly", "yearly"] as const;

function segmentLabel(segment: Segment): string {
  if (segment === "overview") return "Overview";
  if (segment === "activity") return "Activity";
  if (segment === "accounts") return "Accounts";
  return "Investments";
}

export function FinancePage({ finance, active = true }: { finance: FinanceState; active?: boolean }) {
  const t = useT();
  const [segment, setSegment] = useState<Segment>("overview");
  const [sheet, setSheet] = useState<SheetKind | null>(null);
  const [editingAccount, setEditingAccount] = useState<FinanceAccount | null>(null);
  const [editingTx, setEditingTx] = useState<FinanceTransaction | null>(null);
  const [editingRule, setEditingRule] = useState<FinanceRule | null>(null);
  const [classFilter, setClassFilter] = useState("all");
  const [directionFilter, setDirectionFilter] = useState("all");

  const accounts = finance.snapshot.accounts;
  const spending = accounts.filter((account) => account.kind !== "investment");
  const investments = accounts.filter((account) => account.kind === "investment");
  const rules = finance.snapshot.rules.filter((rule) => rule.cadence || rule.nextDate);
  const recent = finance.snapshot.transactions.slice(0, 5);
  const activity = useMemo(() => {
    return finance.snapshot.transactions.filter((tx) => {
      if (classFilter !== "all" && tx.recurrence !== classFilter) return false;
      if (directionFilter !== "all" && tx.direction !== directionFilter) return false;
      return true;
    });
  }, [finance.snapshot.transactions, classFilter, directionFilter]);

  const openAdd = () => {
    setEditingAccount(null);
    setEditingTx(null);
    setEditingRule(null);
    if (segment === "accounts") setSheet("account");
    else if (segment === "investments") setSheet("investment");
    else setSheet("transaction");
  };

  const addLabel = segment === "accounts" ? "Add account" : segment === "investments" ? "Add investment" : "Add transaction";
  const empty = accounts.length === 0 && finance.snapshot.transactions.length === 0 && finance.snapshot.rules.length === 0;

  return (
    <>
      <PageChrome
        title={t("Finance")}
        subtitle={t("On this phone. No bank login.")}
        onAdd={openAdd}
        addLabel={t(addLabel)}
        active={active}
      >
        <div className="mb-4 grid grid-cols-2 gap-2">
          {SEGMENTS.map((id) => (
            <Chip key={id} wide on={segment === id} onClick={() => setSegment(id)}>
              {t(segmentLabel(id))}
            </Chip>
          ))}
        </div>
        {finance.error && <Notice title={t(finance.error)} onDismiss={() => finance.setError(null)} />}
        {segment === "overview" && (
          <Overview
            finance={finance}
            empty={empty}
            spending={spending}
            investments={investments}
            recent={recent}
            onAdd={openAdd}
            onAccount={(account) => {
              setEditingAccount(account);
              setSheet(account.kind === "investment" ? "investment" : "account");
            }}
            onTransaction={(tx) => {
              setEditingTx(tx);
              setSheet("transaction");
            }}
          />
        )}
        {segment === "activity" && (
          <Activity
            rows={activity}
            rules={classFilter === "all" || classFilter === "recurring" ? rules : []}
            classFilter={classFilter}
            directionFilter={directionFilter}
            onClass={setClassFilter}
            onDirection={setDirectionFilter}
            onAdd={openAdd}
            onTransaction={(tx) => {
              setEditingTx(tx);
              setSheet("transaction");
            }}
            onRule={(rule) => {
              setEditingRule(rule);
              setSheet("rule");
            }}
            accounts={accounts}
          />
        )}
        {segment === "accounts" && (
          <AccountList
            rows={spending}
            emptyTitle={t("No accounts yet")}
            emptyBody={t("Add cash, a bank, a card, or a loan. A nickname is enough.")}
            action={t("Add account")}
            onAdd={() => {
              setEditingAccount(null);
              setSheet("account");
            }}
            onOpen={(account) => {
              setEditingAccount(account);
              setSheet("account");
            }}
          />
        )}
        {segment === "investments" && (
          <AccountList
            rows={investments}
            emptyTitle={t("No investments yet")}
            emptyBody={t("Say “I invest 200 euros into VT every month”, or tap +.")}
            action={t("Add investment")}
            onAdd={() => {
              setEditingAccount(null);
              setSheet("investment");
            }}
            onOpen={(account) => {
              setEditingAccount(account);
              setSheet("investment");
            }}
          />
        )}
      </PageChrome>
      <TransactionSheet
        open={sheet === "transaction"}
        finance={finance}
        editing={editingTx}
        onClose={() => setSheet(null)}
      />
      <AccountSheet
        open={sheet === "account" || sheet === "investment"}
        investment={sheet === "investment"}
        finance={finance}
        editing={editingAccount}
        onClose={() => setSheet(null)}
      />
      <RuleSheet open={sheet === "rule"} finance={finance} editing={editingRule} onClose={() => setSheet(null)} />
    </>
  );
}

function Overview({
  finance,
  empty,
  spending,
  investments,
  recent,
  onAdd,
  onAccount,
  onTransaction,
}: {
  finance: FinanceState;
  empty: boolean;
  spending: FinanceAccount[];
  investments: FinanceAccount[];
  recent: FinanceTransaction[];
  onAdd: () => void;
  onAccount: (account: FinanceAccount) => void;
  onTransaction: (tx: FinanceTransaction) => void;
}) {
  const t = useT();
  if (empty) {
    return (
      <EmptyState
        title={t("No money saved yet")}
        body={t("Say “I spent 12 euros on lunch” in Talk, or tap + to add one.")}
        actionLabel={t("Add transaction")}
        onAction={onAdd}
      />
    );
  }
  const totals = finance.snapshot.totals;
  return (
    <div className="space-y-6">
      <p className="text-sm leading-5 text-muted-foreground">{t("Balances are what you last said. Nothing here connects to a bank.")}</p>
      <section>
        <h2 className="text-sm text-muted-foreground">{t("This month")}</h2>
        {totals.length === 0 ? (
          <p className="mt-2 text-base text-foreground">{t("No activity yet")}</p>
        ) : (
          <div className="mt-3 space-y-4">
            {totals.map((row) => (
              <div key={row.currency}>
                <AmountLine label={t("Income")} amount={formatMoney(row.incomeMinor, row.currency)} />
                <AmountLine label={t("Expenses")} amount={formatMoney(row.expenseMinor, row.currency)} />
                {row.paymentMinor > 0 && <AmountLine label={t("Payments")} amount={formatMoney(row.paymentMinor, row.currency)} />}
              </div>
            ))}
          </div>
        )}
      </section>
      {spending.length > 0 && (
        <section>
          <h2 className="mb-2 text-sm text-muted-foreground">{t("Accounts")}</h2>
          <AccountRows rows={spending} onOpen={onAccount} />
        </section>
      )}
      {investments.length > 0 && (
        <section>
          <h2 className="mb-2 text-sm text-muted-foreground">{t("Investments")}</h2>
          <AccountRows rows={investments} onOpen={onAccount} />
        </section>
      )}
      <section>
        <h2 className="mb-2 text-sm text-muted-foreground">{t("Upcoming")}</h2>
        {finance.snapshot.upcoming.length === 0 ? (
          <p className="text-base leading-6 text-muted-foreground">{t("A due date, payday, or subscription shows here and on Calendar.")}</p>
        ) : (
          <div className="divide-y divide-border border-y border-border">
            {finance.snapshot.upcoming.map((row) => (
              <div key={`${row.source}-${row.id}`} className="flex min-h-12 items-baseline justify-between gap-3 py-3">
                <div className="min-w-0">
                  <p className="truncate text-base text-foreground">{row.title}</p>
                  <p className="text-sm text-muted-foreground">
                    {row.date}
                    {row.calendarEventId ? ` · ${t("On Calendar")}` : ""}
                  </p>
                </div>
                <p className="shrink-0 text-lg tabular-nums">{row.amountMinor != null ? formatMoney(row.amountMinor, row.currency) : ""}</p>
              </div>
            ))}
          </div>
        )}
      </section>
      {recent.length > 0 && (
        <section>
          <h2 className="mb-2 text-sm text-muted-foreground">{t("Recent")}</h2>
          <TxRows rows={recent} accounts={finance.snapshot.accounts} onOpen={onTransaction} />
        </section>
      )}
    </div>
  );
}

function AmountLine({ label, amount }: { label: string; amount: string }) {
  return (
    <div className="flex items-baseline justify-between gap-3 py-1">
      <span className="text-base text-muted-foreground">{label}</span>
      <span className="min-w-0 text-right text-2xl leading-tight font-medium tracking-tight break-words tabular-nums">{amount}</span>
    </div>
  );
}

function Activity({
  rows,
  rules,
  classFilter,
  directionFilter,
  onClass,
  onDirection,
  onAdd,
  onTransaction,
  onRule,
  accounts,
}: {
  rows: FinanceTransaction[];
  rules: FinanceRule[];
  classFilter: string;
  directionFilter: string;
  onClass: (value: string) => void;
  onDirection: (value: string) => void;
  onAdd: () => void;
  onTransaction: (tx: FinanceTransaction) => void;
  onRule: (rule: FinanceRule) => void;
  accounts: FinanceAccount[];
}) {
  const t = useT();
  return (
    <div>
      <div className="mb-2 flex flex-wrap gap-2">
        <Chip on={classFilter === "all"} onClick={() => onClass("all")}>{t("All")}</Chip>
        <Chip on={classFilter === "recurring"} onClick={() => onClass("recurring")}>{t("Recurring")}</Chip>
        <Chip on={classFilter === "once"} onClick={() => onClass("once")}>{t("Once")}</Chip>
        <Chip on={classFilter === "irregular"} onClick={() => onClass("irregular")}>{t("Irregular")}</Chip>
      </div>
      <div className="mb-4 flex flex-wrap gap-2">
        <Chip on={directionFilter === "all"} onClick={() => onDirection("all")}>{t("All")}</Chip>
        <Chip on={directionFilter === "expense"} onClick={() => onDirection("expense")}>{t("Expense")}</Chip>
        <Chip on={directionFilter === "income"} onClick={() => onDirection("income")}>{t("Income")}</Chip>
        <Chip on={directionFilter === "payment"} onClick={() => onDirection("payment")}>{t("Payment")}</Chip>
      </div>
      {rules.length > 0 && (
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
      {rows.length === 0 ? (
        <EmptyState title={t("Nothing in this filter")} body={t("Try another filter, or log it in Talk.")} actionLabel={t("Add transaction")} onAction={onAdd} />
      ) : (
        <TxRows rows={rows} accounts={accounts} onOpen={onTransaction} />
      )}
    </div>
  );
}

function AccountList({
  rows,
  emptyTitle,
  emptyBody,
  action,
  onAdd,
  onOpen,
}: {
  rows: FinanceAccount[];
  emptyTitle: string;
  emptyBody: string;
  action: string;
  onAdd: () => void;
  onOpen: (account: FinanceAccount) => void;
}) {
  if (rows.length === 0) return <EmptyState title={emptyTitle} body={emptyBody} actionLabel={action} onAction={onAdd} />;
  return <AccountRows rows={rows} onOpen={onOpen} />;
}

function AccountRows({ rows, onOpen }: { rows: FinanceAccount[]; onOpen: (account: FinanceAccount) => void }) {
  const t = useT();
  return (
    <div className="divide-y divide-border border-y border-border">
      {rows.map((account) => {
        const money = account.shownBalanceMinor == null ? t("Not set") : formatMoney(account.shownBalanceMinor, account.currency);
        const owed = account.kind === "credit_card" || account.kind === "loan";
        return (
          <RowButton key={account.id} onClick={() => onOpen(account)}>
            <div className="min-w-0 flex-1">
              <p className="truncate text-base">{account.name}</p>
              <p className="text-sm text-muted-foreground">
                {t(kindLabel(account.kind))}
                {owed && account.shownBalanceMinor != null ? ` · ${t("Owed")}` : ""}
                {account.kind === "investment" && account.shownBalanceMinor != null ? ` · ${t("Worth")}` : ""}
                {account.dueDate ? ` · ${account.dueDate}` : ""}
                {account.contributionDate ? ` · ${account.contributionDate}` : ""}
                {account.calendarEventId ? ` · ${t("On Calendar")}` : ""}
              </p>
            </div>
            <p className={cn("shrink-0 text-right text-xl leading-6 font-medium tabular-nums", account.shownBalanceMinor == null && "text-base font-normal text-muted-foreground")}>{money}</p>
          </RowButton>
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
}: {
  open: boolean;
  finance: FinanceState;
  editing: FinanceTransaction | null;
  onClose: () => void;
}) {
  const t = useT();
  const [amount, setAmount] = useState("");
  const [currency, setCurrency] = useState(finance.snapshot.defaultCurrency || "USD");
  const [direction, setDirection] = useState("expense");
  const [category, setCategory] = useState("");
  const [accountName, setAccountName] = useState("");
  const [date, setDate] = useState("");
  const [recurrence, setRecurrence] = useState("once");
  const [cadence, setCadence] = useState("");
  const [notes, setNotes] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [seen, setSeen] = useState<string | null>(null);
  const key = editing?.id || "new";
  if (!open && seen !== null) setSeen(null);
  if (open && seen !== key) {
    setSeen(key);
    setAmount(editing ? majorString(editing.amountMinor, editing.currency) : "");
    setCurrency(editing?.currency || finance.snapshot.defaultCurrency || "USD");
    setDirection(editing?.direction || "expense");
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
  onClose,
}: {
  open: boolean;
  investment: boolean;
  finance: FinanceState;
  editing: FinanceAccount | null;
  onClose: () => void;
}) {
  const t = useT();
  const [name, setName] = useState("");
  const [kind, setKind] = useState(investment ? "investment" : "cash");
  const [balance, setBalance] = useState("");
  const [currency, setCurrency] = useState(finance.snapshot.defaultCurrency || "USD");
  const [institution, setInstitution] = useState("");
  const [dueDate, setDueDate] = useState("");
  const [payment, setPayment] = useState("");
  const [cadence, setCadence] = useState("");
  const [ticker, setTicker] = useState("");
  const [contribution, setContribution] = useState("");
  const [contributionCadence, setContributionCadence] = useState("");
  const [contributionDate, setContributionDate] = useState("");
  const [notes, setNotes] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [seen, setSeen] = useState<string | null>(null);
  const key = `${investment ? "i" : "a"}:${editing?.id || "new"}`;
  if (!open && seen !== null) setSeen(null);
  if (open && seen !== key) {
    setSeen(key);
    setName(editing?.name || "");
    setKind(editing?.kind || (investment ? "investment" : "cash"));
    setBalance(majorString(editing?.balanceMinor, editing?.currency || finance.snapshot.defaultCurrency || "USD"));
    setCurrency(editing?.currency || finance.snapshot.defaultCurrency || "USD");
    setInstitution(editing?.institution || "");
    setDueDate(editing?.dueDate || "");
    setPayment(majorString(editing?.paymentMinor, editing?.currency || finance.snapshot.defaultCurrency || "USD"));
    setCadence(editing?.cadence || "");
    setTicker(editing?.ticker || "");
    setContribution(majorString(editing?.contributionMinor, editing?.currency || finance.snapshot.defaultCurrency || "USD"));
    setContributionCadence(editing?.contributionCadence || "");
    setContributionDate(editing?.contributionDate || "");
    setNotes(editing?.notes || "");
    setConfirming(false);
  }
  const save = () => {
    if (!name.trim()) return;
    finance.saveAccount({
      id: editing?.id,
      name,
      kind: investment ? "investment" : kind,
      balance,
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
    });
    onClose();
  };
  const cardOrLoan = kind === "credit_card" || kind === "loan";
  return (
    <Sheet
      open={open}
      title={investment ? (editing ? t("Edit investment") : t("New investment")) : editing ? t("Edit account") : t("New account")}
      onClose={onClose}
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
        <Field label={t("Notes")}>
          <Textarea value={notes} onChange={(e) => setNotes(e.target.value)} />
        </Field>
      </div>
    </Sheet>
  );
}

function RuleSheet({
  open,
  finance,
  editing,
  onClose,
}: {
  open: boolean;
  finance: FinanceState;
  editing: FinanceRule | null;
  onClose: () => void;
}) {
  const t = useT();
  const [name, setName] = useState("");
  const [amount, setAmount] = useState("");
  const [currency, setCurrency] = useState(finance.snapshot.defaultCurrency || "USD");
  const [direction, setDirection] = useState("expense");
  const [cadence, setCadence] = useState("monthly");
  const [nextDate, setNextDate] = useState("");
  const [notes, setNotes] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [seen, setSeen] = useState<string | null>(null);
  const key = editing?.id || "";
  if (!open && seen !== null) setSeen(null);
  if (open && editing && seen !== key) {
    setSeen(key);
    setName(editing.name);
    setAmount(majorString(editing.amountMinor, editing.currency));
    setCurrency(editing.currency || finance.snapshot.defaultCurrency || "USD");
    setDirection(editing.direction || "expense");
    setCadence(editing.cadence || "");
    setNextDate(editing.nextDate || "");
    setNotes(editing.notes || "");
    setConfirming(false);
  }
  if (!editing) return null;
  return (
    <Sheet
      open={open}
      title={editing.name}
      onClose={onClose}
      footer={
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            onClick={() => {
              if (!confirming) {
                setConfirming(true);
                return;
              }
              finance.removeRule(editing.id);
              onClose();
            }}
          >
            {confirming ? t("Delete this") : t("Delete")}
          </Button>
          <div className="flex-1" />
          <Button variant="ghost" onClick={onClose}>{t("Cancel")}</Button>
          <Button
            disabled={finance.busy || !name.trim()}
            onClick={() => {
              finance.saveRule({ id: editing.id, name, amount, currency, direction, cadence, nextDate, notes });
              onClose();
            }}
          >
            {t("Save")}
          </Button>
        </div>
      }
    >
      <div className="space-y-4">
        <Field label={t("Nickname")}>
          <Input value={name} onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field label={t("Amount")}>
          <Input inputMode="decimal" value={amount} onChange={(e) => setAmount(e.target.value)} className="h-16 text-3xl tabular-nums" />
        </Field>
        <Field label={t("Currency")} hint={t("Three letters, such as EUR or USD.")}>
          <Input value={currency} onChange={(e) => setCurrency(e.target.value.toUpperCase())} maxLength={3} autoCapitalize="characters" />
        </Field>
        <Field label={t("Direction")}>
          <div className="flex gap-2 overflow-x-auto">
            <Chip on={direction === "expense"} onClick={() => setDirection("expense")}>{t("Expense")}</Chip>
            <Chip on={direction === "income"} onClick={() => setDirection("income")}>{t("Income")}</Chip>
            <Chip on={direction === "payment"} onClick={() => setDirection("payment")}>{t("Payment")}</Chip>
          </div>
        </Field>
        <CadenceChips value={cadence} onChange={setCadence} />
        <Field label={t("Date")}>
          <Input type="date" value={nextDate} onChange={(e) => setNextDate(e.target.value)} />
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
      className={cn(
        "min-h-12 px-3 text-sm leading-5",
        wide ? "w-full" : "shrink-0",
        on ? "bg-foreground text-background" : "border border-border text-foreground",
      )}
    >
      {children}
    </button>
  );
}

function cadenceLabel(cadence: string): string {
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
