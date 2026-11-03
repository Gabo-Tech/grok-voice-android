import type { ReactNode } from "react";

export function Field({
  label,
  hint,
  error,
  children,
}: {
  label: string;
  hint?: string;
  error?: string | null;
  children: ReactNode;
}) {
  return (
    <label className="block">
      <span className="mb-2 block text-[11px] tracking-[0.16em] text-muted-foreground uppercase">{label}</span>
      {children}
      {error ? (
        <span className="mt-2 block text-sm text-foreground">{error}</span>
      ) : hint ? (
        <span className="mt-2 block text-sm text-muted-foreground">{hint}</span>
      ) : null}
    </label>
  );
}
