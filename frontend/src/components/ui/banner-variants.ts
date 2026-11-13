import { cva, type VariantProps } from "class-variance-authority";

/** A full-width strip above the app; its height is the --banner-h token. */
export const bannerVariants = cva(
  "flex h-(--banner-h) shrink-0 items-center justify-center gap-3 border-b px-4 text-[13px] font-medium",
  {
    variants: {
      variant: {
        info: "border-info/45 bg-info-soft text-info-text",
        warning: "border-warning/45 bg-warning-soft text-warning-text",
        danger: "border-danger/45 bg-danger-soft text-danger-text",
      },
    },
    defaultVariants: { variant: "info" },
  },
);

export type BannerVariant = NonNullable<VariantProps<typeof bannerVariants>["variant"]>;
