import { Bell, ChevronRightIcon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { WsIcon } from "@/components/brand/WsIcon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverHeader, PopoverTitle, PopoverTrigger } from "@/components/ui/popover";
import { RelativeTime } from "@/components/ui/relative-time";
import { Separator } from "@/components/ui/separator";
import { usePendingInvitationCount } from "@/hooks/usePendingInvitationCount";
import { usePendingTeamInvitations, usePendingWorkspaceInvitations } from "@/hooks/useInvitations";
import { avatarUrl } from "@/lib/avatar";
import { ROUTES } from "@/lib/routes";
import type { WsColor } from "@/lib/ws-color";

/** One received invitation; opens the page where it can be accepted. */
function InvitationItem({ to, onNavigate, name, color, src, kind, createdAt }: Readonly<{
  to: string;
  onNavigate: () => void;
  name: string;
  color: WsColor;
  src: string | null;
  kind: string;
  createdAt: string;
}>) {
  return (
    <li>
      <Link
        to={to}
        onClick={onNavigate}
        className="flex items-center gap-2.5 px-3 py-2.5 outline-none transition-colors hover:bg-surface-2 focus-visible:bg-surface-2"
      >
        <WsIcon name={name} color={color} size="sm" src={src} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium text-text">{name}</div>
          <div className="text-xs text-text-muted">
            {kind} · <RelativeTime timestamp={createdAt} />
          </div>
        </div>
        <ChevronRightIcon aria-hidden className="size-4 shrink-0 text-text-subtle" />
      </Link>
    </li>
  );
}

export function PendingInvitationsMenu() {
  const { t } = useTranslation(["nav"]);
  const [open, setOpen] = useState(false);
  const count = usePendingInvitationCount();
  const teams = usePendingTeamInvitations();
  const workspaces = usePendingWorkspaceInvitations();
  const close = () => setOpen(false);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="icon-sm" className="relative" aria-label={t("nav:invitations.open", { count })}>
          <Bell />
          {count > 0 ? (
            <span
              aria-hidden
              className="absolute -top-0.5 -right-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-primary px-1 text-[10px] leading-none font-semibold tabular-nums text-primary-foreground ring-2 ring-bg"
            >
              {count > 9 ? "9+" : count}
            </span>
          ) : null}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-80 gap-0 overflow-hidden p-0">
        <PopoverHeader className="flex-row items-center justify-between px-3 py-2.5">
          <PopoverTitle>{t("nav:notifications.title")}</PopoverTitle>
          {count > 0 ? <Badge variant="accent">{count}</Badge> : null}
        </PopoverHeader>
        <Separator />
        {count === 0 ? (
          <p className="px-3 py-6 text-center text-sm text-muted-foreground">{t("nav:invitations.none")}</p>
        ) : (
          <ul className="max-h-80 divide-y divide-border overflow-y-auto">
            {teams.data?.map((inv) => (
              <InvitationItem
                key={inv.id}
                to={ROUTES.teams}
                onNavigate={close}
                name={inv.team_name}
                color={inv.team_color}
                src={avatarUrl("teams", inv.team_id, inv.team_avatar_version, 64)}
                kind={t("nav:invitations.team")}
                createdAt={inv.created_at}
              />
            ))}
            {workspaces.data?.map((inv) => (
              <InvitationItem
                key={inv.id}
                to={ROUTES.workspaces}
                onNavigate={close}
                name={inv.workspace_name}
                color={inv.workspace_color}
                src={avatarUrl("workspaces", inv.workspace_id, inv.workspace_avatar_version, 64)}
                kind={t("nav:invitations.workspace")}
                createdAt={inv.created_at}
              />
            ))}
          </ul>
        )}
      </PopoverContent>
    </Popover>
  );
}
