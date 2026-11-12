import { useState } from "react";
import type { RateBook } from "@/lib/financeMath";
import { headlineOf, nativeParts, type Headline, type MoneyPart } from "@/lib/financeMath";
import { formatMoney } from "@/lib/money";
import { fill, useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

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
      <p className="fin-muted mt-2 text-sm leading-5 break-words">
        {head.source === "identity" ? head.currency : fill("Together in {code}", { code: head.currency })}
        {head.asOf ? ` · ${fill("As of {date}", { date: head.asOf })}` : ""}
        {head.stale ? ` · ${t("Rates are stale")}` : ""}
      </p>
      <NativeAside parts={parts} display={display} />
    </div>
  );
}

export function StaticHeadline({ head, size = "text-4xl" }: { head: Headline; size?: string }) {
  const t = useT();
  if (head.state === "empty") return null;
  if (head.state === "needs-rate") {
    return <p className="mt-4 text-base leading-6 break-words">{fill("Combined total needs a rate for {codes}.", { codes: head.missing.join(", ") })}</p>;
  }
  return (
    <div className="mt-4 max-w-full">
      <p className={cn("fin-num leading-none font-medium break-words", size, head.amountMinor < 0 && "fin-out")}>{formatMoney(head.amountMinor, head.currency)}</p>
      <p className="fin-muted mt-2 text-sm leading-5 break-words">
        {fill("Together in {code}", { code: head.currency })}
        {head.asOf ? ` · ${fill("As of {date}", { date: head.asOf })}` : ""}
        {head.stale ? ` · ${t("Rates are stale")}` : ""}
      </p>
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

export function RatePanel({
  codes,
  display,
  busy,
  onRefresh,
  onSave,
  bare = false,
}: {
  codes: string[];
  display: string;
  busy: boolean;
  onRefresh: () => void;
  onSave: (base: string, quote: string, rate: string) => void;
  bare?: boolean;
}) {
  const t = useT();
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const quote = display.trim().toUpperCase();
  if (codes.length === 0) return null;
  const label = codes.join(", ");
  const body = (
    <>
      {bare ? null : <h2 className="fin-kicker">{t("Rates")}</h2>}
      {busy ? (
        <p className="mt-3 text-base leading-6 break-words">{fill("Checking the rate for {codes}.", { codes: label })}</p>
      ) : (
        <>
          <p className="mt-3 text-base leading-6 break-words">{fill("Combined total needs a rate for {codes}.", { codes: label })}</p>
          <button type="button" className="stamp mt-3 min-h-12 px-3" onClick={onRefresh}>
            {t("Refresh rates")}
          </button>
          <div className="mt-4 space-y-4">
            {codes.map((code) => (
              <div key={code} className="min-w-0">
                <p className="text-sm font-extrabold tracking-wide uppercase">{fill("Need a rate for {code}", { code })}</p>
                <div className="mt-2 flex min-w-0 flex-wrap items-center gap-2">
                  <span className="text-sm whitespace-nowrap">{fill("1 {code} =", { code })}</span>
                  <input
                    aria-label={fill("Need a rate for {code}", { code })}
                    inputMode="decimal"
                    value={drafts[code] || ""}
                    placeholder="0.92"
                    className="h-12 min-w-0 flex-1 border-[2.5px] border-border bg-transparent px-3 text-base"
                    onChange={(event) => setDrafts((current) => ({ ...current, [code]: event.target.value }))}
                  />
                  <span className="text-sm font-extrabold">{quote}</span>
                  <button
                    type="button"
                    className="stamp min-h-12 px-3"
                    disabled={!drafts[code]?.trim()}
                    onClick={() => onSave(code, quote, drafts[code].trim())}
                  >
                    {t("Save rate")}
                  </button>
                </div>
              </div>
            ))}
          </div>
          <p className="fin-muted mt-3 text-sm leading-5">{t("1 of the first currency equals this many of the second.")}</p>
        </>
      )}
    </>
  );
  if (bare) return <div className="max-w-full">{body}</div>;
  return <section className="fin-panel">{body}</section>;
}
