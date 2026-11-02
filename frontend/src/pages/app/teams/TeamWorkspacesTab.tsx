import { FilterIcon, PlusIcon, SearchIcon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { WsIcon } from "@/components/brand/WsIcon";
import { FormError } from "@/components/common/FormError";
import { RoleBadge } from "@/components/members/RoleBadge";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group";
import { Skeleton } from "@/components/ui/skeleton";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { CreateWorkspaceDialog } from "@/components/workspaces/CreateWorkspaceDialog";
import { WorkspaceStatusBadge } from "@/components/workspaces/WorkspaceStatusBadge";
import { useWorkspaces } from "@/hooks/useWorkspaces";
import type { TeamResponse, WorkspaceSummary } from "@/lib/api/types";
import { avatarUrl } from "@/lib/avatar";
import { canCreateTeamWorkspace } from "@/lib/permissions";
import { ROUTES } from "@/lib/routes";
import { workspaceStatus, type WorkspaceStatus } from "@/lib/workspace-status";

const STATUSES = ["live", "upcoming", "ended", "permanent", "archived"] as const satisfies readonly WorkspaceStatus[];
type StatusFilter = WorkspaceStatus | "all";
const isStatusFilter = (value: string): value is StatusFilter => value === "all" || (STATUSES as readonly string[]).includes(value);

function keep(ws: WorkspaceSummary, query: string, status: StatusFilter, now: Date): boolean {
  if (status !== "all" && workspaceStatus(ws, now) !== status) return false;
  const q = query.trim().toLowerCase();
  return !q || ws.name.toLowerCase().includes(q) || ws.slug.includes(q);
}

/** The team's workspaces that I can open, as a table, from the list already loaded for the sidebar. */
export function TeamWorkspacesTab({ team }: Readonly<{ team: TeamResponse }>) {
  const { t, i18n } = useTranslation(["teams", "workspaces", "nav"]);
  const workspaces = useWorkspaces();
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState<StatusFilter>("all");
  const [creating, setCreating] = useState(false);
  const canCreate = canCreateTeamWorkspace(team);

  const now = new Date();
  const linked = (workspaces.data ?? []).filter((ws) => ws.team?.id === team.id);
  const shown = linked.filter((ws) => keep(ws, query, status, now));
  const dates = new Intl.DateTimeFormat(i18n.language, { day: "numeric", month: "short", year: "numeric" });

  return (
    <div className="rounded-lg border border-border bg-bg-elev">
      <div className="flex flex-wrap items-center gap-2 border-b border-border px-3.5 py-2.5">
        <InputGroup className="w-full max-w-60">
          <InputGroupAddon>
            <SearchIcon aria-hidden />
          </InputGroupAddon>
          <InputGroupInput
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("teams:workspaces.filter")}
            aria-label={t("teams:workspaces.filter")}
          />
        </InputGroup>
        <div className="flex-1" />
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="sm">
              <FilterIcon aria-hidden />
              {t("teams:workspaces.status")}
              {status === "all" ? null : <Badge variant="muted">{t(`workspaces:status.${status}`)}</Badge>}
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuLabel>{t("teams:workspaces.status")}</DropdownMenuLabel>
            <DropdownMenuRadioGroup value={status} onValueChange={(value) => { if (isStatusFilter(value)) setStatus(value); }}>
              <DropdownMenuRadioItem value="all">{t("teams:workspaces.allStatuses")}</DropdownMenuRadioItem>
              {STATUSES.map((s) => (
                <DropdownMenuRadioItem key={s} value={s}>
                  {t(`workspaces:status.${s}`)}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
        {canCreate ? (
          <Button variant="secondary" size="sm" onClick={() => setCreating(true)}>
            <PlusIcon aria-hidden />
            {t("teams:workspaces.new")}
          </Button>
        ) : null}
      </div>

      {workspaces.isError ? (
        <div className="p-4">
          <FormError error={workspaces.error} />
        </div>
      ) : null}

      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{t("teams:workspaces.columns.workspace")}</TableHead>
            <TableHead>{t("teams:workspaces.columns.status")}</TableHead>
            <TableHead>{t("teams:workspaces.columns.type")}</TableHead>
            <TableHead>{t("teams:workspaces.columns.myRole")}</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {workspaces.isPending
            ? [0, 1, 2].map((i) => (
                <TableRow key={i}>
                  <TableCell colSpan={4}>
                    <Skeleton className="h-8" />
                  </TableCell>
                </TableRow>
              ))
            : null}
          {shown.map((ws) => (
            <TableRow key={ws.id} className={ws.archived ? "opacity-70" : undefined}>
              <TableCell>
                <div className="flex items-center gap-2.5">
                  <WsIcon name={ws.name} color={ws.color} size="md" src={avatarUrl("workspaces", ws.id, ws.avatar_version, 64)} />
                  <div className="min-w-0">
                    <Link to={ROUTES.workspace(ws.slug)} className="block truncate text-sm font-medium text-text hover:underline">
                      {ws.name}
                    </Link>
                    {ws.platform ? <div className="text-xs text-text-muted">{t(`nav:platforms.${ws.platform}`)}</div> : null}
                  </div>
                </div>
              </TableCell>
              <TableCell>
                <WorkspaceStatusBadge workspace={ws} />
              </TableCell>
              <TableCell className="text-xs text-text-muted">
                {t(`workspaces:create.type.${ws.type}`)}
                {ws.starts_at && ws.ends_at ? (
                  <div>
                    {dates.format(new Date(ws.starts_at))} → {dates.format(new Date(ws.ends_at))}
                  </div>
                ) : null}
              </TableCell>
              <TableCell>
                <RoleBadge role={ws.my_role} />
              </TableCell>
            </TableRow>
          ))}
          {workspaces.isSuccess && shown.length === 0 ? (
            <TableRow>
              <TableCell colSpan={4} className="py-8 text-center text-sm text-text-muted">
                {t(linked.length === 0 ? "teams:workspaces.empty" : "teams:workspaces.noMatch")}
              </TableCell>
            </TableRow>
          ) : null}
        </TableBody>
      </Table>

      {canCreate ? <CreateWorkspaceDialog open={creating} onOpenChange={setCreating} defaultTeamId={team.id} /> : null}
    </div>
  );
}
