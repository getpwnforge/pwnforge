import { cva, type VariantProps } from "class-variance-authority"

export const tagVariants = cva(
  "inline-flex items-center rounded-sm px-1.5 py-px font-mono text-[11.5px] font-medium leading-relaxed whitespace-nowrap",
  {
    variants: {
      variant: {
        custom: "bg-surface-2 text-text-muted",
        system: "bg-ember-soft text-ember-text",
      },
    },
    defaultVariants: { variant: "custom" },
  }
)

export type TagVariants = VariantProps<typeof tagVariants>
