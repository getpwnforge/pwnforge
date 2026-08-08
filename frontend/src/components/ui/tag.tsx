import * as React from "react"
import { tagVariants, type TagVariants } from "@/components/ui/tag-variants"
import { cn } from "@/lib/utils"

type TagProps = React.ComponentProps<"span"> & TagVariants

function Tag({ className, variant, ...props }: TagProps) {
  return (
    <span data-slot="tag" className={cn(tagVariants({ variant }), className)} {...props} />
  )
}

export { Tag }
