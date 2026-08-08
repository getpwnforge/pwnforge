import * as React from "react"
import { Slot } from "radix-ui"
import { buttonVariants } from "@/components/ui/button-variants"
import { type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"


function Button({
  className,
  variant = "primary",
  size = "default",
  asChild = false,
  ...props
}: React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean
  }) {
  const Comp = asChild ? Slot.Root : "button"

  return (
    <Comp
      data-slot="button"
      data-variant={variant}
      data-size={size}
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  )
}

export { Button }
