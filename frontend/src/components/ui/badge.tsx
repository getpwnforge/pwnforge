import * as React from "react"
import { badgeVariants, type BadgeVariants } from "@/components/ui/badge-variants"
import { cn } from "@/lib/utils"

type BadgeProps = React.ComponentProps<"span"> &
  BadgeVariants & { dot?: string }

function Badge({ className, variant, dot, children, ...props }: BadgeProps) {
  return (
    <span data-slot="badge" className={cn(badgeVariants({ variant }), className)} {...props}>
      {dot ? (
        <span aria-hidden className="size-1.5 shrink-0 rounded-full" style={{ background: dot }} />
      ) : null}
      {children}
    </span>
  )
}

export { Badge }
