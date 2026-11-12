import { useEffect, useRef } from "react";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { pushOverlay } from "@/lib/history";
import { useT } from "@/lib/i18n";

export function Sheet({
  open,
  title,
  onClose,
  children,
  footer,
  className,
  theme,
}: {
  open: boolean;
  title: string;
  onClose: () => void;
  children: React.ReactNode;
  footer?: React.ReactNode;
  className?: string;
  theme?: string;
}) {
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const t = useT();

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCloseRef.current();
    };
    window.addEventListener("keydown", onKey);
    const release = pushOverlay(() => onCloseRef.current());
    return () => {
      window.removeEventListener("keydown", onKey);
      release();
    };
  }, [open]);

  if (!open) return null;

  return (
    <div className={`sheet-root ${className ?? ""}`} data-theme={theme} role="dialog" aria-modal="true" aria-label={title}>
      <button type="button" className="sheet-scrim" aria-label={t("Close")} onClick={onClose} />
      <div className="sheet-panel">
        <header className="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
          <h2 className="masthead text-lg leading-none">{title}</h2>
          <Button size="icon" variant="ghost" onClick={onClose} aria-label={t("Close")}>
            <X />
          </Button>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">{children}</div>
        {footer && <footer className="thumb-actions shrink-0 border-t border-border px-4 py-3">{footer}</footer>}
      </div>
    </div>
  );
}
