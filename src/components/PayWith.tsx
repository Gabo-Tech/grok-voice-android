import type { ReactNode } from "react";
import type { FinanceAccount } from "@/lib/api";
import { isCrypto } from "@/lib/money";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

/** Sentinel used when Cash is offered and no cash wallet exists yet. */
export const CASH_WALLET = "__cash__";

export function PayWith({
  accounts,
  value,
  onChange,
  mode,
}: {
  accounts: FinanceAccount[];
  value: string;
  onChange: (id: string) => void;
  mode: "pay" | "deposit" | "fund";
}) {
  const t = useT();
  const crypto = accounts.filter((account) => isCrypto(account.currency) && account.kind !== "credit_card" && account.kind !== "loan" && account.kind !== "other_debt");
  const cash = accounts.filter((account) => account.kind === "cash" && !isCrypto(account.currency));
  const banks = accounts.filter((account) => (account.kind === "bank" || account.kind === "other" || account.kind === "savings") && !isCrypto(account.currency));
  const cards = accounts.filter((account) => account.kind === "credit_card");
  const showCards = mode !== "fund";
  const label = mode === "deposit" ? t("Deposit to") : mode === "fund" ? t("From") : t("Pay with");
  return (
    <div className="space-y-3">
      <p className="text-sm leading-5 text-muted-foreground">{label}</p>
      {crypto.length > 0 ? (
        <ChipGroup title={t("Crypto")}>
          {crypto.map((account) => (
            <Chip key={account.id} on={value === account.id} onClick={() => onChange(account.id)}>{account.name}</Chip>
          ))}
        </ChipGroup>
      ) : null}
      <ChipGroup title={t("Cash")}>
        {cash.map((account) => (
          <Chip key={account.id} on={value === account.id} onClick={() => onChange(account.id)}>{account.name}</Chip>
        ))}
        {cash.length === 0 ? (
          <Chip on={value === CASH_WALLET} onClick={() => onChange(CASH_WALLET)}>{t("Cash")}</Chip>
        ) : null}
      </ChipGroup>
      <ChipGroup title={t("Accounts")}>
        {banks.length === 0 ? <p className="fin-muted text-sm">{t("No accounts yet")}</p> : null}
        {banks.map((account) => (
          <Chip key={account.id} on={value === account.id} onClick={() => onChange(account.id)}>{account.name}</Chip>
        ))}
      </ChipGroup>
      {showCards ? (
        <ChipGroup title={mode === "deposit" ? t("Credit to a card") : t("Cards")}>
          {cards.length === 0 ? <p className="fin-muted text-sm">{t("No credit card is saved.")}</p> : null}
          {cards.map((account) => (
            <Chip key={account.id} on={value === account.id} onClick={() => onChange(account.id)}>{account.name}</Chip>
          ))}
        </ChipGroup>
      ) : null}
    </div>
  );
}

function ChipGroup({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="min-w-0">
      <p className="fin-kicker mb-2">{title}</p>
      <div className="chip-scroller">{children}</div>
    </div>
  );
}

function Chip({ on, onClick, children }: { on: boolean; onClick: () => void; children: string }) {
  return (
    <button type="button" aria-pressed={on} className={cn("fin-chip min-h-12 shrink-0 whitespace-nowrap", on && "min-h-12")} onClick={onClick}>
      {children}
    </button>
  );
}
