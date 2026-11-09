import { AtSignIcon, LinkIcon, MailIcon } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { FormError } from "@/components/common/FormError";
import { RoleBadge } from "@/components/members/RoleBadge";
import { Button } from "@/components/ui/button";
import { RelativeTime } from "@/components/ui/relative-time";
import { Skeleton } from "@/components/ui/skeleton";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Tabs, TabsList, TabsTab } from "@/components/ui/tabs";
import { useAuth } from "@/hooks/useAuth";
import { useRevokeTeamInvitation, useTeamInvitations } from "@/hooks/useInvitations";
import { apiErrorKey } from "@/lib/api-error";
import type { TeamInvitationResponse, TeamResponse } from "@/lib/api/types";

type Filter = "all" | "mine";
const isFilter = (value: unknown): value is Filter => value === "all" || value === "mine";

/** Under two days left, the expiry stands out. */
const SOON_MS = 2 * 86_400_000;

/**
 * The team's pending invitations, as the Captain and the Admins see them: all of them, or only
 * those they sent (`?invited_by=`). A link shows its uses, never its address.
 */
export function TeamInvitationsTable({ team }: Readonly<{ team: TeamResponse }>) {
  const { t } = useTranslation(["teams"]);
  const { user } = useAuth();
  const [filter, setFilter] = useState<Filter>("all");
  const mine = filter === "mine" && !!user;
  const invitations = useTeamInvitations(team.slug, mine ? { invited_by: user.id } : undefined);
  const list = invitations.data ?? [];

  return (
    <section aria-labelledby="team-invitations-title" className="rounded-lg border border-border bg-bg-elev">
      <div className="flex flex-wrap items-center gap-2 border-b border-border px-3.5 py-2.5">
        <h2 id="team-invitations-title" className="text-sm font-semibold text-text">
          {t("teams:invitations.table.title")}
          {invitations.isSuccess ? <span className="ml-1.5 font-normal tabular-nums text-text-muted">{list.length}</span> : null}
        </h2>
        <div className="flex-1" />
        <Tabs value={filter} onValueChange={(value: unknown) => { if (isFilter(value)) setFilter(value); }}>
          <TabsList aria-label={t("teams:invitations.table.filterLabel")}>
            <TabsTab value="all">{t("teams:invitations.table.all")}</TabsTab>
            <TabsTab value="mine">{t("teams:invitations.table.mine")}</TabsTab>
          </TabsList>
        </Tabs>
      </div>

      {invitations.isError ? (
        <div className="p-4">
          <FormError error={invitations.error} />
        </div>
      ) : null}

      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{t("teams:invitations.table.columns.target")}</TableHead>
            <TableHead>{t("teams:invitations.table.columns.role")}</TableHead>
            <TableHead>{t("teams:invitations.table.columns.invitedBy")}</TableHead>
            <TableHead>{t("teams:invitations.table.columns.created")}</TableHead>
            <TableHead>{t("teams:invitations.table.columns.expires")}</TableHead>
            <TableHead className="w-24">
              <span className="sr-only">{t("teams:invitations.table.columns.actions")}</span>
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {invitations.isPending
            ? [0, 1, 2].map((i) => (
                <TableRow key={i}>
                  <TableCell colSpan={6}>
                    <Skeleton className="h-8" />
                  </TableCell>
                </TableRow>
              ))
            : null}
          {list.map((invitation) => (
            <InvitationRow key={invitation.id} team={team} invitation={invitation} myId={user?.id} />
          ))}
          {invitations.isSuccess && list.length === 0 ? (
            <TableRow>
              <TableCell colSpan={6} className="py-8 text-center text-sm text-text-muted">
                {t(mine ? "teams:invitations.table.emptyMine" : "teams:invitations.table.empty")}
              </TableCell>
            </TableRow>
          ) : null}
        </TableBody>
      </Table>

      <p className="border-t border-border px-3.5 py-2.5 text-xs text-text-muted">{t("teams:invitations.table.footnote")}</p>
    </section>
  );
}

