import type { TeamMemberResponse, TeamResponse, TeamRole } from "@/lib/api/types";
import {
  canArchiveWorkspace,
  canChangeTeamMemberRole,
  canCreateTeamWorkspace,
  canDeleteTeam,
  canEditTeam,
  canInviteToTeam,
  canLeaveTeam,
  canRemoveTeamMember,
  canTransferCaptaincy,
  satisfies,
} from "@/lib/permissions";

/** Highest first, as the roles card and the matrix list them. */
export const TEAM_ROLES = ["captain", "admin", "member", "viewer"] as const satisfies readonly TeamRole[];

const RANK: Record<TeamRole, number> = { captain: 0, admin: 1, member: 2, viewer: 3 };

/** Members by role, highest first, then by arrival. */
export function byRank(members: readonly TeamMemberResponse[]): TeamMemberResponse[] {
  return [...members].sort((a, b) => RANK[a.role] - RANK[b.role] || a.joined_at.localeCompare(b.joined_at));
}

export function countByRole(members: readonly TeamMemberResponse[]): Record<TeamRole, number> {
  const counts: Record<TeamRole, number> = { captain: 0, admin: 0, member: 0, viewer: 0 };
  for (const m of members) counts[m.role] += 1;
  return counts;
}

export type PermissionRow = Readonly<{ key: string; allowed: (role: TeamRole, team: TeamResponse) => boolean }>;
export type PermissionGroup = Readonly<{ key: string; rows: readonly PermissionRow[] }>;

/**
 * The matrix of the Roles tab, read from lib/permissions.ts and not written by hand: it can never
 * promise a right the API refuses. Each row asks the rule with a typical case.
 */
export const PERMISSION_GROUPS: readonly PermissionGroup[] = [
  {
    key: "team",
    rows: [
      { key: "editTeam", allowed: canEditTeam },
      { key: "transfer", allowed: canTransferCaptaincy },
      { key: "deleteTeam", allowed: canDeleteTeam },
    ],
  },
  {
    key: "members",
    rows: [
      { key: "invite", allowed: canInviteToTeam },
      // Acting on a Viewer: an Admin may, on roles below their own.
      { key: "changeRoles", allowed: (role) => canChangeTeamMemberRole(role, "viewer", "member") },
      { key: "remove", allowed: (role) => canRemoveTeamMember({ id: "actor", role }, { id: "target", role: "member" }) },
      { key: "leave", allowed: canLeaveTeam },
    ],
  },
  {
    key: "workspaces",
    rows: [
      { key: "view", allowed: () => true },
      { key: "createWorkspace", allowed: (role, team) => canCreateTeamWorkspace({ ...team, my_role: role }) },
      // canEditWorkspace on a workspace that is not archived.
      { key: "editWorkspace", allowed: (role) => satisfies(role, "admin") },
      { key: "archiveWorkspace", allowed: canArchiveWorkspace },
      // canInviteToWorkspace on a live team workspace, for someone on the team.
      { key: "inviteGuest", allowed: (role) => satisfies(role, "member") },
    ],
  },
];
