import * as React from "react";
import { useTranslation } from "react-i18next";
import { Link, useMatch } from "react-router";
import { ExternalLink } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Logo } from "@/components/brand/Logo";
import { EXTERNAL, ROUTES } from "@/lib/routes";
import { cn } from "@/lib/utils";

const SECTIONS = ["product", "solutions", "community"] as const;

function useActiveSection(enabled: boolean) {
  const [active, setActive] = React.useState<string>(SECTIONS[0]);

  React.useEffect(() => {
    if (!enabled) return;

    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((e) => e.isIntersecting)
          .sort((a, b) => b.intersectionRatio - a.intersectionRatio)[0];
        if (visible) setActive(visible.target.id);
      },
      // Detection band in the middle of the viewport, so a section becomes
      // active when it crosses the center rather than when it first appears.
      { rootMargin: "-40% 0px -55% 0px" }
    );

    SECTIONS.forEach((id) => {
      const el = document.getElementById(id);
      if (el) observer.observe(el);
    });

    return () => observer.disconnect();
  }, [enabled]);

  return active;
}

export function LandingHeader() {
  const { t } = useTranslation("landing");
  const isHome = useMatch(ROUTES.landing);
  const active = useActiveSection(Boolean(isHome));

  return (
    <nav className="sticky top-0 z-50 border-b border-border bg-bg-elev">
      <div className="mx-auto flex w-full justify-between gap-6 px-6 py-3">
        <Link to={ROUTES.landing} className="shrink-0">
          <Logo size="md" type="noBg" />
        </Link>

        <div className="flex flex-1 items-center justify-end gap-10">
          <div className="hidden items-center gap-1 lg:flex">
            {SECTIONS.map((id) => (
              <Link
                key={id}
                to={{ pathname: ROUTES.landing, hash: `#${id}` }}
                className={cn(
                  "border-b-2 px-3 py-1.5 text-h3 font-medium transition-colors",
                  isHome && active === id
                    ? "border-ember text-foreground font-bold"
                    : "border-transparent text-muted-foreground hover:text-foreground"
                )}
              >
                {t(`nav.${id}`)}
              </Link>
            ))}
            <a
              href={EXTERNAL.docs}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1 border-b-2 border-transparent px-3 py-1.5 text-h3 font-medium text-muted-foreground transition-colors hover:text-foreground"
            >
              {t("nav.docs")}
              <ExternalLink className="size-3.5" />
            </a>
          </div>

          <div className="flex shrink-0 items-center gap-2">
            <Button asChild variant="ghost" size="lg" className="text-h3!">
              <Link to={ROUTES.login}>
                {t("nav.signIn")}
              </Link>
            </Button>
            <Button asChild size="lg" className="text-h3!">
              <Link to={ROUTES.register}>
                {t("nav.getStarted")}
              </Link>
            </Button>
          </div>
        </div>
      </div>
    </nav>
  );
}
