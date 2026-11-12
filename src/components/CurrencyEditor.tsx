import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { FinanceState } from "@/hooks/useFinance";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

const COMMON = ["CHF", "EUR", "USD", "GBP", "XMR"];

export function CurrencyEditor({ finance }: { finance: FinanceState }) {
  const t = useT();
  const current = (finance.snapshot.defaultCurrency || "").toUpperCase();
  const [custom, setCustom] = useState(current);
  const [base, setBase] = useState("USD");
  const [quote, setQuote] = useState(current || "EUR");
  const [rate, setRate] = useState("");
  const [note, setNote] = useState("");
  return (
    <div className="mag-panel">
      <h2 className="chapter">{t("Default currency")}</h2>
      <p className="mt-2 text-sm leading-5 text-muted-foreground">
        {t("Used when you do not name a currency. A saved account keeps its own.")}
      </p>
      <div className="mt-3 flex gap-2 overflow-x-auto">
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
      <h2 className="chapter mt-6">{t("Rates")}</h2>
      <p className="mt-2 text-sm leading-5 text-muted-foreground">{t("Native amounts stay. A combined total needs a saved rate.")}</p>
      <p className="mt-2 text-sm leading-5 text-muted-foreground">
        {finance.snapshot.fx?.asOf
          ? `${t("As of")} ${finance.snapshot.fx.asOf}${finance.snapshot.fx.stale ? ` · ${t("Rates are stale")}` : ""}`
          : t("No rate is saved yet.")}
      </p>
      <Button
        type="button"
        className="mt-3"
        disabled={finance.busy}
        onClick={() => {
          setNote("");
          finance.refreshRates();
          setNote(t("Rates stay on this phone."));
        }}
      >
        {t("Refresh rates")}
      </Button>
      <div className="mt-4 grid grid-cols-3 gap-2">
        <Input aria-label={t("Currency")} value={base} maxLength={8} autoCapitalize="characters" onChange={(event) => setBase(event.target.value.toUpperCase())} />
        <Input aria-label={t("Currency")} value={quote} maxLength={8} autoCapitalize="characters" onChange={(event) => setQuote(event.target.value.toUpperCase())} />
        <Input aria-label={t("Manual rate")} inputMode="decimal" value={rate} placeholder="0.92" onChange={(event) => setRate(event.target.value)} />
      </div>
      <p className="fin-muted mt-2 text-sm">{t("1 of the first currency equals this many of the second.")}</p>
      <div className="mt-3 flex gap-2">
        <Button
          type="button"
          disabled={!base.trim() || !quote.trim() || !rate.trim()}
          onClick={() => finance.setManualRate(base.trim(), quote.trim(), rate.trim())}
        >
          {t("Save")}
        </Button>
        <Button type="button" variant="ghost" disabled={!base.trim() || !quote.trim()} onClick={() => finance.setManualRate(base.trim(), quote.trim(), "")}>
          {t("Clear rate")}
        </Button>
      </div>
      {note ? <p className="mt-3 text-sm">{note}</p> : null}
      {(finance.snapshot.fx?.manual || []).map((row) => (
        <p key={`${row.base}-${row.quote}`} className="mt-2 text-sm">
          1 {row.base} = {row.rate} {row.quote}
        </p>
      ))}
    </div>
  );
}
