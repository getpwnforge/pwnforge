import { ArrowUpRightIcon, ExternalLinkIcon, FlagIcon, PlusIcon, UserPlusIcon } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";

import { WsIcon } from "@/components/brand/WsIcon";
import { InviteToWorkspaceDialog } from "@/components/invitations/InviteToWorkspaceDialog";
import { PageChrome } from "@/components/layout/PageChrome";
import { AvatarGroup, AvatarGroupCount } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { UserAvatar } from "@/components/ui/user-avatar";
import { useCurrentWorkspace } from "@/hooks/useCurrentWorkspace";
import { useTeams } from "@/hooks/useTeams";
import { useWorkspaceMembers } from "@/hooks/useWorkspaces";
import type { WorkspaceResponse } from "@/lib/api/types";
import { avatarUrl } from "@/lib/avatar";
import { displayName } from "@/lib/display-name";
import { canInviteToWorkspace } from "@/lib/permissions";
import { EXTERNAL, ROUTES } from "@/lib/routes";

/**
 * NOTE : Temporary function to determine if the user can create a challenge. This will be replaced with a proper permission check in the future.
 * Returns always true for now, but will be updated to check the user's role and the workspace's status.
 */
function canCreateChallenge(role: WorkspaceResponse["my_role"]): boolean {
  return !!role;
}

/** A team as my teams list it. */
type Team = NonNullable<ReturnType<typeof useTeams>["data"]>[number];

/** The host to show for a link; the API only stores parsed http(s) URLs. */
function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

/**
 * The index of /w/:slug, for now: who the workspace is, then an empty state. The KPIs, the
 * activity feed and the categories come with the challenges and the real-time phase.
 */
export function WorkspaceDashboardPage() {
  const { t } = useTranslation(["workspaces", "nav"]);
  const workspace = useCurrentWorkspace();
  const teams = useTeams();
  // WorkspaceResponse carries only team_id: the name and slug come from my teams. A guest who
  // is not in the team does not find it there.
  const team = workspace.team_id ? teams.data?.find((candidate) => candidate.id === workspace.team_id) : undefined;
  const canCreate = !workspace.archived && canCreateChallenge(workspace.my_role);
  const canInvite = canInviteToWorkspace(workspace, workspace.my_role, !!team);
  let emptyKey = "workspaces:dashboard.emptyTextReader";
  if (canCreate) emptyKey = "workspaces:dashboard.emptyText";
  else if (!workspace.team_id) emptyKey = "workspaces:dashboard.emptyTextSolo";

  return (
    <>
      <PageChrome crumbs={[{ label: t("workspaces:list.title"), to: ROUTES.workspaces }, { label: workspace.name }]} />
      <div className="flex flex-col gap-6 p-6">
        <header className="flex flex-wrap items-center gap-3.5">
          <WsIcon
            name={workspace.name}
            color={workspace.color}
            size="lg"
            src={avatarUrl("workspaces", workspace.id, workspace.avatar_version, 256)}
          />
          <div className="min-w-0 flex-1">
            <h1 className="truncate text-page font-semibold tracking-tight text-text">{workspace.name}</h1>
            <Facts workspace={workspace} team={team} />
          </div>
          <Members slug={workspace.slug} />
          {canInvite ? (
            <InviteToWorkspaceDialog
              workspace={workspace}
              trigger={
                <Button variant="ghost" size="sm">
                  <UserPlusIcon aria-hidden />
                  {t("workspaces:members.invite.open")}
                </Button>
              }
            />
          ) : null}
        </header>

        {workspace.description ? <p className="max-w-prose text-sm text-text-muted">{workspace.description}</p> : null}

        <Empty className="h-full">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <FlagIcon aria-hidden />
            </EmptyMedia>
            <EmptyTitle>{t("workspaces:dashboard.emptyTitle")}</EmptyTitle>
            <EmptyDescription>{t(emptyKey)}</EmptyDescription>
          </EmptyHeader>
          {canCreate ? (
            <EmptyContent>
              <Button asChild variant="primary">
                <Link to={ROUTES.challengeCreate(workspace.slug)}>
                  <PlusIcon aria-hidden />
                  {t("workspaces:dashboard.createChallenge")}
                </Link>
              </Button>
            </EmptyContent>
          ) : null}
          <Button asChild variant="link" size="sm" className="text-text-muted">
            <a href={EXTERNAL.challengesDocs} target="_blank" rel="noreferrer">
              {t("workspaces:dashboard.learnMore")}
              <ArrowUpRightIcon aria-hidden />
            </a>
          </Button>
        </Empty>
      </div>
    </>
  );
}

