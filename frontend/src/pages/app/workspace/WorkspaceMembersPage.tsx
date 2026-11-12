import { PlusIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link, Navigate } from "react-router";

import { InviteToWorkspaceDialog } from "@/components/invitations/InviteToWorkspaceDialog";
import { WorkspaceInvitationsTable } from "@/components/invitations/WorkspaceInvitationsTable";
import { PageChrome } from "@/components/layout/PageChrome";
import { WorkspaceExclusionsTable } from "@/components/members/WorkspaceExclusionsTable";
import { WorkspaceMembersTable } from "@/components/members/WorkspaceMembersTable";
import { Button } from "@/components/ui/button";
import { useAuth } from "@/hooks/useAuth";
import { useCurrentWorkspace } from "@/hooks/useCurrentWorkspace";
import { useTeamMembers, useTeams } from "@/hooks/useTeams";
import { teamRolesById } from "@/lib/member-targets";
import { canInviteToWorkspace, canReadExclusions, satisfies } from "@/lib/permissions";
import { ROUTES } from "@/lib/routes";

/**
 * Who has access to a team workspace and where their role comes from, the invitations still
 * pending and the members kept out. A solo workspace has nobody else: it goes to its dashboard.
 */
export function WorkspaceMembersPage() {
  const { t } = useTranslation(["workspaces"]);
  const workspace = useCurrentWorkspace();
  const { user } = useAuth();
  const teams = useTeams();
  const team = workspace.team_id ? teams.data?.find((candidate) => candidate.id === workspace.team_id) : undefined;
  const access = workspace.my_role;
  // The team's roster gives the team role an override hides. Only those who manage need it.
  const manages = !!team && satisfies(access, "admin");
  const teamMembers = useTeamMembers(manages ? team.slug : undefined);

  if (workspace.mode === "solo") return <Navigate to={ROUTES.workspace(workspace.slug)} replace />;
  if (!user) return null;

  const actor = { id: user.id, access };
  const teamRoles = teamRolesById(teamMembers.data);
  const canInvite = canInviteToWorkspace(workspace, access, !!team);

  return (
    <>
      <PageChrome
        crumbs={[{ label: workspace.name, to: ROUTES.workspace(workspace.slug) }, { label: t("workspaces:members.title") }]}
      />
      <div className="flex flex-col gap-5 p-6">
        <header className="flex flex-wrap items-end gap-3">
          <div className="min-w-0 flex-1">
            <h1 className="text-page font-semibold tracking-tight text-text">{t("workspaces:members.title")}</h1>
            <p className="mt-1 text-[13px] text-text-muted">
              {team ? (
                <>
                  {t("workspaces:members.subtitleTeam")}{" "}
                  <Link to={ROUTES.team(team.slug)} className="text-text hover:underline">
                    {team.name}
                  </Link>
                </>
              ) : (
                t("workspaces:members.subtitleGuest")
              )}
            </p>
          </div>
          {canInvite ? (
            <InviteToWorkspaceDialog
              workspace={workspace}
              trigger={
                <Button variant="primary">
                  <PlusIcon aria-hidden />
                  {t("workspaces:members.invite.open")}
                </Button>
              }
            />
          ) : null}
        </header>

        <WorkspaceMembersTable workspace={workspace} actor={actor} teamRoles={teamRoles} />
        {canInvite ? <WorkspaceInvitationsTable workspace={workspace} me={user.id} /> : null}
        {canReadExclusions(workspace, access) ? (
          <WorkspaceExclusionsTable workspace={workspace} actor={actor} teamRoles={teamRoles} />
        ) : null}
      </div>
    </>
  );
}
