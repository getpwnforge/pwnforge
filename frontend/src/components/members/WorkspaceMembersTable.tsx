import { SearchIcon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { FormError } from "@/components/common/FormError";
import { RoleBadge } from "@/components/members/RoleBadge";
import { WorkspaceMemberActions } from "@/components/members/WorkspaceMemberActions";
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
import { Badge } from "@/components/ui/badge";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group";
import { RelativeTime } from "@/components/ui/relative-time";
import { Skeleton } from "@/components/ui/skeleton";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Tabs, TabsList, TabsTab } from "@/components/ui/tabs";
import { UserAvatar } from "@/components/ui/user-avatar";
import { useExcludeWorkspaceMember, useRemoveWorkspaceGuest, useWorkspaceMembers } from "@/hooks/useWorkspaces";
import { apiErrorKey } from "@/lib/api-error";
import type { EffectiveMember, TeamRole, WorkspaceResponse } from "@/lib/api/types";
import { avatarUrl } from "@/lib/avatar";
import { displayName } from "@/lib/display-name";
import { memberTarget } from "@/lib/member-targets";

const FILTERS = ["all", "team", "guests"] as const;
type Filter = (typeof FILTERS)[number];
const isFilter = (value: unknown): value is Filter => FILTERS.includes(value as Filter);

/** Highest role first, guests after the team, then by name. */
const RANK: Record<EffectiveMember["role"], number> = { owner: 0, captain: 0, admin: 1, member: 2, viewer: 3 };
function byRank(a: EffectiveMember, b: EffectiveMember): number {
  return Number(a.is_guest) - Number(b.is_guest) || RANK[a.role] - RANK[b.role] || displayName(a).localeCompare(displayName(b));
}

function matches(member: EffectiveMember, filter: Filter, query: string): boolean {
  if (filter === "team" && member.is_guest) return false;
  if (filter === "guests" && !member.is_guest) return false;
  const q = query.trim().toLowerCase();
  return !q || [member.handle, member.first_name, member.last_name].some((field) => field.toLowerCase().includes(q));
}

type Pending = { kind: "exclude" | "removeGuest"; member: EffectiveMember } | null;

type Props = Readonly<{
  workspace: WorkspaceResponse;
  actor: { id: string; access: WorkspaceResponse["my_role"] };
  teamRoles: ReadonlyMap<string, TeamRole>;
}>;

/**
 * Who has access, with where their role comes from: the team, an override here (and the team
 * role it replaces), or an invitation (and who sent it). Each row offers what the viewer may do.
 */
