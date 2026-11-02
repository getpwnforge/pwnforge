import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import type { WorkspaceSummary } from "@/lib/api/types";
import { workspaceStatus, type WorkspaceStatus } from "@/lib/workspace-status";

const VARIANT: Record<WorkspaceStatus, "default" | "muted" | "success" | "info" | "warning"> = {
  live: "success",
  upcoming: "info",
  ended: "muted",
  permanent: "default",
  archived: "warning",
};

export function WorkspaceStatusBadge({ workspace }: Readonly<{ workspace: Pick<WorkspaceSummary, "archived" | "starts_at" | "ends_at"> }>) {
  const { t } = useTranslation(["workspaces"]);
  const status = workspaceStatus(workspace, new Date());
  return <Badge variant={VARIANT[status]}>{t(`workspaces:status.${status}`)}</Badge>;
}