function InvitationRow({ team, invitation, myId }: Readonly<{
  team: TeamResponse;
  invitation: TeamInvitationResponse;
  myId: string | undefined;
}>) {
  const { t } = useTranslation(["teams", "common"]);
  const revoke = useRevokeTeamInvitation();
  const [now, setNow] = useState<number | null>(null);
  useEffect(() => {
    const updateNow = () => setNow(Date.now());
    updateNow();
    const interval = setInterval(updateNow, 60_000);
    return () => clearInterval(interval);
  }, []);
  const soon = now !== null && new Date(invitation.expires_at).getTime() - now < SOON_MS;
  const name = targetName(invitation, t("teams:invitations.table.link"));

  const confirm = () =>
    revoke.mutate(
      { slug: team.slug, invitationId: invitation.id },
      {
        onSuccess: () => toast.success(t("teams:invitations.table.revoked")),
        onError: (error) => toast.error(t(apiErrorKey(error))),
      },
    );

  return (
    <TableRow>
      <TableCell>
        <Target invitation={invitation} name={name} />
      </TableCell>
      <TableCell>
        <RoleBadge role={invitation.default_role} />
      </TableCell>
      <TableCell className="font-mono text-xs text-text-muted">
        {invitation.invited_by_username ? `@${invitation.invited_by_username}` : "—"}
        {invitation.invited_by && invitation.invited_by === myId ? (
          <span className="ml-1 font-sans">{t("teams:invitations.table.me")}</span>
        ) : null}
      </TableCell>
      <TableCell className="text-xs text-text-muted">
        <RelativeTime timestamp={invitation.created_at} />
      </TableCell>
      <TableCell className={soon ? "text-xs font-medium text-text" : "text-xs text-text-muted"}>
        <RelativeTime timestamp={invitation.expires_at} />
      </TableCell>
      <TableCell className="text-right">
        <ConfirmDialog
          trigger={
            <Button variant="ghost" size="sm" className="text-danger" aria-label={t("teams:invitations.table.revokeNamed", { name })}>
              {t("teams:invitations.table.revoke")}
            </Button>
          }
          title={t("teams:invitations.table.revokeTitle")}
          description={t(
            invitation.target === "link" ? "teams:invitations.table.revokeLinkText" : "teams:invitations.table.revokePersonText",
            { name },
          )}
          confirmLabel={t("teams:invitations.table.revoke")}
          danger
          pending={revoke.isPending}
          onConfirm={confirm}
        />
      </TableCell>
    </TableRow>
  );
}

/** How the row is named, in the table and in the revoke dialog. */
function targetName(invitation: TeamInvitationResponse, linkLabel: string): string {
  if (invitation.target === "link") return linkLabel;
  if (invitation.target === "email") return invitation.invited_email ?? "—";
  return `@${invitation.invited_username ?? "—"}`;
}

function Target({ invitation, name }: Readonly<{ invitation: TeamInvitationResponse; name: string }>) {
  const { t } = useTranslation(["teams"]);
  const isLink = invitation.target === "link";
  let icon: ReactNode = <AtSignIcon aria-hidden className="size-4" />;
  let detail = t("teams:invitations.table.byUsername");
  if (isLink) {
    icon = <LinkIcon aria-hidden className="size-4" />;
    detail = invitation.max_uses
      ? t("teams:invitations.table.uses", { count: invitation.uses, max: invitation.max_uses })
      : t("teams:invitations.table.usesUnlimited", { count: invitation.uses });
  } else if (invitation.target === "email") {
    icon = <MailIcon aria-hidden className="size-4" />;
    detail = t("teams:invitations.table.byEmail");
  }

  return (
    <div className="flex items-center gap-2.5">
      <span
        className={
          isLink
            ? "grid size-8 shrink-0 place-items-center rounded-md bg-ember-soft text-primary"
            : "grid size-8 shrink-0 place-items-center rounded-md border border-border bg-surface text-text-muted"
        }
      >
        {icon}
      </span>
      <div className="min-w-0">
        <div className={isLink ? "truncate text-sm font-medium text-text" : "truncate font-mono text-[13px] font-medium text-text"}>
          {name}
        </div>
        <div className="text-xs tabular-nums text-text-muted">{detail}</div>
      </div>
    </div>
  );
}
