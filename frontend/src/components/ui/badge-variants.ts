import { cva, type VariantProps } from "class-variance-authority"

export const badgeVariants = cva(
  "inline-flex items-center gap-1.5 rounded-full px-[7px] py-1.5 text-[11.5px] font-medium leading-none whitespace-nowrap border",
  {
    variants: {
      // Each variant sets its border color; it only shows once `border` adds a width.
      variant: {
        default: "border-border-strong bg-surface-2 text-text-muted",
        muted: "border border-border bg-transparent text-text-subtle",
        accent: "border-ember/45 bg-ember-soft text-ember-text",
        success: "border-success/45 bg-success-soft text-success-text",
        warning: "border-warning/45 bg-warning-soft text-warning-text",
        danger: "border-danger/45 bg-danger-soft text-danger-text",
        info: "border-info/45 bg-info-soft text-info-text",
      }
    },
    defaultVariants: { variant: "default" },
  }
)

export type BadgeVariants = VariantProps<typeof badgeVariants>
