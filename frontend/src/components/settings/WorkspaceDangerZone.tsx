import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { toast } from "sonner";

import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { SettingsSection } from "@/components/settings/settings-layout";
import { Button } from "@/components/ui/button";
import { useArchiveWorkspace, useDeleteWorkspace, useLeaveWorkspace, useUnarchiveWorkspace } from "@/hooks/useWorkspaces";
import { apiErrorKey } from "@/lib/api-error";
import type { WorkspaceResponse } from "@/lib/api/types";
import { canArchiveWorkspace, canDeleteWorkspace, canLeaveWorkspace } from "@/lib/permissions";
import { ROUTES } from "@/lib/routes";

/**
 * Two blocks, as on the mockup: what can be undone (archive or unarchive, leave), then what
 * cannot (delete). Each action shows only to whom it is allowed, and a block with nothing in it
 * is left out. Archived, only these remain usable.
 */
export function WorkspaceDangerZone({ workspace }: Readonly<{ workspace: WorkspaceResponse }>) {
  const { t } = useTranslation(["workspaces", "errors"]);
  const navigate = useNavigate();
  const archive = useArchiveWorkspace();
  const unarchive = useUnarchiveWorkspace();
  const del = useDeleteWorkspace();
  const leave = useLeaveWorkspace();
  const access = workspace.my_role;
  const onError = (error: Error) => toast.error(t(apiErrorKey(error)));
  // Leaving or deleting empties the workspace from the cache: go before its 404 shows.
  const gone = { onSuccess: () => void navigate(ROUTES.workspaces, { replace: true }), onError };

  const canArchive = canArchiveWorkspace(access);
  const canLeave = canLeaveWorkspace(workspace, access);
  const canDelete = canDeleteWorkspace(access);

  return (
    <div className="flex flex-col gap-7">
      {canArchive || canLeave ? (
        <SettingsSection id="ws-danger-reversible" title={t("workspaces:settings.dangerReversible")}>
          <div className="divide-y divide-border rounded-lg border border-border px-4">
            {canArchive && workspace.archived ? (
              <DangerRow label={t("workspaces:settings.unarchive")} hint={t("workspaces:settings.unarchiveHint")}>
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={unarchive.isPending}
                  onClick={() =>
                    unarchive.mutate(workspace.slug, { onSuccess: () => toast.success(t("workspaces:settings.unarchived")), onError })
                  }
                >
                  {t("workspaces:settings.unarchive")}
                </Button>
              </DangerRow>
            ) : null}
            {canArchive && !workspace.archived ? (
              <DangerRow label={t("workspaces:settings.archive")} hint={t("workspaces:settings.archiveHint")}>
                <ConfirmDialog
                  trigger={<Button variant="secondary" size="sm">{t("workspaces:settings.archive")}</Button>}
                  title={t("workspaces:settings.archiveTitle", { name: workspace.name })}
                  description={t("workspaces:settings.archiveText")}
                  confirmLabel={t("workspaces:settings.archive")}
                  pending={archive.isPending}
                  onConfirm={() =>
                    archive.mutate(workspace.slug, { onSuccess: () => toast.success(t("workspaces:settings.archived")), onError })
                  }
                />
              </DangerRow>
            ) : null}
            {canLeave ? (
              <DangerRow label={t("workspaces:settings.leaveAction")} hint={t("workspaces:settings.leaveHint")}>
                <ConfirmDialog
                  trigger={<Button variant="secondary" size="sm">{t("workspaces:settings.leaveAction")}</Button>}
                  title={t("workspaces:settings.leaveTitle", { name: workspace.name })}
                  description={t("workspaces:settings.leaveDescription")}
                  confirmLabel={t("workspaces:settings.leaveAction")}
                  danger
                  pending={leave.isPending}
                  onConfirm={() => leave.mutate(workspace.slug, gone)}
                />
              </DangerRow>
            ) : null}
          </div>
        </SettingsSection>
      ) : null}

      {canDelete ? (
        <SettingsSection id="ws-danger-destructive" title={t("workspaces:settings.dangerDestructive")}>
          <div className="rounded-lg border border-danger/40 bg-danger/5 px-4">
            <DangerRow label={t("workspaces:settings.delete")} hint={t("workspaces:settings.deleteHint")}>
              <ConfirmDialog
                trigger={<Button variant="danger" size="sm">{t("workspaces:settings.delete")}</Button>}
                title={t("workspaces:settings.deleteTitle", { name: workspace.name })}
                description={t("workspaces:settings.deleteText", { slug: workspace.slug })}
                confirmLabel={t("workspaces:settings.delete")}
                danger
                // The slug, not the name: unique, plain ASCII, nothing to get wrong when retyping it.
                confirmText={workspace.slug}
                pending={del.isPending}
                onConfirm={() => del.mutate(workspace.slug, gone)}
              />
            </DangerRow>
          </div>
        </SettingsSection>
      ) : null}
    </div>
  );
}

function DangerRow({ label, hint, children }: Readonly<{ label: string; hint: string; children: ReactNode }>) {
  return (
    <div className="flex flex-wrap items-center gap-4 py-2.5">
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-medium text-text">{label}</div>
        <div className="mt-0.5 text-xs text-text-muted">{hint}</div>
      </div>
      {children}
    </div>
  );
}