export function WorkspaceMembersTable({ workspace, actor, teamRoles }: Props) {
  const { t } = useTranslation(["workspaces", "common", "errors"]);
  const members = useWorkspaceMembers(workspace.slug);
  const exclude = useExcludeWorkspaceMember();
  const removeGuest = useRemoveWorkspaceGuest();
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [pending, setPending] = useState<Pending>(null);

  const list = members.data ?? [];
  const byId = new Map(list.map((m) => [m.user_id, m]));
  const shown = list.filter((m) => matches(m, filter, query)).sort(byRank);
  const busy = exclude.isPending || removeGuest.isPending;

  const confirm = () => {
    if (!pending) return;
    const name = displayName(pending.member);
    const target = { slug: workspace.slug, userId: pending.member.user_id };
    const done = {
      onSuccess: () => {
        setPending(null);
        toast.success(t(pending.kind === "exclude" ? "workspaces:members.excluded" : "workspaces:members.guestRemoved", { name }));
      },
      onError: (error: Error) => toast.error(t(apiErrorKey(error))),
    };
    if (pending.kind === "exclude") exclude.mutate(target, done);
    else removeGuest.mutate(target, done);
  };

  const via = (member: EffectiveMember) => {
    if (member.role_source === "override") {
      const teamRole = teamRoles.get(member.user_id);
      return teamRole
        ? t("workspaces:members.via.override", { role: t(`common:roles.${teamRole}`) })
        : t("workspaces:members.via.overrideUnknown");
    }
    if (member.role_source === "guest") {
      const inviter = member.invited_by ? byId.get(member.invited_by) : undefined;
      return (
        <span className="inline-flex flex-wrap items-center gap-1.5">
          <Badge variant="warning">{t("workspaces:members.via.guest")}</Badge>
          {inviter ? t("workspaces:members.via.invitedBy", { name: displayName(inviter) }) : null}
        </span>
      );
    }
    return t(`workspaces:members.via.${member.role_source}`);
  };

  return (
    <section aria-labelledby="ws-members-title" className="rounded-lg border border-border bg-bg-elev">
      <div className="flex flex-wrap items-center gap-2 border-b border-border px-3.5 py-2.5">
        <h2 id="ws-members-title" className="text-sm font-semibold text-text">
          {t("workspaces:members.accessTitle")}
          {members.isSuccess ? <span className="ml-1.5 font-normal tabular-nums text-text-muted">{list.length}</span> : null}
        </h2>
        <div className="flex-1" />
        <Tabs value={filter} onValueChange={(value: unknown) => { if (isFilter(value)) setFilter(value); }}>
          <TabsList aria-label={t("workspaces:members.filterLabel")}>
            {FILTERS.map((value) => (
              <TabsTab key={value} value={value}>
                {t(`workspaces:members.filters.${value}`)}
              </TabsTab>
            ))}
          </TabsList>
        </Tabs>
        <InputGroup className="w-full max-w-56">
          <InputGroupAddon>
            <SearchIcon aria-hidden />
          </InputGroupAddon>
          <InputGroupInput
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("workspaces:members.search")}
            aria-label={t("workspaces:members.search")}
          />
        </InputGroup>
      </div>

      {members.isError ? (
        <div className="p-4">
          <FormError error={members.error} />
        </div>
      ) : null}

      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{t("workspaces:members.columns.member")}</TableHead>
            <TableHead>{t("workspaces:members.columns.role")}</TableHead>
            <TableHead>{t("workspaces:members.columns.access")}</TableHead>
            <TableHead>{t("workspaces:members.columns.joined")}</TableHead>
            <TableHead className="w-10">
              <span className="sr-only">{t("workspaces:members.columns.actions")}</span>
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {members.isPending
            ? [0, 1, 2].map((i) => (
                <TableRow key={i}>
                  <TableCell colSpan={5}>
                    <Skeleton className="h-8" />
                  </TableCell>
                </TableRow>
              ))
            : null}
          {shown.map((m) => {
            const target = memberTarget(m, teamRoles);
            return (
              <TableRow key={m.user_id}>
                <TableCell>
                  <div className="flex items-center gap-2.5">
                    <UserAvatar name={displayName(m)} size="default" src={avatarUrl("users", m.user_id, m.avatar_version, 64) ?? undefined} />
                    <div className="min-w-0">
                      <div className="flex items-center gap-1.5 truncate text-sm font-medium text-text">
                        {displayName(m)}
                        {m.user_id === actor.id ? (
                          <span className="rounded border border-border px-1 text-nav font-medium text-text-muted">
                            {t("workspaces:members.you")}
                          </span>
                        ) : null}
                      </div>
                      <div className="truncate font-mono text-xs text-text-muted">@{m.handle}</div>
                    </div>
                  </div>
                </TableCell>
                <TableCell>
                  <RoleBadge role={m.role} />
                </TableCell>
                <TableCell className="text-xs text-text-muted">{via(m)}</TableCell>
                <TableCell className="text-xs text-text-muted">
                  <RelativeTime timestamp={m.joined_at} />
                </TableCell>
                <TableCell>
                  {target ? (
                    <WorkspaceMemberActions
                      workspace={workspace}
                      actor={actor}
                      member={m}
                      target={target}
                      onExclude={(member) => setPending({ kind: "exclude", member })}
                      onRemoveGuest={(member) => setPending({ kind: "removeGuest", member })}
                    />
                  ) : null}
                </TableCell>
              </TableRow>
            );
          })}
          {members.isSuccess && shown.length === 0 ? (
            <TableRow>
              <TableCell colSpan={5} className="py-8 text-center text-sm text-text-muted">
                {t("workspaces:members.noMatch")}
              </TableCell>
            </TableRow>
          ) : null}
        </TableBody>
      </Table>

      <AlertDialog open={pending !== null} onOpenChange={(open) => { if (!open && !busy) setPending(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t(pending?.kind === "removeGuest" ? "workspaces:members.removeGuestTitle" : "workspaces:members.excludeTitle", {
                name: pending ? displayName(pending.member) : "",
              })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t(pending?.kind === "removeGuest" ? "workspaces:members.removeGuestText" : "workspaces:members.excludeText")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>{t("common:actions.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              variant="danger"
              disabled={busy}
              onClick={(event) => {
                // Stays open until the API answers: a refusal shows as a toast with the dialog still up.
                event.preventDefault();
                confirm();
              }}
            >
              {t(pending?.kind === "removeGuest" ? "workspaces:members.removeGuest" : "workspaces:members.exclude")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </section>
  );
}
