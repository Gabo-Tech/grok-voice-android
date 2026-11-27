import { useState, type ReactNode } from "react";
import { Info } from "lucide-react";
import { Sheet } from "@/components/Sheet";

/** A 48px info control. The explanation stays out of the page until it is opened. */
export function InfoTip({ label, text, children }: { label: string; text?: string; children?: ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" className="icon-hit" aria-label={label} aria-expanded={open} onClick={() => setOpen(true)}>
        <Info className="size-5" strokeWidth={1.5} aria-hidden />
      </button>
      <Sheet open={open} title={label} onClose={() => setOpen(false)}>
        <div className="space-y-3 text-base leading-6 text-foreground">
          {text ? <p>{text}</p> : null}
          {children}
        </div>
      </Sheet>
    </>
  );
}
