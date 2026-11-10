import { useTranslation } from "react-i18next";
import { Outlet, useParams } from "react-router";

import { FormError } from "@/components/common/FormError";
import { Skeleton } from "@/components/ui/skeleton";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useRecordWorkspaceVisit } from "@/hooks/useWorkspaces";
import { isApiError } from "@/lib/api-error";
import { NotFoundPage } from "@/pages/app/NotFoundPage";

/**
 * The element of /w/:slug. Loads the workspace once, records the visit for the recents, and
 * hands it to the pages below through the outlet context (see useCurrentWorkspace).
 */
export function WorkspaceLayout() {
  const { slug } = useParams();
  const { t } = useTranslation(["workspaces"]);
  const workspace = useRecordWorkspaceVisit(slug);
  usePageTitle(workspace.data?.name ?? t("workspaces:list.title"));

  if (workspace.isPending) return <LayoutSkeleton />;
  if (workspace.isError) {
    // Missing, out of reach and excluded are one answer: the API does not tell them apart,
    // and useWorkspace has already taken the slug out of the recents.
    if (isApiError(workspace.error) && workspace.error.status === 404) {
      return <NotFoundPage title={t("workspaces:notFound.title")} description={t("workspaces:notFound.description")} />;
    }
    return (
      <div className="p-6">
        <FormError error={workspace.error} />
      </div>
    );
  }

  return (
    <Outlet context={workspace.data} />
  );
}

function LayoutSkeleton() {
  return (
    <div className="flex flex-col gap-6 p-6" aria-busy="true">
      <div className="flex items-center gap-3.5">
        <Skeleton className="size-12 rounded-[10px]" />
        <div className="flex flex-col gap-2">
          <Skeleton className="h-6 w-56" />
          <Skeleton className="h-4 w-80" />
        </div>
      </div>
      <Skeleton className="h-48 w-full" />
    </div>
  );
}
