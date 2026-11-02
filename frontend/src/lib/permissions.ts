// src/lib/permissions.ts
import type {
  Membership,
  TeamResponse,
  TeamRole,
  WorkspaceRole,
  WorkspaceSummary,
} from "@/lib/api/types";

// What the interface may offer, mirrored from the rules the backend applies.
// For greying out or hiding an action, and nothing else: the backend stays
// the sole judge and checks every request on its own. A helper that drifts
// from it only shows a button the server then refuses, or hides one it would
// accept, never grants anything. Each helper names the Rust function it
// copies; change both together.

/** `TeamRole::level` (domain/src/dto/team.rs). */
const LEVEL: Record<TeamRole, number> = { viewer: 0, member: 1, admin: 2, captain: 3 };

function atLeast(role: TeamRole, min: TeamRole): boolean {
  return LEVEL[role] >= LEVEL[min];
}

/**
 * `Membership::satisfies` (domain/src/dto/workspace.rs): whether an access
 * grants at least `min`. The owner of a solo workspace satisfies every role.
 */
export function satisfies(access: Membership, min: TeamRole): boolean {
  return access === "owner" || atLeast(access, min);
}

// ---- Teams ------------------------------------------------------------------

/** `routes::teams::update_team`: `require_role(Admin)`. */
export function canEditTeam(myRole: TeamRole): boolean {
  return atLeast(myRole, "admin");
}

/** `routes::teams::delete_team`: `require_role(Captain)`. */
export function canDeleteTeam(myRole: TeamRole): boolean {
  return myRole === "captain";
}

/**
 * `routes::teams::{list,create,revoke}_invitation`: `require_role(Admin)`.
 * No invitation grants Captain, whoever asks (`team_service::create_invitation`).
 */
export function canInviteToTeam(myRole: TeamRole): boolean {
  return atLeast(myRole, "admin");
}

/**
 * `routes::teams::change_member_role`: Admin at least, then
 * `team_service::can_manage_role`, then `team_service::change_member_role`,
 * which names nobody Captain and demotes no Captain: that role moves only by
 * transfer.
 */
export function canChangeTeamMemberRole(actor: TeamRole, target: TeamRole, next: TeamRole): boolean {
  return atLeast(actor, "admin") && target !== "captain" && next !== "captain";
}

/**
 * `team_service::can_remove_member`: oneself, or anyone but the Captain for
 * an Admin or the Captain; then `team_service::remove_member`, which removes
 * no Captain, not even by their own hand.
 */
export function canRemoveTeamMember(
  actor: { id: string; role: TeamRole },
  target: { id: string; role: TeamRole },
): boolean {
  if (target.role === "captain") return false;
  return actor.id === target.id || atLeast(actor.role, "admin");
}

/** Leaving is removing oneself (`canRemoveTeamMember`): anyone but the Captain. */
export function canLeaveTeam(myRole: TeamRole): boolean {
  return myRole !== "captain";
}

/** `routes::teams::transfer_captaincy`: `require_role(Captain)`. */
export function canTransferCaptaincy(myRole: TeamRole): boolean {
  return myRole === "captain";
}

/**
 * `workspace_service::create_workspace`, `min_role_to_create_workspace`:
 * Admin on the team, or Member when the team's `members_can_create_workspaces`
 * is on. Never a Viewer. A solo workspace is anyone's to create.
 */
export function canCreateTeamWorkspace(
  team: Pick<TeamResponse, "my_role" | "members_can_create_workspaces">,
): boolean {
  return atLeast(team.my_role, team.members_can_create_workspaces ? "member" : "admin");
}

// ---- Workspaces -------------------------------------------------------------

type WorkspaceState = Pick<WorkspaceSummary, "mode" | "archived">;

/**
 * `routes::workspaces::update_workspace` (`WorkspaceMember<role::Admin>`),
 * then `workspace_service::update_workspace`, which refuses an archived
 * workspace (`ensure_writable`).
 */
export function canEditWorkspace(workspace: Pick<WorkspaceState, "archived">, access: Membership): boolean {
  return !workspace.archived && satisfies(access, "admin");
}

/**
 * `routes::workspaces::{archive,unarchive}_workspace`:
 * `WorkspaceMember<role::Captain>`, the owner of a solo workspace included.
 */
export function canArchiveWorkspace(access: Membership): boolean {
  return satisfies(access, "captain");
}

/** `routes::workspaces::delete_workspace`: `WorkspaceMember<role::Captain>`, as archiving. */
export function canDeleteWorkspace(access: Membership): boolean {
  return satisfies(access, "captain");
}

/**
 * `workspace_invitation_service::ensure_can_invite`: a team workspace, not
 * archived, and `require_inviter`: on the team, and at least Member here. A
 * guest never invites, whatever their role. `onTeam` comes from the caller's
 * teams, or from their own `role_source` in the members list.
 */
export function canInviteToWorkspace(workspace: WorkspaceState, access: Membership, onTeam: boolean): boolean {
  return workspace.mode === "team" && !workspace.archived && onTeam && satisfies(access, "member");
}

/** `workspace_invitation_service::cancel_invitation`: its author, or an Admin here. */
export function canCancelWorkspaceInvitation(
  access: Membership,
  me: string,
  invitedBy: string | null | undefined,
): boolean {
  return invitedBy === me || satisfies(access, "admin");
}

