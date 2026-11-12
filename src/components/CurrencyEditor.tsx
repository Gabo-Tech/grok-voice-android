import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { FinanceState } from "@/hooks/useFinance";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

const COMMON = ["CHF", "EUR", "USD", "GBP"];

export function CurrencyEditor({ finance }: { finance: FinanceState }) {
  const t = useT();
  const current = (finance.snapshot.defaultCurrency || "").toUpperCase();
  const [custom, setCustom] = useState(current);
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
    </div>
  );
}
