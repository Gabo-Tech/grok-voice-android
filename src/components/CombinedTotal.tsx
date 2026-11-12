import type { RateBook } from "@/lib/financeMath";
import { bookIsStale, formatFetched, formatRate, headlineOf, nativeParts, quoteRate, type Headline, type MoneyPart } from "@/lib/financeMath";
import { formatMoney } from "@/lib/money";
import { fill, useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

function rateCaption(head: Headline, fetchedMs: number, t: (english: string) => string): string {
  if (head.source === "identity") return head.currency;
  const bits: string[] = [];
  if (head.source === "manual") bits.push(t("Using your rate"));
  else if (!head.stale) bits.push(t("Today's rate"));
  bits.push(fill("Together in {code}", { code: head.currency }));
  if (head.asOf) bits.push(fill("As of {date}", { date: head.asOf }));
  if (fetchedMs > 0 && head.source === "cached") bits.push(fill("Fetched {time}", { time: formatFetched(fetchedMs) }));
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
      <p className="fin-muted mt-2 text-sm leading-5 break-words">{rateCaption(head, book?.fetchedMs || 0, t)}</p>
      <NativeAside parts={parts} display={display} />
    </div>
  );
}

export function StaticHeadline({ head, fetchedMs = 0, size = "text-4xl" }: { head: Headline; fetchedMs?: number; size?: string }) {
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
      <p className="fin-muted mt-2 text-sm leading-5 break-words">{rateCaption(head, fetchedMs, t)}</p>
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

export function LiveRates({
  codes,
  display,
  book,
  nowMs,
  busy,
  missing,
  onOpenSettings,
}: {
  codes: string[];
  display: string;
  book: RateBook | null;
  nowMs: number;
  busy: boolean;
  missing: string[];
  onOpenSettings?: () => void;
}) {
  const t = useT();
  const quote = display.trim().toUpperCase();
  const pairs = [...new Set(codes.map((code) => code.trim().toUpperCase()).filter((code) => code && code !== quote))].sort();
  if (pairs.length === 0 && missing.length === 0) return null;
  const stale = book ? bookIsStale(book, nowMs) : false;
  return (
    <section className="fin-panel w-full">
      <h2 className="fin-kicker">{t("Today's rate")}</h2>
      <p className="fin-muted mt-2 text-sm leading-5">{t("Live rate for the day. Native amounts stay on each item.")}</p>
      {book && (book.asOf || book.fetchedMs > 0) ? (
        <p className="mt-2 text-sm leading-5 break-words">
          {book.asOf ? fill("As of {date}", { date: book.asOf }) : ""}
          {book.fetchedMs > 0 ? `${book.asOf ? " · " : ""}${fill("Fetched {time}", { time: formatFetched(book.fetchedMs) })}` : ""}
          {stale ? ` · ${t("Rates are stale")}` : ""}
        </p>
      ) : (
        <p className="mt-2 text-sm leading-5">{busy ? fill("Checking the rate for {codes}.", { codes: (missing.length ? missing : pairs).join(", ") }) : t("No rate is saved yet.")}</p>
      )}
      {pairs.length > 0 ? (
        <ul className="mt-3 space-y-1">
          {pairs.map((code) => {
            const row = quoteRate(code, quote, book, nowMs);
            return (
              <li key={code} className="fin-num text-base break-words">
                {row && row.source !== "identity"
                  ? fill("1 {code} = {rate} {quote}", { code, rate: formatRate(row.rate), quote })
                  : fill("Combined total needs a rate for {codes}.", { codes: code })}
                {row?.source === "manual" ? ` · ${t("Using your rate")}` : ""}
              </li>
            );
          })}
        </ul>
      ) : null}
      {missing.length > 0 ? (
        <div className="mt-3">
          <p className="text-base leading-6 break-words">
            {busy ? fill("Checking the rate for {codes}.", { codes: missing.join(", ") }) : fill("Combined total needs a rate for {codes}.", { codes: missing.join(", ") })}
          </p>
          {busy ? null : <p className="fin-muted mt-2 text-sm leading-5">{t("Set a manual rate in Settings.")}</p>}
          {onOpenSettings && !busy ? (
            <button type="button" className="stamp mt-3 min-h-12 px-3" onClick={onOpenSettings}>
              {t("Settings")}
            </button>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