/**
 * `routes::workspace_members::list_exclusions`: `WorkspaceMember<role::Admin>`,
 * on a team workspace (`workspace_service::list_exclusions`).
 */
export function canReadExclusions(workspace: Pick<WorkspaceState, "mode">, access: Membership): boolean {
  return workspace.mode === "team" && satisfies(access, "admin");
}

/** `workspace_service::leave_workspace`: a team workspace, and anyone but the Captain. */
export function canLeaveWorkspace(workspace: Pick<WorkspaceState, "mode">, access: Membership): boolean {
  return workspace.mode === "team" && access !== "captain";
}

/** A change to one person's access, as the member routes name it (`workspace_service::MemberWrite`). */
export type MemberWrite =
  | { kind: "set_role"; role: WorkspaceRole }
  | { kind: "clear_override" }
  | { kind: "exclude" }
  | { kind: "lift_exclusion" }
  | { kind: "remove_guest" };

/** What a member write needs to know about the person it targets. */
export type MemberWriteTarget = {
  userId: string;
  /**
   * Their role on the team, `null` for someone off it. The members list
   * gives it when their `role_source` is `team`; otherwise the team's own
   * members list does.
   */
  teamRole: TeamRole | null;
  /** Their access here now, or `excluded`. */
  access: Membership | "excluded";
  /** Admitted with no place on the team (`EffectiveMember.is_guest`). */
  isGuest: boolean;
};

type Access = Membership | "excluded" | "none";

/** `belowAdmin` in workspace_service.rs. No access at all counts as below. */
function belowAdmin(access: Access): boolean {
  if (access === "none" || access === "excluded") return true;
  if (access === "owner") return false;
  return LEVEL[access] < LEVEL.admin;
}

/**
 * `MemberState::after`: the target's access once `write` applies, or `null`
 * when it does not apply to them (exclusions and overrides are for team
 * members, removal for guests).
 */
function accessAfter(write: MemberWrite, target: MemberWriteTarget): Access | null {
  if (target.teamRole === null) {
    if (write.kind === "set_role") return write.role;
    if (write.kind === "remove_guest") return "none";
    return null;
  }
  switch (write.kind) {
    case "set_role":
      return write.role;
    case "exclude":
      return "excluded";
    case "clear_override":
      return target.access === "excluded" ? target.access : target.teamRole;
    case "lift_exclusion":
      return target.access === "excluded" ? target.teamRole : target.access;
    case "remove_guest":
      return null;
  }
}

/**
 * `workspace_service::write_member`, refusal for refusal: a solo workspace;
 * an archived one, unless the write only takes access away; someone neither
 * on the team nor a guest; oneself; the Captain; a write that does not apply
 * to that kind of member; then the caller's rank: the Captain may act, an
 * Admin only when the target is below Admin both before and after; and last
 * a guest's ceiling, Member (`set_member_override`).
 */
export function canWriteWorkspaceMember(
  workspace: WorkspaceState,
  actor: { id: string; access: Membership },
  target: MemberWriteTarget,
  write: MemberWrite,
): boolean {
  if (workspace.mode !== "team") return false;
  const onlyTakesAccessAway = write.kind === "exclude" || write.kind === "remove_guest";
  if (workspace.archived && !onlyTakesAccessAway) return false;
  if (target.teamRole === null && !target.isGuest) return false;
  if (target.userId === actor.id) return false;
  if (target.teamRole === "captain") return false;

  const after = accessAfter(write, target);
  if (after === null) return false;

  const mayAct =
    actor.access === "captain" ||
    (actor.access === "admin" && belowAdmin(target.access) && belowAdmin(after));
  if (!mayAct) return false;

  return !(write.kind === "set_role" && target.isGuest && LEVEL[write.role] > LEVEL.member);
}

export function canSetWorkspaceMemberRole(
  workspace: WorkspaceState,
  actor: { id: string; access: Membership },
  target: MemberWriteTarget,
  role: WorkspaceRole,
): boolean {
  return canWriteWorkspaceMember(workspace, actor, target, { kind: "set_role", role });
}

export function canClearRoleOverride(
  workspace: WorkspaceState,
  actor: { id: string; access: Membership },
  target: MemberWriteTarget,
): boolean {
  return canWriteWorkspaceMember(workspace, actor, target, { kind: "clear_override" });
}

export function canExcludeWorkspaceMember(
  workspace: WorkspaceState,
  actor: { id: string; access: Membership },
  target: MemberWriteTarget,
): boolean {
  return canWriteWorkspaceMember(workspace, actor, target, { kind: "exclude" });
}

/**
 * Lifting an exclusion: `write_member` with `MemberWrite::LiftExclusion`. It
 * gives the team role back, so an Admin may lift only the exclusion of a
 * member whose team role is below Admin, and an archived workspace refuses it.
 */
export function canLiftExclusion(
  workspace: WorkspaceState,
  actor: { id: string; access: Membership },
  target: MemberWriteTarget,
): boolean {
  return canWriteWorkspaceMember(workspace, actor, target, { kind: "lift_exclusion" });
}

export function canRemoveWorkspaceGuest(
  workspace: WorkspaceState,
  actor: { id: string; access: Membership },
  target: MemberWriteTarget,
): boolean {
  return canWriteWorkspaceMember(workspace, actor, target, { kind: "remove_guest" });
}
