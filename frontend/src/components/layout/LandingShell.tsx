import * as React from "react";
import { useTranslation } from "react-i18next";
import { Link, Outlet, useLocation, useMatch } from "react-router";

import { Button } from "@/components/ui/button";
import { Logo } from "@/components/brand/Logo";

import { ExternalLink } from "lucide-react";
import { GithubIcon, DiscordIcon } from "@/components/icons/BrandIcons";

import { EXTERNAL, ROUTES } from "@/lib/routes";
import { cn } from "@/lib/utils";
import { useEffect } from "react";

const START_YEAR = 2026;
const currentYear = new Date().getFullYear();
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

function useHashScroll() {
  const { hash } = useLocation();

  useEffect(() => {
    if (!hash) return;
    // Wait for the target section to exist in the DOM
    const el = document.getElementById(hash.slice(1));
    if (el) el.scrollIntoView({ behavior: "smooth" });
  }, [hash]);
}

function LandingNav() {
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
              <Link to={ROUTES.login} >
                {t("nav.signIn")}
              </Link>
            </Button>
            <Button asChild size="lg" className="text-h3!">
              <Link to={ROUTES.register} >
                {t("nav.getStarted")}
              </Link>
            </Button>
          </div>
        </div>
      </div>
    </nav>
  );
}

export function LandingShell() {
  const { t } = useTranslation("landing");
  useHashScroll();

  return (
    <div className="flex min-h-dvh flex-col">
      {/* Landing Navbar */}
      <LandingNav />

      {/* Landing content */}
      <main className="flex flex-col items-center justify-center gap-18 px-6 py-10">
        <Outlet />
      </main>

      {/* Landing Footer */}
      <footer className="border-t border-border bg-bg-elev px-6 py-4 text-sm text-muted-foreground mt-auto">
        <div className="grid grid-cols-2 gap-8 lg:grid-cols-4">
          <div className="col-span-2 lg:col-span-1">
            <Logo size="md" type="noBg" />

          </div>
          <div className="flex flex-col items-start gap-2">
            <span className="text-text-subtle uppercase font-mono tracking-wider">{t("footer.resources.title")}</span>
            <Button asChild variant="link" className="text-muted-foreground hover:text-foreground">
              <Link to={EXTERNAL.docs} target="_blank" rel="noopener noreferrer">
                {t("footer.resources.docs")}
              </Link>
            </Button>
            <Button asChild variant="link" className="text-muted-foreground hover:text-foreground">
              <Link to={EXTERNAL.api} target="_blank" rel="noopener noreferrer">
                {t("footer.resources.api")}
              </Link>
            </Button>
            <Button asChild variant="link" className="text-muted-foreground hover:text-foreground">
              <Link to={EXTERNAL.selfhost} target="_blank" rel="noopener noreferrer">
                {t("footer.resources.selfhosted")}
              </Link>
            </Button>
            <Button asChild variant="link" className="text-muted-foreground hover:text-foreground">
              <Link to={ROUTES.changelog} target="_blank" rel="noopener noreferrer">
                {t("footer.resources.changelog")}
              </Link>
            </Button>
            <Button asChild variant="link" className="text-muted-foreground hover:text-foreground">
              <Link to={EXTERNAL.status} target="_blank" rel="noopener noreferrer">
                {t("footer.resources.status")}
              </Link>
            </Button>
          </div>
          <div className="flex flex-col items-start gap-2">
            <span className="text-text-subtle uppercase font-mono tracking-wider">{t("footer.community.title")}</span>
            <Button asChild variant="link" className="text-muted-foreground hover:text-foreground">
              <Link to={EXTERNAL.github} target="_blank" rel="noopener noreferrer">
                <GithubIcon />
                {t("footer.community.github")}
              </Link>
            </Button>
            <Button asChild variant="link" className="text-muted-foreground hover:text-foreground">
              <Link to={EXTERNAL.discord} target="_blank" rel="noopener noreferrer">
                <DiscordIcon />
                {t("footer.community.discord")}
              </Link>
            </Button>
            <Button asChild variant="link" className="text-muted-foreground hover:text-foreground">
              <Link to={EXTERNAL.contributing} target="_blank" rel="noopener noreferrer">
                {t("footer.community.contributing")}
              </Link>
            </Button>
          </div>
          <div className="flex flex-col items-start gap-2">
            <span className="text-text-subtle uppercase font-mono tracking-wider">{t("footer.legal.title")}</span>
            <Button asChild variant="link" className="text-muted-foreground hover:text-foreground">
              <Link to={ROUTES.privacy} rel="noopener noreferrer">
                {t("footer.legal.privacy")}
              </Link>
            </Button>
            <Button asChild variant="link" className="text-muted-foreground hover:text-foreground">
              <Link to={ROUTES.terms} rel="noopener noreferrer">
                {t("footer.legal.terms")}
              </Link>
            </Button>
            <Button asChild variant="link" className="text-muted-foreground hover:text-foreground">
              <Link to={EXTERNAL.security} target="_blank" rel="noopener noreferrer">
                {t("footer.legal.security")}
              </Link>
            </Button>
            <Button asChild variant="link" className="text-muted-foreground hover:text-foreground">
              <Link to={ROUTES.contact} rel="noopener noreferrer">
                {t("footer.legal.contact")}
              </Link>
            </Button>
          </div>
        </div>
        <div className="flex flex-row gap-1 justify-between mt-4 border-t border-border pt-4">
          <span className="text-muted-foreground text-sm">
            {currentYear > START_YEAR
              ? t("footer.copyrightRange", { startYear: START_YEAR, year: currentYear })
              : t("footer.copyright", { year: currentYear })}
          </span>
          <span>
            {t("footer.licensePrefix")}{" "}
            <a href={EXTERNAL.license} target="_blank" rel="noopener noreferrer" className="underline underline-offset-2 hover:text-foreground">
              {t("footer.licenseName")}
            </a>
          </span>
        </div>
      </footer>
    </div>
  );
}
