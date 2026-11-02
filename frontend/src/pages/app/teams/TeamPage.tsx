import { useTranslation } from "react-i18next";
import { useParams, useSearchParams } from "react-router";
import { WsIcon } from "@/components/brand/WsIcon";
import { FormError } from "@/components/common/FormError";
import { PageChrome } from "@/components/layout/PageChrome";
import { AvatarGroup, AvatarGroupCount } from "@/components/ui/avatar";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsList, TabsPanel, TabsTab } from "@/components/ui/tabs";
import { UserAvatar } from "@/components/ui/user-avatar";
import { useTeamInvitations } from "@/hooks/useInvitations";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useTeam, useTeamMembers } from "@/hooks/useTeams";
import { useWorkspaces } from "@/hooks/useWorkspaces";
import { isApiError } from "@/lib/api-error";
import type { TeamResponse } from "@/lib/api/types";
import { avatarUrl } from "@/lib/avatar";
import { displayName } from "@/lib/display-name";
import { canInviteToTeam } from "@/lib/permissions";
import { ROUTES } from "@/lib/routes";
import { byRank } from "@/lib/team-roles";
import { NotFoundPage } from "@/pages/app/NotFoundPage";
import { TeamMembersTab } from "@/pages/app/teams/TeamMembersTab";
import { TeamRolesTab } from "@/pages/app/teams/TeamRolesTab";
import { TeamSettingsTab } from "@/pages/app/teams/TeamSettingsTab";
import { TeamWorkspacesTab } from "@/pages/app/teams/TeamWorkspacesTab";

const TEAM_TABS = ["members", "workspaces", "roles", "invitations", "settings"] as const;
type TeamTab = (typeof TEAM_TABS)[number];
const isTeamTab = (value: unknown): value is TeamTab => (TEAM_TABS as readonly unknown[]).includes(value);

/** Avatars in the header before "+N". */
const AVATARS_MAX = 6;

/** /teams/:slug: loads the team, then shows it, or says it is not found. */
export function TeamPage() {
  const { slug } = useParams();
  const { t } = useTranslation(["teams"]);
  const team = useTeam(slug);
  usePageTitle(team.data?.name ?? t("teams:list.title"));

  if (team.isPending) {
    return (
      <div className="flex flex-col gap-4 px-6 py-8">
        <Skeleton className="h-12 w-72" />
        <Skeleton className="h-9 w-full" />
      </div>
    );
  }
  if (team.isError) {
    if (isApiError(team.error) && team.error.status === 404) {
      return <NotFoundPage title={t("teams:notFound.title")} description={t("teams:notFound.description")} />;
    }
    return (
      <div className="px-6 py-8">
        <FormError error={team.error} />
      </div>
    );
  }
  return <TeamView team={team.data} />;
}

/**
 * The header, then the tabs. The open tab lives in `?tab=`, so a link, a reload or Back lands on
 * it; Members, the default, keeps a bare URL. A tab I may not see falls back to Members.
 */
function TeamView({ team }: Readonly<{ team: TeamResponse }>) {
  const { t, i18n } = useTranslation(["teams", "nav"]);
  const [params, setParams] = useSearchParams();
  const members = useTeamMembers(team.slug);
  const workspaces = useWorkspaces();

  const canInvite = canInviteToTeam(team.my_role);
  const visible = TEAM_TABS.filter((tab) => tab !== "invitations" || canInvite);
  const raw = params.get("tab");
  const tab: TeamTab = isTeamTab(raw) && visible.includes(raw) ? raw : "members";

  const workspaceCount = (workspaces.data ?? []).filter((ws) => ws.team?.id === team.id).length;
  const created = new Intl.DateTimeFormat(i18n.language, { month: "long", year: "numeric" }).format(new Date(team.created_at));
  const people = byRank(members.data ?? []);
  const shown = people.slice(0, AVATARS_MAX);
  const rest = people.length - shown.length;

  // Pushes an entry, so Back returns to the previous tab.
  const openTab = (value: unknown) => {
    if (isTeamTab(value)) setParams(value === "members" ? {} : { tab: value });
  };

  return (
    <div className="flex min-w-0 flex-col">
      <PageChrome
        crumbs={[
          { label: t("nav:sections.personalSpace") },
          { label: t("teams:list.title"), to: ROUTES.teams },
          { label: team.name },
        ]}
      />
      <div id="main">
        <div id="page" className="flex flex-col gap-4 px-6 py-8">
          <header className="flex flex-wrap items-center justify-between gap-4">
            <div className="flex min-w-0 items-center gap-3.5">
              <WsIcon name={team.name} color={team.color} size="lg" src={avatarUrl("teams", team.id, team.avatar_version, 256)} />
              <div className="min-w-0">
                <h1 className="truncate text-page font-semibold text-text">{team.name}</h1>
                <p className="text-text-muted">
                  <span>{t("teams:card.members", { count: team.member_count })}</span>
                  {" · "}
                  <span>{t("teams:card.workspaces", { count: workspaceCount })}</span>
                  {" · "}
                  <span>{t("teams:detail.created", { date: created })}</span>
                </p>
              </div>
            </div>
            {people.length > 0 ? (
              // One name for the whole group: a screen reader hears "7 members", not seven names.
              <AvatarGroup role="img" aria-label={t("teams:card.members", { count: team.member_count })}>
                {shown.map((m) => (
                  <UserAvatar key={m.user_id} name={displayName(m)} size="default" src={avatarUrl("users", m.user_id, m.avatar_version, 64) ?? undefined} />
                ))}
                {rest > 0 ? <AvatarGroupCount>+{rest}</AvatarGroupCount> : null}
              </AvatarGroup>
            ) : null}
          </header>

          <Tabs value={tab} onValueChange={openTab} className="flex flex-col gap-4">
            <TabsList variant="underline" aria-label={t("teams:detail.tabsLabel")} className="w-full justify-start border-b border-border">
              {visible.map((value) => (
                <TabsTab key={value} value={value} className="flex-none gap-1.5">
                  {t(`teams:detail.tabs.${value}`)}
                  {value === "members" ? <TabCount value={team.member_count} /> : null}
                  {value === "workspaces" ? <TabCount value={workspaceCount} /> : null}
                  {value === "invitations" ? <PendingInvitationsCount slug={team.slug} /> : null}
                </TabsTab>
              ))}
            </TabsList>

            {/* Base UI mounts only the open panel: each tab fetches only when shown. */}
            <TabsPanel value="members">
              <TeamMembersTab team={team} />
            </TabsPanel>
            <TabsPanel value="workspaces">
              <TeamWorkspacesTab team={team} />
            </TabsPanel>
            <TabsPanel value="roles">
              <TeamRolesTab team={team} />
            </TabsPanel>
            {canInvite ? <TabsPanel value="invitations">{/* TeamInvitationsTab, next commit */}</TabsPanel> : null}
            <TabsPanel value="settings">
              <TeamSettingsTab team={team} />
            </TabsPanel>
          </Tabs>
        </div>
      </div>
    </div>
  );
}

function TabCount({ value }: Readonly<{ value: number }>) {
  return <span className="text-xs font-normal tabular-nums text-text-subtle">{value}</span>;
}

/** Rendered only on the Invitations tab, which only Admins and the Captain see: no request for the others. */
function PendingInvitationsCount({ slug }: Readonly<{ slug: string }>) {
  const invitations = useTeamInvitations(slug);
  const count = invitations.data?.length ?? 0;
  return count > 0 ? <TabCount value={count} /> : null;
}
