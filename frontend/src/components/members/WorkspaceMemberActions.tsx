import { CheckIcon, MoreHorizontalIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useClearWorkspaceRoleOverride, useSetWorkspaceMemberRole } from "@/hooks/useWorkspaces";
import { apiErrorKey } from "@/lib/api-error";
import type { EffectiveMember, WorkspaceResponse, WorkspaceRole } from "@/lib/api/types";
import { displayName } from "@/lib/display-name";
import {
  canClearRoleOverride,
  canExcludeWorkspaceMember,
  canRemoveWorkspaceGuest,
  canSetWorkspaceMemberRole,
  type MemberWriteTarget,
} from "@/lib/permissions";

/** Captain never: it moves only with the team. A guest tops out at Member. */
const TEAM_MEMBER_ROLES: readonly WorkspaceRole[] = ["admin", "member", "viewer"];
const GUEST_ROLES: readonly WorkspaceRole[] = ["member", "viewer"];
const GROUP_LABEL = "px-2 pt-1.5 pb-1 text-[11px] font-medium tracking-wide text-text-subtle uppercase";

type Actor = { id: string; access: WorkspaceResponse["my_role"] };

/** What the menu offers, each entry checked against the rules the API applies. */
type Menu = {
  /** The roles to list: the current one, and those the viewer may give. */
  roles: WorkspaceRole[];
  changeable: boolean;
  /** The team role an override can go back to, or null. */
  resetTo: MemberWriteTarget["teamRole"];
  /** Taking access away: an exclusion for a team member, a removal for a guest. */
  removal: "exclude" | "removeGuest" | null;
};

function menuOf(workspace: WorkspaceResponse, actor: Actor, member: EffectiveMember, target: MemberWriteTarget): Menu {
  const roles = (target.isGuest ? GUEST_ROLES : TEAM_MEMBER_ROLES).filter(
    (role) => role === member.role || canSetWorkspaceMemberRole(workspace, actor, target, role),
  );
  const mayClear = member.role_source === "override" && canClearRoleOverride(workspace, actor, target);
  let removal: Menu["removal"] = null;
  if (target.isGuest && canRemoveWorkspaceGuest(workspace, actor, target)) removal = "removeGuest";
  else if (!target.isGuest && canExcludeWorkspaceMember(workspace, actor, target)) removal = "exclude";
  return {
    roles,
    changeable: roles.some((role) => role !== member.role),
    resetTo: mayClear ? target.teamRole : null,
    removal,
  };
}

type Props = Readonly<{
  workspace: WorkspaceResponse;
  actor: Actor;
  member: EffectiveMember;
  target: MemberWriteTarget;
  onExclude: (member: EffectiveMember) => void;
  onRemoveGuest: (member: EffectiveMember) => void;
}>;

/**
 * What the viewer may do to one member. A role is picked at once (it can be picked back);
 * exclusion and removal ask first, through the page. Picking the team role back on an override
 * clears the override rather than copying it.
 */
export function WorkspaceMemberActions({ workspace, actor, member, target, onExclude, onRemoveGuest }: Props) {
  const { t } = useTranslation(["workspaces", "common", "errors"]);
  const setRole = useSetWorkspaceMemberRole();
  const clearOverride = useClearWorkspaceRoleOverride();
  const menu = menuOf(workspace, actor, member, target);
  if (!menu.changeable && !menu.resetTo && !menu.removal) return null;

  const onError = (error: Error) => toast.error(t(apiErrorKey(error)));
  const name = displayName(member);
  const who = { slug: workspace.slug, userId: member.user_id };

  const reset = () =>
    clearOverride.mutate(who, { onSuccess: () => toast.success(t("workspaces:members.roleReset", { name })), onError });
  const pick = (role: WorkspaceRole) => {
    if (role === menu.resetTo) return reset();
    setRole.mutate(
      { ...who, role },
      { onSuccess: () => toast.success(t("workspaces:members.roleSet", { name, role: t(`common:roles.${role}`) })), onError },
    );
  };
  const remove = () => (menu.removal === "exclude" ? onExclude : onRemoveGuest)(member);

  return (
    <DropdownMenu modal={false}>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon-sm" aria-label={t("workspaces:members.actionsFor", { name })}>
          <MoreHorizontalIcon />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-56">
        {menu.changeable ? (
          <RoleItems
            label={t(target.isGuest ? "workspaces:members.guestRole" : "workspaces:members.workspaceRole")}
            roles={menu.roles}
            current={member.role}
            onPick={pick}
          />
        ) : null}
        {menu.resetTo ? (
          <>
            {menu.changeable ? <DropdownMenuSeparator /> : null}
            <DropdownMenuItem onSelect={reset}>
              {t("workspaces:members.resetRole", { role: t(`common:roles.${menu.resetTo}`) })}
            </DropdownMenuItem>
          </>
        ) : null}
        {menu.removal ? (
          <>
            {menu.changeable || menu.resetTo ? <DropdownMenuSeparator /> : null}
            <DropdownMenuItem variant="destructive" onSelect={remove}>
              {t(`workspaces:members.${menu.removal}`)}
            </DropdownMenuItem>
          </>
        ) : null}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** The role choices, the current one checked and inert. */
function RoleItems({
  label,
  roles,
  current,
  onPick,
}: Readonly<{ label: string; roles: readonly WorkspaceRole[]; current: EffectiveMember["role"]; onPick: (role: WorkspaceRole) => void }>) {
  const { t } = useTranslation(["common"]);
  return (
    <>
      <div className={GROUP_LABEL}>{label}</div>
      {roles.map((role) =>
        role === current ? (
          <DropdownMenuItem key={role} disabled>
            {t(`common:roles.${role}`)}
            <CheckIcon aria-hidden className="ml-auto size-4" />
          </DropdownMenuItem>
        ) : (
          <DropdownMenuItem key={role} onSelect={() => onPick(role)}>
            {t(`common:roles.${role}`)}
          </DropdownMenuItem>
        ),
      )}
    </>
  );
}
