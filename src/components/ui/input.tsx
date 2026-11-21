import * as React from "react"
import { cn } from "cn"

function Input({ className, type, ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      data-slot="input"
      className={cn(
        "h-12 w-full min-w-0 rounded-none border-0 border-b border-[var(--line)] bg-transparent px-0 text-base text-foreground outline-none placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-50",
        "focus-visible:border-[var(--text-primary)]",
        "aria-invalid:border-foreground",
        className
      )}
      {...props}
    />
  )
}

export { Input }
