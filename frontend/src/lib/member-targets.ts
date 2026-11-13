import type { EffectiveMember, ExcludedMember, TeamMemberResponse, TeamRole } from "@/lib/api/types";
import type { MemberWriteTarget } from "@/lib/permissions";

/** Each team member's role on the team, by user id: the members list only says it for `team`. */
export function teamRolesById(teamMembers: readonly TeamMemberResponse[] | undefined): ReadonlyMap<string, TeamRole> {
  return new Map((teamMembers ?? []).map((m) => [m.user_id, m.role]));
}

/**
 * What the permission rules need about someone on the members list, or `null` while it cannot
 * be told yet: an override hides the team role, which then comes from the team's own list.
 * A solo's owner is never a target.
 */
export function memberTarget(member: EffectiveMember, teamRoles: ReadonlyMap<string, TeamRole>): MemberWriteTarget | null {
  const base = { userId: member.user_id, access: member.role, isGuest: member.is_guest };
  switch (member.role_source) {
    case "guest":
      return { ...base, teamRole: null };
    case "team":
      // Inherited, so the role here is the team role.
      return member.role === "owner" ? null : { ...base, teamRole: member.role };
    case "override": {
      const teamRole = teamRoles.get(member.user_id);
      return teamRole ? { ...base, teamRole } : null;
    }
    default:
      return null;
  }
}

/** An excluded team member, whose team role comes from the team's list. */
export function excludedTarget(member: ExcludedMember, teamRoles: ReadonlyMap<string, TeamRole>): MemberWriteTarget | null {
  const teamRole = teamRoles.get(member.user_id);
  return teamRole ? { userId: member.user_id, teamRole, access: "excluded", isGuest: false } : null;
}
