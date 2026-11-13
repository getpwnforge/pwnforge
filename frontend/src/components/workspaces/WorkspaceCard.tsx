import { MoreHorizontalIcon, StarIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "react-router";
import { toast } from "sonner";
import { WsIcon } from "@/components/brand/WsIcon";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { useArchiveWorkspace, useLeaveWorkspace, useSetFavorite, useUnarchiveWorkspace } from "@/hooks/useWorkspaces";
import { apiErrorKey } from "@/lib/api-error";
import type { WorkspaceSummary } from "@/lib/api/types";
import { avatarUrl } from "@/lib/avatar";
import { canArchiveWorkspace, canLeaveWorkspace } from "@/lib/permissions";
import { ROUTES } from "@/lib/routes";
import { Badge } from "@/components/ui/badge";
import { Progress, ProgressLabel, ProgressValue } from "@/components/ui/progress";
import { useState } from "react";
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from "@/components/ui/alert-dialog";

export function WorkspaceCard({ workspace: ws }: Readonly<{ workspace: WorkspaceSummary }>) {
  const { t } = useTranslation(["workspaces", "nav"]);
  const favorite = useSetFavorite();
  const nbChallenges: number = 100; // NOTE: get from API
  const nbSolved = 55; // NOTE: get from API
  const toggleFavorite = () =>
    favorite.mutate({ slug: ws.slug, favorite: !ws.favorite }, { onError: (error) => toast.error(t(apiErrorKey(error))) });

  // The link's ::after stretches it over the whole card, so the card is clickable without
  // nesting the buttons inside an <a>: their clicks (and the portaled menu's) never reach it.
  return (
    <Card className={`relative h-40 hover:bg-bg has-[a:focus-visible]:ring-2 has-[a:focus-visible]:ring-ring ${ws.archived ? "opacity-50" : ""}`}>
      <CardHeader className="flex justify-between">
          <div className="flex items-center gap-2">
            <WsIcon name={ws.name} color={ws.color} src={avatarUrl("workspaces", ws.id, ws.avatar_version, 64)} size="lg" />
            <div className="flex flex-col">
              <div className="flex items-center gap-3">
                <Link to={ROUTES.workspace(ws.slug)} className="font-bold text-base outline-none after:absolute after:inset-0">
                  {ws.name}
                </Link>
                <Button variant="ghost" size="icon-sm" aria-pressed={ws.favorite} onClick={toggleFavorite} className="relative z-10 -my-1"
                  aria-label={t(ws.favorite ? "workspaces:card.unfavorite" : "workspaces:card.favorite")}><StarIcon className={`size-4 text-ember ${ws.favorite ? "fill-current" : ""}`} /></Button>
              </div>
              <div>
                {ws.platform ? <span className="text-xs text-muted-foreground">{t(`nav:platforms.${ws.platform}`)} </span> : null}
                <Badge variant={ws.team ? "accent" : "default"}>{ws.team ? t("nav:workspaceModes.team") : t("nav:workspaceModes.solo")}</Badge>
              </div>
            </div>
          </div>
          <CardMenu workspace={ws} />
      </CardHeader>
      <CardContent className="flex flex-col justify-between gap-2">
        {nbChallenges === 0 ? (
          <p className="text-sm text-muted-foreground">{t("workspaces:card.noChallenges")}</p>
        ) : (
            <Progress value={(nbSolved / nbChallenges) * 100} >
              <ProgressLabel className="text-sm text-muted-foreground">{t("workspaces:card.solvedRatio", { solved: nbSolved, total: nbChallenges })}</ProgressLabel>
              <ProgressValue />
            </Progress>
        )}
      </CardContent>
    </Card>
  );
}

/** The card's ⋯ menu: each entry shown only when the role allows it. */
function CardMenu({ workspace: ws }: Readonly<{ workspace: WorkspaceSummary }>) {
  const { t } = useTranslation(["workspaces"]);
  const navigate = useNavigate();
  const archive = useArchiveWorkspace();
  const unarchive = useUnarchiveWorkspace();
  const leave = useLeaveWorkspace();
  const [leaving, setLeaving] = useState(false);
  const onError = (error: Error) => toast.error(t(apiErrorKey(error)));

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon-sm" aria-label={t("workspaces:card.menu", { name: ws.name })} className="relative z-10 items-end"><MoreHorizontalIcon /></Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onSelect={() => navigate(ROUTES.settings(ws.slug))}>{t("workspaces:card.settings")}</DropdownMenuItem>
          {canArchiveWorkspace(ws.my_role) ? (
            <DropdownMenuItem onSelect={() => (ws.archived ? unarchive : archive).mutate(ws.slug, { onError })}>
              {t(ws.archived ? "workspaces:card.unarchive" : "workspaces:card.archive")}
            </DropdownMenuItem>
          ) : null}
          {canLeaveWorkspace(ws, ws.my_role) ? (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuItem variant="destructive" onSelect={() => setLeaving(true)}>
                  {t("workspaces:settings.leaveAction")}
                </DropdownMenuItem>
              </>
            ) : null}
        </DropdownMenuContent>
      </DropdownMenu>

      <AlertDialog open={leaving} onOpenChange={setLeaving}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("workspaces:settings.leaveTitle")}</AlertDialogTitle>
            <AlertDialogDescription>{t("workspaces:settings.leaveDescription")}</AlertDialogDescription>
            <AlertDialogDescription className="text-sm font-bold">
              {t("workspaces:settings.leaveConfirm")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common:actions.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              variant="danger"
              disabled={leave.isPending}
              onClick={(event) => {
                // Stays open until the server answers; closes on success.
                event.preventDefault();
                leave.mutate(ws.slug, { onSuccess: () => setLeaving(false), onError });
              }}
            >
              {t("workspaces:settings.leaveAction")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
