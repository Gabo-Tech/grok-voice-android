import type { RateBook } from "@/lib/financeMath";
import { headlineOf, nativeParts, type Headline, type MoneyPart } from "@/lib/financeMath";
import { formatMoney } from "@/lib/money";
import { fill, useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

/** Quiet line under a combined amount. A stale book is labeled. This is not a rate list. */
function rateCaption(head: Headline, t: (english: string) => string): string {
  if (head.source === "identity") return head.currency;
  const bits: string[] = [];
  if (head.source === "manual") bits.push(t("Using your rate"));
  bits.push(fill("Together in {code}", { code: head.currency }));
  if (head.asOf) bits.push(fill("As of {date}", { date: head.asOf }));
  if (head.stale) bits.push(t("Rates are stale"));
  return bits.join(" · ");
}

export function CombinedHeadline({
  parts,
  display,
  book,
  nowMs,
  busy = false,
  size = "text-4xl",
  tone,
}: {
  parts: MoneyPart[];
  display: string;
  book: RateBook | null;
  nowMs: number;
  busy?: boolean;
  size?: string;
  tone?: "in" | "out" | "auto";
}) {
  const t = useT();
  const head = headlineOf(parts, display, book, nowMs);
  if (head.state === "empty") return null;
  if (head.state === "needs-rate") {
    const codes = head.missing.join(", ");
    return (
      <div className="mt-4 max-w-full">
        <p className="text-base leading-6 break-words">{busy ? fill("Checking the rate for {codes}.", { codes }) : fill("Combined total needs a rate for {codes}.", { codes })}</p>
        {busy ? null : <p className="fin-muted mt-2 text-sm leading-5">{t("Set a manual rate in Settings.")}</p>}
        <NativeAside parts={parts} display={display} />
      </div>
    );
  }
  const signed = tone === "auto" ? (head.amountMinor < 0 ? "out" : head.amountMinor > 0 ? "in" : undefined) : tone;
  return (
    <div className="mt-4 max-w-full">
      <p className={cn("fin-num leading-none font-medium break-words", size, signed === "in" && "fin-in", signed === "out" && "fin-out")}>
        {formatMoney(head.amountMinor, head.currency)}
      </p>
      <p className="fin-muted mt-2 text-sm leading-5 break-words">{rateCaption(head, t)}</p>
      <NativeAside parts={parts} display={display} />
    </div>
  );
}

export function StaticHeadline({ head, size = "text-4xl" }: { head: Headline; fetchedMs?: number; size?: string }) {
  const t = useT();
  if (head.state === "empty") return null;
  if (head.state === "needs-rate") {
    return (
      <div className="mt-4 max-w-full">
        <p className="text-base leading-6 break-words">{fill("Combined total needs a rate for {codes}.", { codes: head.missing.join(", ") })}</p>
        <p className="fin-muted mt-2 text-sm leading-5">{t("Set a manual rate in Settings.")}</p>
      </div>
    );
  }
  return (
    <div className="mt-4 max-w-full">
      <p className={cn("fin-num leading-none font-medium break-words", size, head.amountMinor < 0 && "fin-out")}>{formatMoney(head.amountMinor, head.currency)}</p>
      <p className="fin-muted mt-2 text-sm leading-5 break-words">{rateCaption(head, t)}</p>
    </div>
  );
}

function NativeAside({ parts, display }: { parts: MoneyPart[]; display: string }) {
  const t = useT();
  const rows = nativeParts(parts);
  if (rows.length === 0) return null;
  if (rows.length === 1 && rows[0].currency === display.trim().toUpperCase()) return null;
  return (
    <p className="fin-muted mt-2 text-sm leading-5 break-words">
      {t("In native amounts")} · {rows.map((row) => formatMoney(row.amountMinor, row.currency)).join(" · ")}
    </p>
  );
}
