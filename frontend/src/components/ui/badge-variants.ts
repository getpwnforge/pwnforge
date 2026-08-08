import { cva, type VariantProps } from "class-variance-authority"

export const badgeVariants = cva(
  "inline-flex items-center gap-1.5 rounded-full px-[7px] py-1 text-[11.5px] font-medium leading-none whitespace-nowrap",
  {
    variants: {
      variant: {
        default: "bg-surface-2 text-text-muted",
        muted: "border border-border bg-transparent text-text-subtle",
        accent: "bg-ember-soft text-ember-text",
        success: "bg-success-soft text-success-text",
        warning: "bg-warning-soft text-warning-text",
        danger: "bg-danger-soft text-danger-text",
        info: "bg-info-soft text-info-text",
      },
    },
    defaultVariants: { variant: "default" },
  }
)

export type BadgeVariants = VariantProps<typeof badgeVariants>
