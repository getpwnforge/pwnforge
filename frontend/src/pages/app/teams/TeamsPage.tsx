import { PlusIcon, SettingsIcon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { WsIcon } from "@/components/brand/WsIcon";
import { FormError } from "@/components/common/FormError";
import { PendingTeamInvitationsCard } from "@/components/invitations/PendingInvitationsCard";
import { PageChrome } from "@/components/layout/PageChrome";
import { RoleBadge } from "@/components/members/RoleBadge";
import { CreateTeamDialog } from "@/components/teams/CreateTeamDialog";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useTeams } from "@/hooks/useTeams";
import { useWorkspaces } from "@/hooks/useWorkspaces";
import type { TeamResponse, WorkspaceSummary } from "@/lib/api/types";
import { avatarUrl } from "@/lib/avatar";
import { ROUTES } from "@/lib/routes";

const GRID = "grid gap-3 sm:grid-cols-2";

/** How many of my workspaces each team owns, from the list already loaded for the sidebar. */
function workspacesByTeam(workspaces: readonly WorkspaceSummary[] | undefined): Map<string, number> {
  const counts = new Map<string, number>();
  for (const ws of workspaces ?? []) {
    if (ws.team) counts.set(ws.team.id, (counts.get(ws.team.id) ?? 0) + 1);
  }
  return counts;
}

export function TeamsPage() {
  const { t } = useTranslation(["teams", "nav"]);
  usePageTitle(t("teams:list.title"));
  const teams = useTeams();
  const workspaceCounts = workspacesByTeam(useWorkspaces().data);
  const [creating, setCreating] = useState(false);

  return (
    <>
      <div className="flex min-w-0 flex-col">
        <PageChrome crumbs={[{ label: t("nav:sections.personalSpace") }, { label: t("teams:list.title") }]} />
        <div id="main">
          <div id="page" className="flex flex-col gap-6 px-6 py-8">
            <div id="page-header" className="flex flex-wrap items-end justify-between gap-4">
              <div>
                <h1 className="text-page font-semibold text-text">{t("teams:list.title")}</h1>
                <p className="text-text-muted">{t("teams:list.subtitle")}</p>
              </div>
              <Button variant="primary" onClick={() => setCreating(true)}>
                <PlusIcon aria-hidden />
                {t("teams:list.new")}
              </Button>
            </div>

            {teams.isError ? <FormError error={teams.error} /> : null}

            {teams.isPending ? (
              <div className={GRID}>
                {[0, 1].map((i) => <Skeleton key={i} className="h-36 rounded-lg" />)}
              </div>
            ) : (
              <div className={GRID}>
                {teams.data?.map((team) => (
                  <TeamCard key={team.id} team={team} workspaces={workspaceCounts.get(team.id) ?? 0} />
                ))}
                {/* As on the mockup: invitations, then the dashed card, end the grid. */}
                <PendingTeamInvitationsCard />
                <button
                  type="button"
                  onClick={() => setCreating(true)}
                  className="flex min-h-36 cursor-pointer flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-border text-text-muted hover:border-border-strong"
                >
                  <span className="grid size-9 place-items-center rounded-md bg-surface">
                    <PlusIcon aria-hidden />
                  </span>
                  <span className="text-sm font-medium text-text">{t("teams:list.newCard.title")}</span>
                  <span className="text-xs">{t("teams:list.newCard.hint")}</span>
                </button>
              </div>
            )}
          </div>
        </div>
      </div>
      <CreateTeamDialog open={creating} onOpenChange={setCreating} />
    </>
  );
}

/** A team as the mockup shows it; the whole card opens the team, its buttons go to a tab. */
function TeamCard({ team, workspaces }: Readonly<{ team: TeamResponse; workspaces: number }>) {
  const { t } = useTranslation(["teams"]);
  return (
    <Card className="relative gap-0 p-0 transition-colors hover:border-border-strong">
      <div className="flex items-start gap-3 p-4">
        <WsIcon name={team.name} color={team.color} size="lg" src={avatarUrl("teams", team.id, team.avatar_version, 256)} />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            {/* The link stretches over the card: a click anywhere opens the team. */}
            <Link
              to={ROUTES.team(team.slug)}
              className="truncate text-base font-semibold text-text after:absolute after:inset-0 after:rounded-lg focus-visible:outline-none focus-visible:after:ring-2 focus-visible:after:ring-ring"
            >
              {team.name}
            </Link>
            <RoleBadge role={team.my_role} />
          </div>
          <div className="text-sm text-text-muted">
            <span>{t("teams:card.members", { count: team.member_count })}</span>
            {" · "}
            <span>{t("teams:card.workspaces", { count: workspaces })}</span>
          </div>
        </div>
      </div>

      <div className="flex items-center justify-end gap-1 border-t border-border px-4 py-2.5">
        {/* Above the stretched link, so they keep their own target. */}
        <Button asChild variant="secondary" size="sm" className="relative z-10">
          <Link to={ROUTES.teamWorkspaces(team.slug)}>{t("teams:card.openWorkspaces")}</Link>
        </Button>
        <Button asChild variant="ghost" size="icon-sm" className="relative z-10">
          <Link to={ROUTES.teamSettings(team.slug)} aria-label={t("teams:card.settings", { name: team.name })}>
            <SettingsIcon aria-hidden />
          </Link>
        </Button>
      </div>
    </Card>
  );
}
