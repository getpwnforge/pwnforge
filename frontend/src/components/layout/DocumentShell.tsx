import { ArrowLeft } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Outlet, useLocation, useNavigate } from "react-router";

import { Button } from "@/components/ui/button";
import { Logo } from "@/components/brand/Logo";
import { SiteFooter } from "@/components/layout/SiteFooter";
import { useAuth } from "@/hooks/useAuth";
import { ROUTES } from "@/lib/routes";

function BackButton() {
  const { t } = useTranslation("common");
  const navigate = useNavigate();
  const location = useLocation();
  const { user } = useAuth();

  // "default" means there is no entry for this session in the router's
  // history (direct link, new tab, link opened from an email). navigate(-1)
  // would then leave the app or land on an unrelated page, so fall back to
  // a sensible default depending on auth state instead.
  const hasHistory = location.key !== "default";

  function handleBack() {
    if (hasHistory) {
      navigate(-1);
      return;
    }
    navigate(user ? ROUTES.workspaces : ROUTES.landing);
  }

  return (
    <Button variant="ghost" size="sm" onClick={handleBack} className="shrink-0 gap-1.5">
      <ArrowLeft className="size-4" />
      {t("actions.back")}
    </Button>
  );
}

// Used for standalone content pages (legal notice, privacy policy, terms,
// changelog...) reached either from the public landing footer or from
// inside the authenticated app (settings, a legal-update banner). Unlike
// LandingShell, this has no marketing nav and no scroll-spy: both audiences
// would find that chrome irrelevant or actively misleading (e.g. a
// "Sign in" button shown to an already-authenticated user).
export function DocumentShell() {
  return (
    <div className="flex min-h-dvh flex-col">
      <header className="border-b border-border bg-bg-elev px-6 py-3">
        <div className="mx-auto w-full">
          <Logo size="md" type="noBg" />
        </div>
      </header>

      {/* Two-column reading layout: BackButton gets its own column next to
          the document rather than sitting above or inside its title, and
          stays sticky while the (often long) document scrolls. Stacks to a
          single column on mobile since there's no room for a side column
          on narrow viewports. */}
      <main className="mx-auto grid w-full max-w-4xl flex-1 grid-cols-1 items-start gap-6 px-6 py-10 md:grid-cols-[auto_1fr] md:gap-8">
        <div className="md:sticky md:top-6">
          <BackButton />
        </div>
        <Outlet />
      </main>

      <SiteFooter />
    </div>
  );
}
