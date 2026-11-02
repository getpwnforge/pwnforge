import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { Membership, RoleSource } from "@/lib/api/types";

const VARIANT: Record<Membership, "accent" | "info" | "default" | "muted"> = {
  owner: "accent",
  captain: "accent",
  admin: "info",
  member: "default",
  viewer: "muted",
};

export function RoleBadge({ role, source }: Readonly<{ role: Membership; source?: RoleSource }>) {
  const { t } = useTranslation();
  const badge = <Badge variant={VARIANT[role]}>{t(`common:roles.${role}`)}</Badge>;
  if (!source || source === "team" || source === "owner") return badge;
  return (
    <Tooltip>
      <TooltipTrigger asChild>{badge}</TooltipTrigger>
      <TooltipContent>{t(`common:roleSources.${source}`)}</TooltipContent>
    </Tooltip>
  );
}
