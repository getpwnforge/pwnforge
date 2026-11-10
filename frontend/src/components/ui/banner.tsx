import { XIcon } from "lucide-react";
import type { ComponentProps, ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { bannerVariants, type BannerVariant } from "@/components/ui/banner-variants";
import { cn } from "@/lib/utils";

type BannerProps = Omit<ComponentProps<"output">, "children"> & {
  variant?: BannerVariant;
  icon?: ReactNode;
  children: ReactNode;
  /** Shown only when given: a state that lasts (an archived workspace) has nothing to dismiss. */
  onDismiss?: () => void;
  dismissLabel?: string;
};

/**
 * A notice about the whole screen, rendered by AppShell above the sidebar and the page.
 * An <output>, so assistive technologies announce it when it appears.
 */
export function Banner({ variant, icon, children, onDismiss, dismissLabel, className, ...props }: Readonly<BannerProps>) {
  return (
    <output className={cn(bannerVariants({ variant }), className)} {...props}>
      {icon}
      {children}
      {onDismiss ? (
        <Button variant="ghost" size="icon" className="shrink-0 p-1" onClick={onDismiss} aria-label={dismissLabel}>
          <XIcon aria-hidden className="size-4" />
        </Button>
      ) : null}
    </output>
  );
}
