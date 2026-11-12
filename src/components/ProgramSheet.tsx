import { useState } from "react";
import { Sheet } from "@/components/Sheet";
import { Field } from "@/components/Field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { FinanceState } from "@/hooks/useFinance";
import type { FinanceRule } from "@/lib/api";
import { todayIso } from "@/components/FinanceDashboard";
import { majorString } from "@/lib/money";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

type Preset = "course" | "rent" | "subscription" | "income" | "payout" | "once";

const PRESETS: Preset[] = ["course", "rent", "subscription", "income", "payout", "once"];

export function ProgramSheet({
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
  const [step, setStep] = useState(0);
  const [preset, setPreset] = useState<Preset>("course");
  const [name, setName] = useState("");
  const [amount, setAmount] = useState("");
  const [currency, setCurrency] = useState(finance.snapshot.defaultCurrency || "");
  const [direction, setDirection] = useState("expense");
  const [purpose, setPurpose] = useState("recurring");
  const [cadence, setCadence] = useState("monthly");
  const [interval, setInterval] = useState("10");
  const [endKind, setEndKind] = useState("after_count");
  const [times, setTimes] = useState("9");
  const [endDate, setEndDate] = useState("");
  const [start, setStart] = useState("");
  const [amountMode, setAmountMode] = useState("fixed");
  const [payoutCurrency, setPayoutCurrency] = useState("XMR");
  const [accountId, setAccountId] = useState("");
  const [leadDays, setLeadDays] = useState(2);
  const [category, setCategory] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [seen, setSeen] = useState<string | null>(null);
  const key = editing?.id || "new";
  if (!open && seen !== null) setSeen(null);
  if (open && seen !== key) {
    setSeen(key);
    setStep(editing ? 4 : 0);
    setPreset(presetOf(editing));
    setName(editing?.name || "");
    setAmount(editing ? majorString(editing.amountMinor, editing.currency) : "");
    setCurrency(editing?.currency || finance.snapshot.defaultCurrency || "");
    setDirection(editing?.direction || "expense");
    setPurpose(editing?.purpose || "recurring");
    setCadence(editing?.cadence || "monthly");
    setInterval(String(editing?.intervalDays || 10));
    setEndKind(editing?.endKind || (editing?.cadence ? "none" : "after_count"));
    setTimes(String(editing?.endCount || 9));
    setEndDate(editing?.endDate || "");
    setStart(editing?.startDate || editing?.nextDate || "");
    setAmountMode(editing?.amountMode || "fixed");
    setPayoutCurrency(editing?.payoutCurrency || "XMR");
    setAccountId(editing?.accountId || "");
    setLeadDays(editing?.leadDays ?? 2);
    setCategory(editing?.category || "");
    setConfirming(false);
  }

  const choose = (next: Preset) => {
    setPreset(next);
    if (next === "course") {
      setDirection("expense");
      setPurpose("recurring");
      setCadence("monthly");
      setEndKind("after_count");
      setTimes("9");
      if (!name) setName("Course");
    } else if (next === "rent") {
      setDirection("expense");
      setPurpose("recurring");
      setCadence("monthly");
      setEndKind("none");
      if (!name) setName("Rent");
    } else if (next === "subscription") {
      setDirection("expense");
      setPurpose("subscription");
      setCadence("monthly");
      setEndKind("none");
    } else if (next === "income") {
      setDirection("income");
      setPurpose("recurring");
      setCadence("monthly");
      setEndKind("none");
    } else if (next === "payout") {
      setDirection("income");
      setPurpose("investment_payout");
      setCadence("every_n_days");
      setInterval("10");
      setEndKind("none");
      setAmountMode("quote");
      setPayoutCurrency("XMR");
    } else {
      setDirection("expense");
      setPurpose("one_off");
      setCadence("");
      setEndKind("after_count");
      setTimes("1");
    }
    setStep(1);
  };

  const save = () => {
    if (!name.trim() || !amount.trim()) return;
    const when = start || todayIso();
    const account = finance.snapshot.accounts.find((row) => row.id === accountId);
    finance.saveRule({
      id: editing?.id,
      name: name.trim(),
      amount,
      currency,
      direction,
      cadence,
      nextDate: when,
      startDate: when,
      purpose,
      endKind: purpose === "one_off" ? "after_count" : endKind,
      endCount: purpose === "one_off" || endKind === "after_count" ? Number(times) || 1 : 0,
      endDate: endKind === "on_date" ? endDate : "",
      intervalDays: cadence === "every_n_days" ? Number(interval) || 10 : 0,
      amountMode: purpose === "investment_payout" ? amountMode : "fixed",
      payoutCurrency: purpose === "investment_payout" ? payoutCurrency : "",
      accountId: account?.id || "",
      accountName: account?.name || "",
      accountKind: purpose === "investment_payout" ? "investment" : "",
      category,
      leadDays,
      status: editing?.status === "paused" || editing?.status === "stopped" ? editing.status : "active",
    });
    onClose();
  };

  const investments = finance.snapshot.accounts.filter((account) => account.kind === "investment");
  const accounts = purpose === "investment_payout" ? investments : finance.snapshot.accounts;

  return (
    <Sheet
      open={open}
      title={editing ? editing.name : t("Program")}
      onClose={onClose}
      className="finance"
      footer={
        <div className="flex items-center gap-2">
          {editing ? (
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
          ) : null}
          {step > 0 ? (
            <Button variant="ghost" onClick={() => setStep((value) => Math.max(0, value - 1))}>
              {t("Back")}
            </Button>
          ) : null}
          <div className="flex-1" />
          {editing && editing.status !== "stopped" ? (
            <Button variant="ghost" onClick={() => { finance.scheduleAction(editing.id, editing.status === "paused" ? "resume" : "pause"); onClose(); }}>
              {editing.status === "paused" ? t("Resume") : t("Pause")}
            </Button>
          ) : null}
          {editing && editing.status !== "stopped" ? (
            <Button variant="ghost" onClick={() => { finance.scheduleAction(editing.id, "stop"); onClose(); }}>
              {t("Stop")}
            </Button>
          ) : null}
          {step < 4 ? <Button onClick={() => setStep((value) => value + 1)}>{t("Next")}</Button> : null}
          <Button disabled={finance.busy || !name.trim() || !amount.trim()} onClick={save}>
            {t("Save")}
          </Button>
        </div>
      }
    >
      {step === 0 ? (
        <div className="space-y-3">
          <p className="fin-muted text-sm leading-5">{t("One plan at a time. You can change it later.")}</p>
          {PRESETS.map((id) => (
            <button key={id} type="button" className={cn("fin-panel w-full text-left", preset === id && "outline outline-1")} onClick={() => choose(id)}>
              <p className="text-base font-extrabold tracking-wide uppercase">{t(presetTitle(id))}</p>
              <p className="fin-muted mt-1 text-sm leading-5">{t(presetBody(id))}</p>
            </button>
          ))}
        </div>
      ) : null}
      {step === 1 ? (
        <div className="space-y-4">
          <Field label={t("Amount")}>
            <Input inputMode="decimal" value={amount} onChange={(event) => setAmount(event.target.value)} className="h-16 text-3xl tabular-nums" />
          </Field>
          <Field label={t("Currency")}>
            <Input value={currency} maxLength={8} autoCapitalize="characters" onChange={(event) => setCurrency(event.target.value.toUpperCase())} />
          </Field>
          {purpose === "investment_payout" ? (
            <>
              <div className="flex gap-2">
                <Chip on={amountMode === "fixed"} onClick={() => setAmountMode("fixed")}>{t("Fixed amount in that currency")}</Chip>
                <Chip on={amountMode === "quote"} onClick={() => setAmountMode("quote")}>{t("A value in one currency, paid in another")}</Chip>
              </div>
              {amountMode === "quote" ? (
                <Field label={t("Payout currency")}>
                  <Input value={payoutCurrency} maxLength={8} autoCapitalize="characters" onChange={(event) => setPayoutCurrency(event.target.value.toUpperCase())} />
                </Field>
              ) : null}
              <p className="fin-muted text-sm leading-5">{t("The principal stays what you entered.")}</p>
            </>
          ) : null}
        </div>
      ) : null}
      {step === 2 ? (
        <Field label={t("How often")}>
          <div className="flex flex-wrap gap-2">
            <Chip on={cadence === ""} onClick={() => { setCadence(""); setPurpose("one_off"); setEndKind("after_count"); setTimes("1"); }}>{t("Once")}</Chip>
            <Chip on={cadence === "weekly"} onClick={() => setCadence("weekly")}>{t("Weekly")}</Chip>
            <Chip on={cadence === "monthly"} onClick={() => setCadence("monthly")}>{t("Monthly")}</Chip>
            <Chip on={cadence === "yearly"} onClick={() => setCadence("yearly")}>{t("Yearly")}</Chip>
            <Chip on={cadence === "every_n_days"} onClick={() => setCadence("every_n_days")}>{t("Every N days")}</Chip>
          </div>
          {cadence === "every_n_days" ? (
            <Field label={t("Days apart")}>
              <Input inputMode="numeric" value={interval} onChange={(event) => setInterval(event.target.value)} />
            </Field>
          ) : null}
        </Field>
      ) : null}
      {step === 3 ? (
        <div className="space-y-4">
          <p className="fin-kicker">{t("How long")}</p>
          {purpose === "one_off" || cadence === "" ? (
            <p className="text-base">{t("One time")}</p>
          ) : (
            <>
              <div className="flex flex-wrap gap-2">
                <Chip on={endKind === "after_count"} onClick={() => setEndKind("after_count")}>{t("A set number of times")}</Chip>
                <Chip on={endKind === "on_date"} onClick={() => setEndKind("on_date")}>{t("Until a date")}</Chip>
                <Chip on={endKind === "none"} onClick={() => setEndKind("none")}>{t("Until I stop it")}</Chip>
              </div>
              {endKind === "after_count" ? (
                <Field label={t("Times")}>
                  <Input inputMode="numeric" value={times} onChange={(event) => setTimes(event.target.value)} />
                </Field>
              ) : null}
              {endKind === "on_date" ? (
                <Field label={t("Until a date")}>
                  <Input type="date" value={endDate} onChange={(event) => setEndDate(event.target.value)} />
                </Field>
              ) : null}
              {endKind === "none" ? <p className="fin-muted text-sm leading-5">{t("No end date. The next 12 months are shown, not a lifetime total.")}</p> : null}
            </>
          )}
        </div>
      ) : null}
      {step === 4 ? (
        <div className="space-y-4">
          <Field label={t("Name this")}>
            <Input value={name} onChange={(event) => setName(event.target.value)} />
          </Field>
          <Field label={t("Start date")}>
            <Input type="date" value={start} onChange={(event) => setStart(event.target.value)} />
          </Field>
          <Field label={t("Which account")}>
            <div className="flex gap-2 overflow-x-auto">
              <Chip on={accountId === ""} onClick={() => setAccountId("")}>{t("No account")}</Chip>
              {accounts.map((account) => (
                <Chip key={account.id} on={accountId === account.id} onClick={() => setAccountId(account.id)}>{account.name}</Chip>
              ))}
            </div>
          </Field>
          <Field label={t("Category")}>
            <Input value={category} onChange={(event) => setCategory(event.target.value)} />
          </Field>
          <Field label={t("Remind me")}>
            <div className="flex gap-2 overflow-x-auto">
              {[0, 1, 2, 7].map((days) => (
                <Chip key={days} on={leadDays === days} onClick={() => setLeadDays(days)}>{days === 0 ? t("On the day") : days === 2 ? t("2 days before") : String(days)}</Chip>
              ))}
            </div>
          </Field>
        </div>
      ) : null}
    </Sheet>
  );
}

function Chip({ on, onClick, children }: { on: boolean; onClick: () => void; children: string }) {
  return (
    <button type="button" aria-pressed={on} className={cn("fin-chip", on && "min-h-12")} onClick={onClick}>
      {children}
    </button>
  );
}

function presetOf(rule: FinanceRule | null): Preset {
  if (!rule) return "course";
  if (rule.purpose === "investment_payout") return "payout";
  if (rule.purpose === "subscription") return "subscription";
  if (rule.purpose === "one_off" || !rule.cadence) return "once";
  if (rule.endKind === "after_count") return "course";
  if (rule.direction === "income") return "income";
  return "rent";
}

function presetTitle(id: Preset): string {
  if (id === "course") return "Course or fixed run";
  if (id === "rent") return "Open-ended";
  if (id === "subscription") return "Subscription";
  if (id === "income") return "Money coming in";
  if (id === "payout") return "Investment payout";
  return "One time";
}

function presetBody(id: Preset): string {
  if (id === "course") return "It ends after a number of payments.";
  if (id === "rent") return "It keeps going until you pause or stop it.";
  if (id === "subscription") return "Monthly or yearly, until you stop it.";
  if (id === "income") return "A payday, or income on a schedule.";
  if (id === "payout") return "The payout only. The principal stays what you entered.";
  return "A single expense, or income not received yet.";
}
