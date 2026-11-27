import { useEffect, useRef } from "react";
import { ChevronLeft } from "lucide-react";
import { AppMark } from "@/components/AppMark";
import { Button } from "@/components/ui/button";
import { setBarAction } from "@/lib/history";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

export function PageChrome({
  title,
  subtitle,
  info,
  bare = false,
  onAdd,
  addLabel,
  mark = false,
  active = true,
  children,
  className,
}: {
  title: string;
  subtitle?: string;
  info?: React.ReactNode;
  /** Hide the title when a parent screen already named the page. */
  bare?: boolean;
  onAdd?: () => void;
  addLabel?: string;
  mark?: boolean;
  /** Only the visible screen owns the bottom add action. */
  active?: boolean;
  children: React.ReactNode;
  className?: string;
}) {
  const addRef = useRef(onAdd);
  addRef.current = onAdd;
  const hasAdd = Boolean(onAdd);
  useEffect(() => {
    if (!active || !hasAdd) return;
    return setBarAction({ label: addLabel ?? "Add", run: () => addRef.current?.() });
  }, [active, addLabel, hasAdd]);

  return (
    <div className={cn("flex min-h-0 flex-1 flex-col", className)}>
      {bare ? null : (
      <header className="shrink-0 px-1 pb-4 pt-1">
        <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
          {mark ? <AppMark className="w-14 shrink-0 landscape:w-12" alt="Grok Voice" /> : null}
          <div className="page-title min-w-0 flex-1">
          <div className="title-with-info">
            <h1 data-fit="title" className="masthead text-foreground">{title}</h1>
            {info}
          </div>
          <div className="masthead-rule" />
          {subtitle && <p className="page-sub mt-2 text-sm leading-5 text-muted-foreground">{subtitle}</p>}
          </div>
        </div>
      </header>
      )}
      <div className="page-scroll min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto">{children}</div>
    </div>
  );
}

export function BackButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="mb-2 flex min-h-12 w-full shrink-0 items-center gap-1 px-1 text-left text-base text-foreground"
    >
      <ChevronLeft className="size-5 shrink-0" aria-hidden />
      <span>{label}</span>
    </button>
  );
}

export function EmptyState({
  title,
  body,
  actionLabel,
  onAction,
}: {
  title: string;
  body: string;
  actionLabel?: string;
  onAction?: () => void;
}) {
  return (
    <div className="mag-panel">
      <p className="masthead text-xl text-foreground">{title}</p>
      <p className="mt-2 text-base leading-6 text-muted-foreground">{body}</p>
      {actionLabel && onAction ? (
        <Button className="mt-4" onClick={onAction}>
          {actionLabel}
        </Button>
      ) : null}
    </div>
  );
}

export function Notice({
  title,
  body,
  actionLabel,
  onAction,
  onDismiss,
}: {
  title: string;
  body?: string | null;
  actionLabel?: string;
  onAction?: () => void;
  onDismiss?: () => void;
}) {
  const t = useT();
  return (
    <div role="alert" className="mag-panel mb-3">
      <p className="text-base leading-6 text-foreground">{title}</p>
      {body ? <p className="mt-1 text-sm leading-5 text-muted-foreground">{body}</p> : null}
      {(onAction && actionLabel) || onDismiss ? (
        <div className="mt-3 flex flex-wrap gap-2">
          {onAction && actionLabel ? (
            <Button type="button" onClick={onAction}>
              {actionLabel}
            </Button>
          ) : null}
          {onDismiss ? (
            <Button type="button" variant="outline" onClick={onDismiss}>
              {t("Hide")}
            </Button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

export function RowButton({
  onClick,
  children,
  className,
  style,
}: {
  onClick?: () => void;
  children: React.ReactNode;
  className?: string;
  style?: React.CSSProperties;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={style}
      className={cn(
        "row-item flex min-h-12 w-full shrink-0 items-start gap-3 px-0 py-3.5 text-left transition-colors active:bg-foreground/5",
        className,
      )}
    >
      {children}
    </button>
  );
}
