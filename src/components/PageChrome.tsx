import { ChevronLeft } from "lucide-react";
import { AppMark } from "@/components/AppMark";
import { Button } from "@/components/ui/button";
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";

export function PageChrome({
  title,
  subtitle,
  info,
  bare = false,
  mark = false,
  children,
  className,
}: {
  title: string;
  subtitle?: string;
  info?: React.ReactNode;
  /** Hide the title when a parent screen already named the page. */
  bare?: boolean;
  mark?: boolean;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex min-h-0 flex-1 flex-col", className)}>
      {bare ? null : (
      <header className="screen-head shrink-0">
        <div className="screen-head-row">
          {mark ? <AppMark className="screen-mark" alt="" /> : null}
          <div className="page-title min-w-0 flex-1">
            <div className="title-with-info">
              <h1 data-fit="title" className="masthead text-foreground">{title}</h1>
              {info}
            </div>
            {subtitle ? <p className="page-sub mt-2 text-sm leading-5 text-muted-foreground">{subtitle}</p> : null}
          </div>
        </div>
        <div className="masthead-rule" />
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
  center = false,
}: {
  title: string;
  body: string;
  actionLabel?: string;
  onAction?: () => void;
  center?: boolean;
}) {
  return (
    <div className={center ? "empty-center" : "mag-panel"}>
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
