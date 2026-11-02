import { ArrowLeftIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "react-router";
import { Button } from "@/components/ui/button";
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { usePageTitle } from "@/hooks/usePageTitle";
import { ROUTES } from "@/lib/routes";

type Props = Readonly<{ title?: string; description?: string }>;

/**
 * React Router numbers its history entries in `history.state.idx`: 0 is the first page of the
 * app in this tab. Above 0, going back stays in PwnForge; at 0 it would leave the site.
 */
function canGoBack(): boolean {
  const state = window.history.state as { idx?: number } | null;
  return (state?.idx ?? 0) > 0;
}

export function NotFoundPage({ title, description }: Props = {}) {
  const { t } = useTranslation(["common"]);
  usePageTitle(t("notFound.title"));
  const navigate = useNavigate();

  return (
    <Empty className="h-full">
      <EmptyHeader>
        <EmptyTitle>{title ?? t("notFound.title")}</EmptyTitle>
        <EmptyDescription>{description ?? t("notFound.description")}</EmptyDescription>
      </EmptyHeader>
      <EmptyContent className="flex-row justify-center gap-2">
        {canGoBack() ? (
          <Button variant="primary" onClick={() => void navigate(-1)}>
            <ArrowLeftIcon aria-hidden />
            {t("notFound.back")}
          </Button>
        ) : null}
        <Button asChild variant={canGoBack() ? "ghost" : "primary"}>
          <Link to={ROUTES.workspaces}>{t("notFound.home")}</Link>
        </Button>
      </EmptyContent>
    </Empty>
  );
}
