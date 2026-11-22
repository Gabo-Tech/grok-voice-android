import { useEffect, useState } from "react";
import { Sheet } from "@/components/Sheet";
import { Field } from "@/components/Field";
import { Input } from "@/components/ui/input";
import type { FinanceAccount, PriceAttempt } from "@/lib/api";
import { useT } from "@/lib/i18n";

export function PriceSources({
  open,
  account,
  attempts,
  onClose,
  onRefresh,
  onSave,
}: {
  open: boolean;
  account: FinanceAccount | null;
  attempts: PriceAttempt[];
  onClose: () => void;
  onRefresh: (account: FinanceAccount) => void;
  onSave: (account: FinanceAccount, coingeckoId: string, paprikaId: string) => void;
}) {
  const t = useT();
  const [gecko, setGecko] = useState("");
  const [paprika, setPaprika] = useState("");
  const accountId = account?.id || "";
  const savedGecko = account?.providerCoingeckoId || "";
  const savedPaprika = account?.providerPaprikaId || "";
  useEffect(() => {
    if (!open) return;
    setGecko(savedGecko);
    setPaprika(savedPaprika);
  }, [open, accountId, savedGecko, savedPaprika]);

  if (!open || !account) return null;
  const symbol = (account.marketSymbol || account.ticker || "").trim();
  const rows = attempts.filter((row) => !symbol || row.symbol.toUpperCase() === symbol.toUpperCase());
  const quantityMissing = !(account.units || "").trim();

  return (
    <Sheet
      open={open}
      title={t("Price sources")}
      onClose={onClose}
      className="finance"
      footer={
        <button type="button" className="fin-primary w-full" onClick={() => onRefresh(account)}>
          {t("Refresh price now")}
        </button>
      }
    >
      <p className="text-base leading-6">{account.name}</p>
      <p className="mt-2 text-sm leading-5 text-muted-foreground">
        {t("Symbol")} {symbol || t("none")}
      </p>
      <p className="mt-1 text-sm leading-5 text-muted-foreground">
        {t("CoinGecko id")} {account.providerCoingeckoId || t("none")}
      </p>
      <p className="mt-1 text-sm leading-5 text-muted-foreground">
        {t("CoinPaprika id")} {account.providerPaprikaId || t("none")}
      </p>
      {quantityMissing ? <p className="mt-3 text-sm leading-5">{t("No quantity is saved, so a unit price cannot become a value.")}</p> : null}
      <ul className="mt-4">
        {rows.length === 0 ? <li className="border-t border-border py-3 text-sm leading-5">{t("No source has been tried yet.")}</li> : null}
        {rows.map((row, index) => (
          <li key={`${row.source}-${row.id}-${index}`} className="border-t border-border py-3">
            <p className="text-sm leading-5">{row.source}</p>
            <p className="text-[13px] leading-5 text-muted-foreground">
              {row.id || t("none")} · {t(row.detail || row.status)}
            </p>
          </li>
        ))}
      </ul>
      <div className="mt-6 space-y-3">
        <p className="text-sm leading-5">{t("Set provider id")}</p>
        <Field label={t("CoinGecko id")}>
          <Input value={gecko} onChange={(event) => setGecko(event.target.value)} autoCapitalize="off" autoCorrect="off" spellCheck={false} />
        </Field>
        <Field label={t("CoinPaprika id")}>
          <Input value={paprika} onChange={(event) => setPaprika(event.target.value)} autoCapitalize="off" autoCorrect="off" spellCheck={false} />
        </Field>
        <button
          type="button"
          className="fin-textbtn underline"
          onClick={() => onSave(account, gecko.trim(), paprika.trim())}
        >
          {t("Save provider id")}
        </button>
      </div>
    </Sheet>
  );
}
