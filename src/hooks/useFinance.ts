import { useCallback, useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  api,
  errorText,
  type FinanceAccount,
  type FinanceAccountDraft,
  type FinanceRule,
  type FinanceRuleDraft,
  type FinanceSnapshot,
  type FinanceTransaction,
  type FinancePdfRequest,
  type FinanceTransactionDraft,
} from "@/lib/api";
import { emptyFinance, withShown } from "@/lib/money";

function phoneLocale(): string {
  return typeof navigator === "undefined" ? "" : navigator.language;
}

let previewId = 1;

export function useFinance() {
  const inApp = isTauri();
  const [snapshot, setSnapshot] = useState<FinanceSnapshot>(() => emptyFinance(phoneLocale()));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    if (!inApp) return;
    api
      .financeSnapshot(phoneLocale())
      .then((next) => {
        setSnapshot(next);
        setError(null);
      })
      .catch((err) => setError(errorText(err)));
  }, [inApp]);

  useEffect(() => {
    load();
    if (!inApp) return;
    let unlisten: (() => void) | undefined;
    void listen("voice://finance", () => load()).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [inApp, load]);

  const run = useCallback(
    async (work: () => Promise<FinanceSnapshot>) => {
      setBusy(true);
      try {
        setSnapshot(await work());
        setError(null);
      } catch (err) {
        setError(errorText(err));
      } finally {
        setBusy(false);
      }
    },
    [],
  );

  const saveAccount = useCallback(
    (draft: FinanceAccountDraft) => {
      if (!inApp) {
        setSnapshot((current) => withShown(previewAccount(current, draft)));
        return;
      }
      void run(() => api.saveFinanceAccount(draft));
    },
    [inApp, run],
  );

  const saveTransaction = useCallback(
    (draft: FinanceTransactionDraft) => {
      if (!inApp) {
        setSnapshot((current) => withShown(previewTransaction(current, draft)));
        return;
      }
      void run(() => api.saveFinanceTransaction(draft));
    },
    [inApp, run],
  );

  const saveRule = useCallback(
    (draft: FinanceRuleDraft) => {
      if (!inApp) {
        setSnapshot((current) => withShown(previewRule(current, draft)));
        return;
      }
      void run(() => api.saveFinanceRule(draft));
    },
    [inApp, run],
  );

  const removeAccount = useCallback(
    (id: string) => {
      if (!inApp) {
        setSnapshot((current) =>
          withShown({
            ...current,
            accounts: current.accounts.filter((account) => account.id !== id),
            rules: current.rules.filter((rule) => !(rule.fromAccount && rule.accountId === id)),
          }),
        );
        return;
      }
      void run(() => api.deleteFinanceAccount(id));
    },
    [inApp, run],
  );

  const removeTransaction = useCallback(
    (id: string) => {
      if (!inApp) {
        setSnapshot((current) => withShown({ ...current, transactions: current.transactions.filter((tx) => tx.id !== id) }));
        return;
      }
      void run(() => api.deleteFinanceTransaction(id));
    },
    [inApp, run],
  );

  const removeRule = useCallback(
    (id: string) => {
      if (!inApp) {
        setSnapshot((current) => withShown({ ...current, rules: current.rules.filter((rule) => rule.id !== id) }));
        return;
      }
      void run(() => api.deleteFinanceRule(id));
    },
    [inApp, run],
  );

  const setDefaultCurrency = useCallback(
    (currency: string) => {
      if (!inApp) {
        setSnapshot((current) => ({ ...current, defaultCurrency: currency.toUpperCase() }));
        return;
      }
      void run(() => api.setFinanceDefaultCurrency(currency));
    },
    [inApp, run],
  );

  const renameCategory = useCallback(
    (from: string, to: string) => {
      if (!inApp) {
        setSnapshot((current) => ({
          ...current,
          categories: current.categories.map((category) => (category.name === from ? { ...category, name: to } : category)),
          transactions: current.transactions.map((tx) => (tx.category === from ? { ...tx, category: to } : tx)),
        }));
        return;
      }
      void run(() => api.renameFinanceCategory(from, to));
    },
    [inApp, run],
  );

  const markPaid = useCallback(
    (id: string) => {
      if (!inApp) return;
      void run(() => api.markFinancePaid(id));
    },
    [inApp, run],
  );

  const exportPdf = useCallback((filter: FinancePdfRequest) => api.exportFinancePdf(filter), []);

  return {
    snapshot,
    error,
    busy,
    inApp,
    load,
    saveAccount,
    saveTransaction,
    saveRule,
    removeAccount,
    removeTransaction,
    removeRule,
    setDefaultCurrency,
    renameCategory,
    markPaid,
    exportPdf,
    setError,
  };
}