/** Avatars shown before the "+N". */
const AVATARS_MAX = 5;

/** Who has access, guests included; the full list is on the Members page. */
function Members({ slug }: Readonly<{ slug: string }>) {
  const { t } = useTranslation(["workspaces"]);
  const members = useWorkspaceMembers(slug);
  const people = members.data ?? [];
  if (people.length === 0) return null;
  const shown = people.slice(0, AVATARS_MAX);
  const rest = people.length - shown.length;

  return (
    // One name for the whole group: a screen reader hears "7 members", not seven names.
    <AvatarGroup role="img" aria-label={t("workspaces:dashboard.members", { count: people.length })}>
      {shown.map((m) => (
        <UserAvatar
          key={m.user_id}
          name={displayName(m)}
          size="default"
          src={avatarUrl("users", m.user_id, m.avatar_version, 64) ?? undefined}
        />
      ))}
      {rest > 0 ? <AvatarGroupCount>+{rest}</AvatarGroupCount> : null}
    </AvatarGroup>
  );
}

/** Team (or solo), platform, dates and event link, on one line under the name. */
function Facts({ workspace, team }: Readonly<{ workspace: WorkspaceResponse; team: Team | undefined }>) {
  const { t, i18n } = useTranslation(["workspaces", "nav"]);
  const dates = new Intl.DateTimeFormat(i18n.language, { day: "numeric", month: "short", year: "numeric" });
  const start = workspace.starts_at ? dates.format(new Date(workspace.starts_at)) : null;
  const end = workspace.ends_at ? dates.format(new Date(workspace.ends_at)) : null;

  const parts: { key: string; node: ReactNode }[] = [];
  if (!workspace.team_id) parts.push({ key: "solo", node: t("workspaces:dashboard.solo") });
  else if (team) {
    parts.push({
      key: "team",
      node: (
        <Link to={ROUTES.team(team.slug)} className="text-text hover:underline">
          {team.name}
        </Link>
      ),
    });
  } else parts.push({ key: "team", node: t("workspaces:dashboard.team") });
  if (workspace.platform) parts.push({ key: "platform", node: t(`nav:platforms.${workspace.platform}`) });
  if (start && end) parts.push({ key: "dates", node: t("workspaces:dashboard.dates", { start, end }) });
  else if (start) parts.push({ key: "dates", node: t("workspaces:dashboard.from", { date: start }) });
  else if (end) parts.push({ key: "dates", node: t("workspaces:dashboard.until", { date: end }) });
  if (workspace.event_url) {
    parts.push({
      key: "event",
      node: (
        <a
          href={workspace.event_url}
          target="_blank"
          rel="noopener noreferrer"
          className="inline-flex items-center gap-1 text-text hover:underline"
        >
          {hostOf(workspace.event_url)}
          <ExternalLinkIcon aria-hidden className="size-3.5" />
          <span className="sr-only">{t("workspaces:dashboard.eventLinkHint")}</span>
        </a>
      ),
    });
  }

  return (
    <p className="mt-1 flex flex-wrap items-center gap-x-1.5 text-[13px] text-text-muted">
      {parts.map((part, index) => (
        <span key={part.key} className="inline-flex items-center gap-1.5">
          {index > 0 ? <span aria-hidden>·</span> : null}
          {part.node}
        </span>
      ))}
    </p>
  );
}
