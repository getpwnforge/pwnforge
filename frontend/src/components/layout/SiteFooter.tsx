import { useTranslation } from "react-i18next";
import { Link } from "react-router";

import { Button } from "@/components/ui/button";
import { Logo } from "@/components/brand/Logo";
import { GithubIcon, DiscordIcon } from "@/components/icons/BrandIcons";
import { EXTERNAL, ROUTES } from "@/lib/routes";

const START_YEAR = 2026;
const currentYear = new Date().getFullYear();

// Shared between LandingShell and DocumentShell. Both surfaces use the
// "landing" i18n namespace for these strings, since the footer content
// originated there and is identical in both contexts.
export function SiteFooter() {
  const { t } = useTranslation("landing");

  return (
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
            <Link to={ROUTES.legal} rel="noopener noreferrer">
              {t("footer.legal.notice")}
            </Link>
          </Button>
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
  );
}
