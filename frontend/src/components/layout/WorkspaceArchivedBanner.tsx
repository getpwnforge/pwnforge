import { ArchiveIcon } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Banner } from "@/components/ui/banner";
import { useWorkspace, useWorkspaceSlug } from "@/hooks/useWorkspaces";

/**
 * Read-only, as the API enforces it, on every page of an archived workspace. The workspace
 * comes from the cache WorkspaceLayout fills: no request of its own, and nothing outside /w/:slug.
 */
export function WorkspaceArchivedBanner() {
  const { t } = useTranslation(["workspaces"]);
  const slug = useWorkspaceSlug();
  const workspace = useWorkspace(slug ?? undefined);

  if (!workspace.data?.archived) return null;

  return (
    <Banner variant="warning" icon={<ArchiveIcon aria-hidden className="size-4 shrink-0" />}>
      <span className="truncate">{t("workspaces:archived.banner")}</span>
    </Banner>
  );
}
