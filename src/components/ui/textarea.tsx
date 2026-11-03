import * as React from "react"
import { cn } from "cn"

function Textarea({ className, ...props }: React.ComponentProps<"textarea">) {
  return (
    <textarea
      data-slot="textarea"
      className={cn(
        "min-h-32 w-full border border-input bg-transparent px-3 py-3 text-base leading-relaxed text-foreground outline-none placeholder:text-muted-foreground",
        "focus-visible:border-foreground",
        "aria-invalid:border-foreground",
        className
      )}
      {...props}
    />
  )
}

export { Textarea }
