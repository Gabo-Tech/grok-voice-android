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

  const panelRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const panel = panelRef.current;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const focusable = () =>
      panel
        ? [...panel.querySelectorAll<HTMLElement>("button, [href], input, select, textarea, [tabindex]:not([tabindex='-1'])")].filter(
            (el) => !el.hasAttribute("disabled") && el.tabIndex !== -1,
          )
        : [];
    const titleEl = panel?.querySelector("h2");
    if (titleEl instanceof HTMLElement) titleEl.focus();
    else focusable()[0]?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onCloseRef.current();
        return;
      }
      if (e.key !== "Tab" || !panel) return;
      const items = focusable();
      if (!items.length) {
        e.preventDefault();
        return;
      }
      const firstEl = items[0];
      const lastEl = items[items.length - 1];
      const active = document.activeElement;
      if (e.shiftKey && (active === firstEl || !panel.contains(active))) {
        e.preventDefault();
        lastEl.focus();
      } else if (!e.shiftKey && (active === lastEl || !panel.contains(active))) {
        e.preventDefault();
        firstEl.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    const release = pushOverlay(() => onCloseRef.current());
    return () => {
      window.removeEventListener("keydown", onKey);
      release();
      previous?.focus();
    };
  }, [open]);

  if (!open) return null;

  return (
    <div className={`sheet-root ${className ?? ""}`} data-theme={theme} role="dialog" aria-modal="true" aria-label={title}>
      <button type="button" className="sheet-scrim" aria-label={t("Close")} onClick={onClose} />
      <div ref={panelRef} className="sheet-panel">
        <div className="sheet-handle" aria-hidden />
        <header className="sheet-head flex shrink-0 items-center justify-between gap-3 px-4 py-3">
          <h2 tabIndex={-1} className="sheet-title min-w-0 flex-1 text-lg outline-none">{title}</h2>
          <Button size="icon" variant="ghost" onClick={onClose} aria-label={t("Close")}>
            <X />
          </Button>
        </header>
        <div className="sheet-scroll min-h-0 flex-1 overflow-y-auto px-4 py-4">{children}</div>
        {footer && <footer className="sheet-foot thumb-actions shrink-0 border-t border-border px-4 py-3">{footer}</footer>}
      </div>
    </div>
  );
}

/** Save stays the one solid action. Secondary controls sit above it. Delete sits on its own row. */
export function SheetFooter({
  primaryLabel,
  onPrimary,
  primaryDisabled,
  secondary,
  destructive,
}: {
  primaryLabel: string;
  onPrimary: () => void;
  primaryDisabled?: boolean;
  secondary?: React.ReactNode;
  destructive?: React.ReactNode;
}) {
  return (
    <div className="sheet-actions">
      {secondary ? <div className="sheet-actions-row">{secondary}</div> : null}
      {destructive ? <div className="sheet-actions-row sheet-actions-apart">{destructive}</div> : null}
      <Button type="button" className="w-full" disabled={primaryDisabled} onClick={onPrimary}>
        {primaryLabel}
      </Button>
    </div>
  );
}
