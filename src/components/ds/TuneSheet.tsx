import { useState, type ReactNode } from "react";
import { SlidersHorizontal } from "lucide-react";
import { Sheet } from "@/components/Sheet";

/** One tune icon. Secondary options live in the sheet, with the page left on its defaults. */
export function TuneSheet({ label, children }: { label: string; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" className="icon-hit" aria-label={label} aria-expanded={open} onClick={() => setOpen(true)}>
        <SlidersHorizontal className="size-5" strokeWidth={1.5} aria-hidden />
      </button>
      <Sheet open={open} title={label} onClose={() => setOpen(false)}>
        <div className="tune-sheet">{children}</div>
      </Sheet>
    </>
  );
}
