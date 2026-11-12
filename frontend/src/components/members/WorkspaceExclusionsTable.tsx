import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { FormError } from "@/components/common/FormError";
import { RoleBadge } from "@/components/members/RoleBadge";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { UserAvatar } from "@/components/ui/user-avatar";
import { useLiftWorkspaceExclusion, useWorkspaceExclusions } from "@/hooks/useWorkspaces";
import { apiErrorKey } from "@/lib/api-error";
import type { TeamRole, WorkspaceResponse } from "@/lib/api/types";
import { avatarUrl } from "@/lib/avatar";
import { displayName } from "@/lib/display-name";
import { excludedTarget } from "@/lib/member-targets";
import { canLiftExclusion } from "@/lib/permissions";

type Props = Readonly<{
  workspace: WorkspaceResponse;
  actor: { id: string; access: WorkspaceResponse["my_role"] };
  teamRoles: ReadonlyMap<string, TeamRole>;
}>;

/**
 * The team members kept out of this workspace, by a Captain or an Admin or by leaving it. Only
 * for those who may read it (the page decides), and nothing at all while it is empty.
 */
export function WorkspaceExclusionsTable({ workspace, actor, teamRoles }: Props) {
  const { t } = useTranslation(["workspaces", "common", "errors"]);
  const exclusions = useWorkspaceExclusions(workspace.slug);
  const lift = useLiftWorkspaceExclusion();
  const list = exclusions.data ?? [];
  if (exclusions.isSuccess && list.length === 0) return null;
  if (exclusions.isPending) return null;

  return (
    <section aria-labelledby="ws-exclusions-title" className="rounded-lg border border-border bg-bg-elev">
      <div className="border-b border-border px-3.5 py-2.5">
        <h2 id="ws-exclusions-title" className="text-sm font-semibold text-text">
          {t("workspaces:members.exclusions.title")}
          <span className="ml-1.5 font-normal tabular-nums text-text-muted">{list.length}</span>
        </h2>
        <p className="mt-0.5 text-xs text-text-muted">{t("workspaces:members.exclusions.hint")}</p>
      </div>

      {exclusions.isError ? (
        <div className="p-4">
          <FormError error={exclusions.error} />
        </div>
      ) : null}

      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{t("workspaces:members.columns.member")}</TableHead>
            <TableHead>{t("workspaces:members.exclusions.teamRole")}</TableHead>
            <TableHead className="w-32">
              <span className="sr-only">{t("workspaces:members.columns.actions")}</span>
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {list.map((m) => {
            const teamRole = teamRoles.get(m.user_id);
            const target = excludedTarget(m, teamRoles);
            const name = displayName(m);
            return (
              <TableRow key={m.user_id}>
                <TableCell>
                  <div className="flex items-center gap-2.5">
                    <UserAvatar name={name} size="default" src={avatarUrl("users", m.user_id, m.avatar_version, 64) ?? undefined} />
                    <div className="min-w-0">
                      <div className="truncate text-sm font-medium text-text">{name}</div>
                      <div className="truncate font-mono text-xs text-text-muted">@{m.handle}</div>
                    </div>
                  </div>
                </TableCell>
                <TableCell>{teamRole ? <RoleBadge role={teamRole} /> : null}</TableCell>
                <TableCell className="text-right">
                  {target && canLiftExclusion(workspace, actor, target) ? (
                    <Button
                      variant="secondary"
                      size="sm"
                      disabled={lift.isPending}
                      onClick={() =>
                        lift.mutate(
                          { slug: workspace.slug, userId: m.user_id },
                          {
                            onSuccess: () => toast.success(t("workspaces:members.exclusions.lifted", { name })),
                            onError: (error) => toast.error(t(apiErrorKey(error))),
                          },
                        )
                      }
                    >
                      {t("workspaces:members.exclusions.lift")}
                    </Button>
                  ) : null}
                </TableCell>
              </TableRow>
            );
          })}
        </TableBody>
      </Table>
    </section>
  );
}
