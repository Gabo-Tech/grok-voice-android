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
      <span className="mb-2 block text-sm leading-5 text-muted-foreground">{label}</span>
      {children}
      {error ? (
        <span role="alert" className="mt-2 block text-sm leading-5 text-foreground">
          {error}
        </span>
      ) : hint ? (
        <span className="mt-2 block text-sm leading-5 text-muted-foreground">{hint}</span>
      ) : null}
    </label>
  );
}
