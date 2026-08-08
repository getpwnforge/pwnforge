import * as React from "react"
import { Badge } from "@/components/ui/badge"
import { statusBorder, type StatusVariant } from "@/components/ui/status-badge-variants"
import { cn } from "@/lib/utils"

type StatusBadgeProps = React.ComponentProps<"span"> & {
  variant?: StatusVariant
}

function StatusBadge({ variant = "default", className, children, ...props }: StatusBadgeProps) {
  return (
    <Badge variant={variant} className={cn("border", statusBorder[variant], className)} {...props}>
      {children}
    </Badge>
  )
}

export { StatusBadge }
