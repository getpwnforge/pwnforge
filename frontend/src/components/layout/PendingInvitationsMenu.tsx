import { Bell } from "lucide-react";
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

type InvitationItemProps = Readonly<{
  name: string;
  color: WsColor;
  src: string | null;
  kind: string;
  createdAt: string;
}>;

function InvitationItem({ name, color, src, kind, createdAt }: InvitationItemProps) {
  return (
    <li className="flex items-center gap-2.5 px-3 py-2.5">
      <WsIcon name={name} color={color} size="md" src={src} />
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium text-text">{name}</div>
        <div className="truncate text-xs text-muted-foreground">
          {kind} · <RelativeTime timestamp={createdAt} />
        </div>
      </div>
    </li>
  );
}

export function PendingInvitationsMenu() {
  const { t } = useTranslation(["nav"]);
  const [open, setOpen] = useState(false);
  const count = usePendingInvitationCount();
  const teams = usePendingTeamInvitations();
  const workspaces = usePendingWorkspaceInvitations();

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="icon-sm" aria-label={t("nav:invitations.open", { count })}>
          <Bell />
          {count > 0 ? <span className="absolute top-0 right-0 flex items-center justify-center w-5 h-5 text-xs font-bold text-white bg-red-500 rounded-full">{count}</span> : null}
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
                name={inv.workspace_name}
                color={inv.workspace_color}
                src={avatarUrl("workspaces", inv.workspace_id, inv.workspace_avatar_version, 64)}
                kind={t("nav:invitations.workspace")}
                createdAt={inv.created_at}
              />
            ))}
          </ul>
        )}
        <Separator />
        {/* The sidebar stays mounted across navigations: without this the
            popover would remain open over the page it just led to. */}
        <Link
          to={ROUTES.teams}
          onClick={() => setOpen(false)}
          className="block px-3 py-2 text-center text-sm text-muted-foreground transition-colors outline-none hover:bg-surface-2 hover:text-foreground focus-visible:bg-surface-2 focus-visible:text-foreground"
        >
          {t("nav:invitations.seeAll")}
        </Link>
      </PopoverContent>
    </Popover>
  );
}
