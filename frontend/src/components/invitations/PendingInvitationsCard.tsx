import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { toast } from "sonner";
import { WsIcon } from "@/components/brand/WsIcon";
import { Button } from "@/components/ui/button";
import { RelativeTime } from "@/components/ui/relative-time";
import {
  useAcceptTeamInvitation,
  useAcceptWorkspaceInvitation,
  usePendingTeamInvitations,
  usePendingWorkspaceInvitations,
  useRejectTeamInvitation,
  useRejectWorkspaceInvitation,
} from "@/hooks/useInvitations";
import { apiErrorKey } from "@/lib/api-error";
import type { Membership } from "@/lib/api/types";
import { avatarUrl } from "@/lib/avatar";
import { ROUTES } from "@/lib/routes";
import { Skeleton } from "@/components/ui/skeleton";
import { Separator } from "../ui/separator";

/** Team invitations waiting for me, at the top of /teams. Renders nothing when there is none. */
export function PendingTeamInvitationsCard() {
  const { t } = useTranslation(["common"]);
  const navigate = useNavigate();
  const pending = usePendingTeamInvitations();
  const accept = useAcceptTeamInvitation();
  const reject = useRejectTeamInvitation();
  const onError = (error: Error) => toast.error(t(apiErrorKey(error)));

  return (
    <InvitationsBox count={pending.data?.length} loading={pending.isPending}>
      {pending.data?.map((inv) => (
        <InvitationRow
          key={inv.id}
          icon={<WsIcon name={inv.team_name} color={inv.team_color} size="md" src={avatarUrl("teams", inv.team_id, inv.team_avatar_version, 64)} />}
          name={inv.team_name}
          invitedBy={inv.invited_by_username}
          role={inv.default_role}
          expiresAt={inv.expires_at}
          busy={accept.isPending || reject.isPending}
          onReject={() => reject.mutate(inv.id, { onError })}
          onAccept={() => accept.mutate(inv.id, { onSuccess: (team) => void navigate(ROUTES.team(team.slug)), onError })}
        />
      ))}
    </InvitationsBox>
  );
}

// PendingWorkspaceInvitationsCard: same, with the workspace hooks, the workspace_* fields,
// inv.role and navigate(ROUTES.workspace(ws.slug)).
export function PendingWorkspaceInvitationsCard() {
  const { t } = useTranslation(["common"]);
  const navigate = useNavigate();
  const pending = usePendingWorkspaceInvitations();
  const accept = useAcceptWorkspaceInvitation();
  const reject = useRejectWorkspaceInvitation();
  const onError = (error: Error) => toast.error(t(apiErrorKey(error)));

  return (
    <InvitationsBox count={pending.data?.length} loading={pending.isPending}>
      {pending.data?.map((inv) => (
        <InvitationRow
          key={inv.id}
          icon={<WsIcon name={inv.workspace_name} color={inv.workspace_color} size="md" src={avatarUrl("workspaces", inv.workspace_id, inv.workspace_avatar_version, 64)} />}
          name={inv.workspace_name}
          invitedBy={inv.invited_by_username}
          role={inv.role}
          expiresAt={inv.expires_at}
          busy={accept.isPending || reject.isPending}
          onReject={() => reject.mutate(inv.id, { onError })}
          onAccept={() => accept.mutate(inv.id, { onSuccess: (ws) => void navigate(ROUTES.workspace(ws.slug)), onError })}
        />
      ))}
    </InvitationsBox>
  );
}

/** The grid cell of the mockup: always there, says so when nothing is pending. */
function InvitationsBox({ count, loading, children }: Readonly<{ count?: number; loading: boolean; children: ReactNode }>) {
  const { t } = useTranslation(["common"]);
  let content: ReactNode;
  if (loading) {
    content = <Skeleton className="mt-3 h-10" />;
  } else if (count) {
    content = <ul className="divide-y divide-border">{children}</ul>;
  } else {
    content = <p className="py-6 text-center text-sm text-text-muted">{t("common:invitations.none")}</p>;
  }

  return (
    <section aria-labelledby="pending-invitations-title" className="rounded-lg border border-border bg-bg-elev p-4">
      <h2 id="pending-invitations-title" className="mb-3 text-xs font-medium uppercase tracking-wide text-text-muted">
        {t("common:invitations.title")}
        {count ? ` · ${count}` : ""}
      </h2>
      <Separator />
      {content}
    </section>
  );
}

function InvitationRow({ icon, name, invitedBy, role, expiresAt, busy, onReject, onAccept }: Readonly<{
  icon: ReactNode;
  name: string;
  invitedBy?: string | null;
  role: Membership;
  expiresAt: string;
  busy: boolean;
  onReject: () => void;
  onAccept: () => void;
}>) {
  const { t } = useTranslation(["common"]);
  return (
    <li className="flex items-center gap-3 py-3">
      <span aria-hidden>{icon}</span>
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium text-text">{name}</div>
        <div className="truncate text-xs text-text-muted">
          {invitedBy ? <>{t("common:invitations.by", { who: `@${invitedBy}` })} · </> : null}
          {t(`common:roles.${role}`)} · {t("common:invitations.expires")} <RelativeTime timestamp={expiresAt} />
        </div>
      </div>
      <Button variant="ghost" size="sm" disabled={busy} onClick={onReject} aria-label={t("common:invitations.rejectNamed", { name })}>
        {t("common:invitations.reject")}
      </Button>
      <Button size="sm" disabled={busy} onClick={onAccept} aria-label={t("common:invitations.acceptNamed", { name })}>
        {t("common:invitations.accept")}
      </Button>
    </li>
  );
}
