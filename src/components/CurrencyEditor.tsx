import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { FinanceState } from "@/hooks/useFinance";
import { bookFromStatus, bookIsStale, formatFetched, formatRate, quoteRate } from "@/lib/financeMath";
import { fill, useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

const COMMON = ["CHF", "EUR", "USD", "GBP", "XMR"];

export function CurrencyEditor({ finance }: { finance: FinanceState }) {
  const t = useT();
  const current = (finance.snapshot.defaultCurrency || "").toUpperCase();
  const [custom, setCustom] = useState(current);
  const [base, setBase] = useState(current === "EUR" ? "USD" : "EUR");
  const [rate, setRate] = useState("");
  const [overrideLive, setOverrideLive] = useState(false);
  const [note, setNote] = useState("");
  const book = bookFromStatus(finance.snapshot.fx);
  const nowMs = Date.now();
  const stale = book ? bookIsStale(book, nowMs) : false;
  const codes = usedCodes(finance);
  const manual = finance.snapshot.fx?.manual || [];
  return (
    <div className="mag-panel w-full">
      <h2 className="chapter">{t("Default currency")}</h2>
      <p className="mt-2 text-sm leading-5 text-muted-foreground">
        {t("Used when you do not name a currency. A saved account keeps its own.")}
      </p>
      <div className="chip-scroller mt-3">
        {COMMON.map((code) => (
          <button
            key={code}
            type="button"
            aria-pressed={current === code}
            className={cn("stamp min-h-12 min-w-16", current === code && "stamp-on")}
            onClick={() => finance.setDefaultCurrency(code)}
          >
            {code}
          </button>
        ))}
      </div>
      <div className="mt-3 flex gap-2">
        <Input
          value={custom}
          maxLength={3}
          autoCapitalize="characters"
          aria-label={t("Default currency")}
          onChange={(event) => setCustom(event.target.value.toUpperCase())}
        />
        <Button type="button" disabled={!custom.trim()} onClick={() => finance.setDefaultCurrency(custom.trim())}>
          {t("Save")}
        </Button>
      </div>

      <h2 className="chapter mt-6">{t("Today's rate")}</h2>
      <p className="mt-2 text-sm leading-5 text-muted-foreground">{t("Live rate for the day. Native amounts stay on each item.")}</p>
      <p className="mt-2 text-sm leading-5 text-muted-foreground">
        {book?.asOf
          ? `${fill("As of {date}", { date: book.asOf })}${book.fetchedMs ? ` · ${fill("Fetched {time}", { time: formatFetched(book.fetchedMs) })}` : ""}${stale ? ` · ${t("Rates are stale")}` : ""}`
          : t("No rate is saved yet.")}
      </p>
      {current ? (
        <ul className="mt-3 space-y-1">
          {codes
            .filter((code) => code !== current)
            .map((code) => {
              const row = quoteRate(code, current, book, nowMs);
              return (
                <li key={code} className="text-sm break-words">
                  {row && row.source !== "identity"
                    ? fill("1 {code} = {rate} {quote}", { code, rate: formatRate(row.rate), quote: current })
                    : fill("Combined total needs a rate for {codes}.", { codes: code })}
                  {row?.source === "manual" ? ` · ${t("Using your rate")}` : ""}
                </li>
              );
            })}
        </ul>
      ) : null}
      <Button
        type="button"
        className="mt-3"
        disabled={finance.busy}
        onClick={() => {
          setNote("");
          finance.refreshRates(true);
          setNote(t("Rates stay on this phone."));
        }}
      >
        {t("Refresh rates")}
      </Button>
      {note ? <p className="mt-3 text-sm">{note}</p> : null}

      <div className="mt-6 border-t border-border pt-4">
        <h2 className="text-sm font-extrabold tracking-wide text-muted-foreground uppercase">{t("Manual rates")}</h2>
        <p className="mt-2 text-sm leading-5 text-muted-foreground">{t("A fallback when today's rate is missing.")}</p>
        <div className="mt-3 flex min-w-0 flex-wrap items-center gap-2">
          <span className="text-sm whitespace-nowrap">{fill("1 {code} =", { code: base || "USD" })}</span>
          <Input
            aria-label={t("Currency")}
            value={base}
            maxLength={3}
            autoCapitalize="characters"
            className="w-20"
            onChange={(event) => setBase(event.target.value.toUpperCase())}
          />
          <Input
            aria-label={t("Manual rate")}
            inputMode="decimal"
            value={rate}
            placeholder="0.92"
            className="min-w-0 flex-1"
            onChange={(event) => setRate(event.target.value)}
          />
          <span className="text-sm font-extrabold">{current || t("Default currency")}</span>
        </div>
        <label className="mt-3 flex items-start gap-2 text-sm leading-5">
          <input
            type="checkbox"
            className="mt-1 size-4"
            checked={overrideLive}
            onChange={(event) => setOverrideLive(event.target.checked)}
          />
          <span>{t("Use this instead of the live rate")}</span>
        </label>
        <p className="fin-muted mt-2 text-sm">{t("1 of the first currency equals this many of the second.")}</p>
        <div className="mt-3 flex gap-2">
          <Button
            type="button"
            disabled={!base.trim() || !current || base.trim().toUpperCase() === current || !rate.trim()}
            onClick={() => finance.setManualRate(base.trim(), current, rate.trim(), overrideLive)}
          >
            {t("Save")}
          </Button>
          <Button type="button" variant="ghost" disabled={!base.trim() || !current} onClick={() => finance.setManualRate(base.trim(), current, "")}>
            {t("Clear rate")}
          </Button>
        </div>
        {manual.map((row) => (
          <div key={`${row.base}-${row.quote}`} className="mt-3 flex flex-wrap items-center justify-between gap-2 text-sm">
            <p>
              1 {row.base} = {row.rate} {row.quote}
              {row.overrideLive ? ` · ${t("Using your rate")}` : ""}
            </p>
            <div className="flex gap-2">
              <Button type="button" variant="ghost" onClick={() => finance.setManualRate(row.base, row.quote, String(row.rate), !row.overrideLive)}>
                {t("Use this instead of the live rate")}
              </Button>
              <Button type="button" variant="ghost" onClick={() => finance.setManualRate(row.base, row.quote, "")}>
                {t("Clear rate")}
              </Button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function usedCodes(finance: FinanceState): string[] {
  const codes = new Set<string>();
  const add = (code?: string) => {
    const next = (code || "").trim().toUpperCase();
    if (next) codes.add(next);
  };
  add(finance.snapshot.defaultCurrency);
  for (const account of finance.snapshot.accounts) add(account.currency);
  for (const tx of finance.snapshot.transactions) add(tx.currency);
  for (const rule of finance.snapshot.rules) {
    add(rule.currency);
    add(rule.payoutCurrency);
  }
  return [...codes].sort();
}
