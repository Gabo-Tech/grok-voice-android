import { useEffect, useRef } from "react";
import { ChevronLeft } from "lucide-react";
import { Button } from "@/components/ui/button";
import { setBarAction } from "@/lib/history";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

export function PageChrome({
  title,
  subtitle,
  onAdd,
  addLabel,
  active = true,
  children,
  className,
}: {
  title: string;
  subtitle?: string;
  onAdd?: () => void;
  addLabel?: string;
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
      <header className="shrink-0 px-1 pb-4 pt-1">
        <div className="min-w-0">
          <h1 className="text-2xl leading-tight font-medium tracking-tight text-foreground">{title}</h1>
          {subtitle && <p className="mt-1 text-sm leading-5 text-muted-foreground">{subtitle}</p>}
        </div>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto pb-2">{children}</div>
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
    <div className="px-1 py-8">
      <p className="text-lg leading-7 text-foreground">{title}</p>
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
    <div role="alert" className="mb-3 border border-foreground/40 px-3 py-3">
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
        "row-item flex min-h-12 w-full shrink-0 items-start gap-3 px-1 py-3.5 text-left transition-colors active:bg-white/5",
        className,
      )}
    >
      {children}
    </button>
  );
}
