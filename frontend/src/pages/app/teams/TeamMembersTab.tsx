import { MoreHorizontalIcon, PlusIcon, SearchIcon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "react-router";
import { toast } from "sonner";
import { WsIcon } from "@/components/brand/WsIcon";
import { FormError } from "@/components/common/FormError";
import { RoleBadge } from "@/components/members/RoleBadge";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group";
import { RelativeTime } from "@/components/ui/relative-time";
import { Skeleton } from "@/components/ui/skeleton";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { UserAvatar } from "@/components/ui/user-avatar";
import { WorkspaceStatusBadge } from "@/components/workspaces/WorkspaceStatusBadge";
import { useAuth } from "@/hooks/useAuth";
import { useChangeTeamMemberRole, useRemoveTeamMember, useTeamMembers } from "@/hooks/useTeams";
import { useWorkspaces } from "@/hooks/useWorkspaces";
import { apiErrorKey } from "@/lib/api-error";
import type { TeamMemberResponse, TeamResponse, TeamRole } from "@/lib/api/types";
import { avatarUrl } from "@/lib/avatar";
import { displayName } from "@/lib/display-name";
import { canChangeTeamMemberRole, canInviteToTeam, canLeaveTeam, canRemoveTeamMember } from "@/lib/permissions";
import { ROUTES } from "@/lib/routes";
import { byRank, countByRole, TEAM_ROLES } from "@/lib/team-roles";

/** Captain is never given here: it moves only by transfer, in the settings. */
const ASSIGNABLE: readonly TeamRole[] = ["admin", "member", "viewer"];
/** Workspaces listed in the side card before "See all". */
const LINKED_MAX = 5;
const CARD = "rounded-lg border border-border bg-bg-elev";

type Pending = { kind: "remove" | "leave"; member: TeamMemberResponse } | null;

function matches(m: TeamMemberResponse, query: string): boolean {
  const q = query.trim().toLowerCase();
  return !q || [m.username, m.first_name, m.last_name].some((field) => field.toLowerCase().includes(q));
}

export function TeamMembersTab({ team }: Readonly<{ team: TeamResponse }>) {
  const members = useTeamMembers(team.slug);

  return (
    <div className="grid items-start gap-4 lg:grid-cols-[1fr_320px]">
      <MembersTable team={team} members={members.data} loading={members.isPending} error={members.error} />
      <aside className="flex flex-col gap-4">
        <RolesCard team={team} members={members.data ?? []} />
        <LinkedWorkspacesCard team={team} />
      </aside>
    </div>
  );
}

function MembersTable({ team, members, loading, error }: Readonly<{
  team: TeamResponse;
  members: readonly TeamMemberResponse[] | undefined;
  loading: boolean;
  error: unknown;
}>) {
  const { t } = useTranslation(["teams", "common"]);
  const navigate = useNavigate();
  const { user } = useAuth();
  const changeRole = useChangeTeamMemberRole();
  const remove = useRemoveTeamMember();
  const [query, setQuery] = useState("");
  const [pending, setPending] = useState<Pending>(null);
  const onError = (e: Error) => toast.error(t(apiErrorKey(e)));

  const list = byRank(members ?? []);
  const byId = new Map(list.map((m) => [m.user_id, m]));
  const shown = list.filter((m) => matches(m, query));

  const confirm = () => {
    if (!pending) return;
    const leaving = pending.kind === "leave";
    remove.mutate(
      { slug: team.slug, userId: pending.member.user_id },
      {
        onSuccess: () => {
          setPending(null);
          // Leaving empties the team from the cache: go before its 404 shows.
          if (leaving) void navigate(ROUTES.teams);
        },
        onError,
      },
    );
  };

  return (
    <div className={CARD}>
      <div className="flex flex-wrap items-center gap-2 border-b border-border px-3.5 py-2.5">
        <InputGroup className="w-full max-w-60">
          <InputGroupAddon>
            <SearchIcon aria-hidden />
          </InputGroupAddon>
          <InputGroupInput
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("teams:members.filter")}
            aria-label={t("teams:members.filter")}
          />
        </InputGroup>
        <div className="flex-1" />
        {canInviteToTeam(team.my_role) ? (
          <Button asChild variant="secondary" size="sm">
            <Link to={ROUTES.teamInvitations(team.slug)}>
              <PlusIcon aria-hidden />
              {t("teams:members.invite")}
            </Link>
          </Button>
        ) : null}
      </div>

      {error ? (
        <div className="p-4">
          <FormError error={error} />
        </div>
      ) : null}

      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{t("teams:members.columns.member")}</TableHead>
            <TableHead>{t("teams:members.columns.role")}</TableHead>
            <TableHead>{t("teams:members.columns.joined")}</TableHead>
            <TableHead>{t("teams:members.columns.invitedBy")}</TableHead>
            <TableHead className="w-10">
              <span className="sr-only">{t("teams:members.columns.actions")}</span>
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {loading
            ? [0, 1, 2].map((i) => (
                <TableRow key={i}>
                  <TableCell colSpan={5}>
                    <Skeleton className="h-8" />
                  </TableCell>
                </TableRow>
              ))
            : null}
          {shown.map((m) => {
            const me = m.user_id === user?.id;
            const inviter = m.invited_by ? byId.get(m.invited_by) : undefined;
            const roles = ASSIGNABLE.filter((next) => next !== m.role && canChangeTeamMemberRole(team.my_role, m.role, next));
            const canRemove = !me && !!user && canRemoveTeamMember({ id: user.id, role: team.my_role }, { id: m.user_id, role: m.role });
            const canLeave = me && canLeaveTeam(team.my_role);

            return (
              <TableRow key={m.user_id}>
                <TableCell>
                  <div className="flex items-center gap-2.5">
                    <UserAvatar name={displayName(m)} size="default" src={avatarUrl("users", m.user_id, m.avatar_version, 64) ?? undefined} />
                    <div className="min-w-0">
                      <div className="truncate text-sm font-medium text-text">{displayName(m)}</div>
                      <div className="truncate font-mono text-xs text-text-muted">@{m.username}</div>
                    </div>
                  </div>
                </TableCell>
                <TableCell>
                  <RoleBadge role={m.role} />
                </TableCell>
                <TableCell className="text-xs text-text-muted">
                  <RelativeTime timestamp={m.joined_at} />
                </TableCell>
                <TableCell className="font-mono text-xs text-text-muted">{inviter ? `@${inviter.username}` : "—"}</TableCell>
                <TableCell>
                  {roles.length > 0 || canRemove || canLeave ? (
                    <DropdownMenu modal={false}>
                      <DropdownMenuTrigger asChild>
                        <Button variant="ghost" size="icon-sm" aria-label={t("teams:members.actionsFor", { name: displayName(m) })}>
                          <MoreHorizontalIcon />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        {roles.map((next) => (
                          <DropdownMenuItem
                            key={next}
                            onSelect={() => changeRole.mutate({ slug: team.slug, userId: m.user_id, role: next }, { onError })}
                          >
                            {t("teams:members.setRole", { role: t(`common:roles.${next}`) })}
                          </DropdownMenuItem>
                        ))}
                        {roles.length > 0 && (canRemove || canLeave) ? <DropdownMenuSeparator /> : null}
                        {canRemove ? (
                          <DropdownMenuItem variant="destructive" onSelect={() => setPending({ kind: "remove", member: m })}>
                            {t("teams:members.remove")}
                          </DropdownMenuItem>
                        ) : null}
                        {canLeave ? (
                          <DropdownMenuItem variant="destructive" onSelect={() => setPending({ kind: "leave", member: m })}>
                            {t("teams:members.leave")}
                          </DropdownMenuItem>
                        ) : null}
                      </DropdownMenuContent>
                    </DropdownMenu>
                  ) : null}
                </TableCell>
              </TableRow>
            );
          })}
          {!loading && !error && shown.length === 0 ? (
            <TableRow>
              <TableCell colSpan={5} className="py-8 text-center text-sm text-text-muted">
                {t("teams:members.noMatch")}
              </TableCell>
            </TableRow>
          ) : null}
        </TableBody>
      </Table>

      <AlertDialog open={pending !== null} onOpenChange={(open) => { if (!open) setPending(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {pending?.kind === "leave"
                ? t("teams:members.leaveTitle", { team: team.name })
                : t("teams:members.removeTitle", { name: pending ? displayName(pending.member) : "" })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t(pending?.kind === "leave" ? "teams:members.leaveText" : "teams:members.removeText")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common:actions.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              variant="danger"
              disabled={remove.isPending}
              onClick={(event) => {
                // Stays open until the server answers; closes on success.
                event.preventDefault();
                confirm();
              }}
            >
              {t(pending?.kind === "leave" ? "teams:members.leave" : "teams:members.remove")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

/** Each role, what it allows in a line, and how many members hold it. */
function RolesCard({ team, members }: Readonly<{ team: TeamResponse; members: readonly TeamMemberResponse[] }>) {
  const { t } = useTranslation(["teams"]);
  const counts = countByRole(members);
  return (
    <section aria-labelledby="roles-card" className={`${CARD} p-4`}>
      <h2 id="roles-card" className="mb-2 text-sm font-semibold text-text">
        {t("teams:roles.title")}
      </h2>
      <ul className="divide-y divide-border">
        {TEAM_ROLES.map((role) => (
          <li key={role} className="flex items-center gap-2.5 py-2">
            <span className="w-20 shrink-0">
              <RoleBadge role={role} />
            </span>
            <span className="min-w-0 flex-1 text-xs text-text-muted">{t(`teams:roles.short.${role}`)}</span>
            <span className="text-xs tabular-nums text-text-muted">{counts[role]}</span>
          </li>
        ))}
      </ul>
      <Button asChild variant="ghost" size="sm" className="mt-2 w-full">
        <Link to={ROUTES.teamRoles(team.slug)}>{t("teams:roles.seeDetail")}</Link>
      </Button>
    </section>
  );
}

/** The team's workspaces I can open, at a glance; the Workspaces tab has the full list. */
function LinkedWorkspacesCard({ team }: Readonly<{ team: TeamResponse }>) {
  const { t } = useTranslation(["teams"]);
  const workspaces = useWorkspaces();
  const linked = (workspaces.data ?? []).filter((ws) => ws.team?.id === team.id);

  return (
    <section aria-labelledby="linked-card" className={`${CARD} p-4`}>
      <h2 id="linked-card" className="mb-2 flex items-baseline gap-1.5 text-sm font-semibold text-text">
        {t("teams:linked.title")}
        <span className="text-xs font-normal tabular-nums text-text-muted">{linked.length}</span>
      </h2>
      {linked.length === 0 ? (
        <p className="py-3 text-xs text-text-muted">{t("teams:workspaces.empty")}</p>
      ) : (
        <ul className="divide-y divide-border">
          {linked.slice(0, LINKED_MAX).map((ws) => (
            <li key={ws.id} className="flex items-center gap-2 py-2">
              <WsIcon name={ws.name} color={ws.color} size="sm" src={avatarUrl("workspaces", ws.id, ws.avatar_version, 64)} />
              <Link to={ROUTES.workspace(ws.slug)} className="min-w-0 flex-1 truncate text-sm font-medium text-text hover:underline">
                {ws.name}
              </Link>
              <WorkspaceStatusBadge workspace={ws} />
            </li>
          ))}
        </ul>
      )}
      {linked.length > LINKED_MAX ? (
        <Button asChild variant="ghost" size="sm" className="mt-2 w-full">
          <Link to={ROUTES.teamWorkspaces(team.slug)}>{t("teams:linked.seeAll", { count: linked.length })}</Link>
        </Button>
      ) : null}
    </section>
  );
}
