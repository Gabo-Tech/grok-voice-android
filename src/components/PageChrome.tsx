import { Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export function PageChrome({
  title,
  subtitle,
  onAdd,
  addLabel,
  children,
  className,
}: {
  title: string;
  subtitle?: string;
  onAdd?: () => void;
  addLabel?: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex min-h-0 flex-1 flex-col", className)}>
      <header className="shrink-0 px-1 pb-4 pt-1">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <h1 className="text-2xl leading-tight font-medium tracking-tight text-foreground">{title}</h1>
            {subtitle && <p className="mt-1 text-xs leading-normal text-muted-foreground">{subtitle}</p>}
          </div>
          {onAdd && (
            <Button size="icon" variant="outline" className="fab-pulse shrink-0" onClick={onAdd} aria-label={addLabel ?? "Add"}>
              <Plus />
            </Button>
          )}
        </div>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto pb-2">{children}</div>
    </div>
  );
}

export function EmptyState({ title, body }: { title: string; body: string }) {
  return (
    <div className="px-1 py-8">
      <p className="text-base leading-6 text-foreground">{title}</p>
      <p className="mt-2 text-sm leading-5 text-muted-foreground">{body}</p>
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
        "row-item flex w-full shrink-0 items-start gap-3 px-1 py-3.5 text-left transition-colors active:bg-white/5",
        className,
      )}
    >
      {children}
    </button>
  );
}