export type FinanceState = ReturnType<typeof useFinance>;

function nextId(prefix: string): string {
  previewId += 1;
  return `${prefix}${previewId}`;
}

function today(): string {
  const date = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function minorOf(raw: string, currency: string): number | null {
  const cleaned = raw.trim().replace(",", ".");
  if (!cleaned) return null;
  const value = Number(cleaned);
  if (!Number.isFinite(value) || value <= 0) return null;
  const exp = currency === "JPY" ? 0 : 2;
  return Math.round(value * 10 ** exp);
}

function previewAccount(current: FinanceSnapshot, draft: FinanceAccountDraft): FinanceSnapshot {
  const currency = (draft.currency || current.defaultCurrency).toUpperCase();
  const balance = minorOf(draft.balance || "", currency);
  const now = Date.now();
  const existing = current.accounts.find((account) => account.id === draft.id);
  const account: FinanceAccount = {
    id: existing?.id || draft.id || nextId("fa"),
    name: draft.name.trim(),
    kind: draft.kind || "other",
    institution: draft.institution || "",
    balanceMinor: draft.balance?.trim() ? balance : null,
    currency,
    balanceAsOf: draft.balance?.trim() ? today() : "",
    balanceUpdatedMs: draft.balance?.trim() ? now : 0,
    dueDate: draft.dueDate || "",
    paymentMinor: minorOf(draft.payment || "", currency),
    cadence: draft.cadence || "",
    notes: draft.notes || "",
    updatedMs: now,
    ticker: draft.ticker || "",
    contributionMinor: minorOf(draft.contribution || "", currency),
    contributionCadence: draft.contributionCadence || "",
    contributionDate: draft.contributionDate || "",
    calendarEventId: draft.dueDate || draft.contributionDate ? `local-fin-${existing?.id || "preview"}` : "",
    color: draft.color || existing?.color || "#C4B5A0",
    investmentType: draft.investmentType || existing?.investmentType || "",
    units: draft.valueMode === "stated" ? "" : draft.units || "",
    priceMinor: draft.valueMode === "stated" ? null : minorOf(draft.unitPrice || "", currency),
    leadDays: draft.leadDays ?? existing?.leadDays ?? 2,
    shownBalanceMinor: null,
  };
  const accounts = existing ? current.accounts.map((row) => (row.id === account.id ? account : row)) : [...current.accounts, account];
  const dated = account.kind === "investment" ? account.contributionDate : account.dueDate;
  const cadence = account.kind === "investment" ? account.contributionCadence : account.cadence;
  const amount = account.kind === "investment" ? account.contributionMinor : account.paymentMinor;
  const rules = current.rules.filter((rule) => !(rule.fromAccount && rule.accountId === account.id));
  if (dated || cadence) {
    rules.push({
      id: nextId("fr"),
      name: account.name,
      amountMinor: amount,
      currency: account.currency,
      direction: account.kind === "credit_card" || account.kind === "loan" || account.kind === "other_debt" || account.kind === "investment" ? "payment" : "expense",
      cadence,
      weekdays: [],
      nextDate: dated,
      category: account.kind,
      accountId: account.id,
      notes: "",
      calendarEventId: dated ? account.calendarEventId : "",
      updatedMs: now,
      fromAccount: true,
      leadDays: account.leadDays,
      reminderId: 0,
    });
  }
  return { ...current, accounts, rules };
}

function previewTransaction(current: FinanceSnapshot, draft: FinanceTransactionDraft): FinanceSnapshot {
  const currency = (draft.currency || current.defaultCurrency).toUpperCase();
  const amount = minorOf(draft.amount, currency);
  if (!amount) return current;
  const wanted = (draft.accountName || "").trim();
  let accounts = current.accounts;
  let named = accounts.find((account) => account.name.toLowerCase() === wanted.toLowerCase());
  if (wanted && !named) {
    named = {
      id: nextId("fa"),
      name: wanted,
      kind: /cash|wallet/i.test(wanted) ? "cash" : /visa|card|amex/i.test(wanted) ? "credit_card" : "other",
      institution: "",
      balanceMinor: null,
      currency,
      balanceAsOf: "",
      balanceUpdatedMs: 0,
      dueDate: "",
      paymentMinor: null,
      cadence: "",
      notes: "",
      updatedMs: Date.now(),
      ticker: "",
      contributionMinor: null,
      contributionCadence: "",
      contributionDate: "",
      calendarEventId: "",
      color: "#C4B5A0",
      investmentType: "",
      units: "",
      priceMinor: null,
      leadDays: 2,
      shownBalanceMinor: null,
    };
    accounts = [...accounts, named];
  }
  const tx: FinanceTransaction = {
    id: draft.id || nextId("ft"),
    amountMinor: amount,
    currency,
    direction: draft.direction,
    category: draft.category || "",
    accountId: named?.id || "",
    fromAccountId: "",
    date: draft.date || today(),
    notes: draft.notes || "",
    recurrence: draft.recurrence || "once",
    ruleId: "",
    createdMs: Date.now(),
    updatedMs: Date.now(),
  };
  const transactions = draft.id ? current.transactions.map((row) => (row.id === draft.id ? { ...tx, id: draft.id } : row)) : [tx, ...current.transactions];
  let rules = current.rules;
  if ((draft.recurrence === "recurring" || draft.cadence) && draft.cadence) {
    const rule: FinanceRule = {
      id: nextId("fr"),
      name: draft.category || draft.direction,
      amountMinor: amount,
      currency,
      direction: draft.direction,
      cadence: draft.cadence,
      weekdays: [],
      nextDate: draft.date || "",
      category: draft.category || "",
      accountId: named?.id || "",
      notes: "",
      calendarEventId: draft.date ? "local-fin-preview" : "",
      updatedMs: Date.now(),
      fromAccount: false,
      leadDays: 2,
      reminderId: 0,
    };
    rules = [...rules, rule];
  }
  return { ...current, accounts, transactions, rules };
}

function previewRule(current: FinanceSnapshot, draft: FinanceRuleDraft): FinanceSnapshot {
  const currency = (draft.currency || current.defaultCurrency).toUpperCase();
  const rule: FinanceRule = {
    id: draft.id || nextId("fr"),
    name: draft.name.trim(),
    amountMinor: minorOf(draft.amount || "", currency),
    currency,
    direction: draft.direction || "expense",
    cadence: draft.cadence || "",
    weekdays: draft.weekdays || [],
    nextDate: draft.nextDate || "",
    category: draft.category || "",
    accountId: "",
    notes: draft.notes || "",
    calendarEventId: draft.nextDate ? "local-fin-preview" : "",
    updatedMs: Date.now(),
    fromAccount: false,
    leadDays: draft.leadDays ?? 2,
    reminderId: 0,
  };
  const rules = draft.id ? current.rules.map((row) => (row.id === draft.id ? rule : row)) : [...current.rules, rule];
  return { ...current, rules };
}
