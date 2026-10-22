// src/lib/permissions.test.ts
import { describe, expect, it } from "vitest";

import type { Membership, TeamRole } from "@/lib/api/types";
import {
  canArchiveWorkspace,
  canCancelWorkspaceInvitation,
  canChangeTeamMemberRole,
  canClearRoleOverride,
  canCreateTeamWorkspace,
  canDeleteTeam,
  canDeleteWorkspace,
  canEditTeam,
  canEditWorkspace,
  canExcludeWorkspaceMember,
  canInviteToTeam,
  canInviteToWorkspace,
  canLeaveTeam,
  canLeaveWorkspace,
  canLiftExclusion,
  canReadExclusions,
  canRemoveTeamMember,
  canRemoveWorkspaceGuest,
  canSetWorkspaceMemberRole,
  canTransferCaptaincy,
  type MemberWriteTarget,
} from "@/lib/permissions";

// Literal truth tables, written from the backend's rules and its own tests
// (routes/teams.rs, routes/workspaces.rs, routes/workspace_members.rs),
// never computed the way the helpers compute them.

const ROLES: TeamRole[] = ["captain", "admin", "member", "viewer"];

describe("the team helpers", () => {
  it.each([
    ["captain", true, true, true, true],
    ["admin", true, false, true, false],
    ["member", false, false, false, false],
    ["viewer", false, false, false, false],
  ] as const)("%s: edit %s, delete %s, invite %s, transfer %s", (role, edit, del, invite, transfer) => {
    expect(canEditTeam(role)).toBe(edit);
    expect(canDeleteTeam(role)).toBe(del);
    expect(canInviteToTeam(role)).toBe(invite);
    expect(canTransferCaptaincy(role)).toBe(transfer);
  });

  it("only an Admin or the Captain creates a team workspace", () => {
    expect(ROLES.filter(canCreateTeamWorkspace)).toEqual(["captain", "admin"]);
  });

  it("everyone but the Captain may leave", () => {
    expect(ROLES.filter(canLeaveTeam)).toEqual(["admin", "member", "viewer"]);
  });

  // a_role_change_can_neither_name_nor_demote_the_captain, and can_manage_role.
  it.each([
    ["captain", "member", "viewer", true],
    ["captain", "admin", "member", true],
    ["captain", "member", "captain", false],
    ["captain", "captain", "admin", false],
    ["admin", "member", "admin", true],
    ["admin", "admin", "member", true],
    ["admin", "captain", "admin", false],
    ["admin", "viewer", "captain", false],
    ["member", "viewer", "member", false],
    ["viewer", "viewer", "member", false],
  ] as const)("%s changing a %s to %s: %s", (actor, target, next, expected) => {
    expect(canChangeTeamMemberRole(actor, target, next)).toBe(expected);
  });

  // an_admin_cannot_remove_the_captain, the_captain_cannot_leave, can_remove_member.
  it.each([
    ["captain", "admin", false, true],
    ["captain", "captain", true, false],
    ["admin", "captain", false, false],
    ["admin", "member", false, true],
    ["admin", "admin", false, true],
    ["member", "viewer", false, false],
    ["member", "member", true, true],
    ["viewer", "viewer", true, true],
  ] as const)("%s removing a %s (themselves: %s): %s", (actor, target, self, expected) => {
    const actorId = "actor";
    expect(
      canRemoveTeamMember({ id: actorId, role: actor }, { id: self ? actorId : "target", role: target }),
    ).toBe(expected);
  });
});

describe("the workspace helpers", () => {
  const team = { mode: "team", archived: false } as const;
  const archived = { mode: "team", archived: true } as const;
  const solo = { mode: "solo", archived: false } as const;
  const ACCESSES: Membership[] = ["owner", "captain", "admin", "member", "viewer"];

  it("edit: an Admin and above, never once archived", () => {
    expect(ACCESSES.filter((access) => canEditWorkspace(team, access))).toEqual(["owner", "captain", "admin"]);
    expect(ACCESSES.filter((access) => canEditWorkspace(archived, access))).toEqual([]);
  });

  it("archive and delete: the Captain, or the owner of a solo workspace", () => {
    expect(ACCESSES.filter(canArchiveWorkspace)).toEqual(["owner", "captain"]);
    expect(ACCESSES.filter(canDeleteWorkspace)).toEqual(["owner", "captain"]);
  });

  it.each([
    [team, "member", true, true],
    [team, "viewer", true, false],
    [team, "member", false, false],
    [team, "admin", true, true],
    [archived, "captain", true, false],
    [solo, "owner", false, false],
  ] as const)("invite in %o as %s (on the team: %s): %s", (workspace, access, onTeam, expected) => {
    expect(canInviteToWorkspace(workspace, access, onTeam)).toBe(expected);
  });

  it("cancel an invitation: its author, or an Admin", () => {
    expect(canCancelWorkspaceInvitation("member", "me", "me")).toBe(true);
    expect(canCancelWorkspaceInvitation("admin", "me", "someone")).toBe(true);
    expect(canCancelWorkspaceInvitation("member", "me", "someone")).toBe(false);
    expect(canCancelWorkspaceInvitation("member", "me", null)).toBe(false);
  });

  it("read the exclusions: an Admin and above, on a team workspace", () => {
    expect(ACCESSES.filter((access) => canReadExclusions(team, access))).toEqual(["owner", "captain", "admin"]);
    expect(canReadExclusions(solo, "owner")).toBe(false);
  });

  it("leave: anyone but the Captain, on a team workspace", () => {
    expect(ACCESSES.filter((access) => canLeaveWorkspace(team, access))).toEqual(["owner", "admin", "member", "viewer"]);
    expect(canLeaveWorkspace(solo, "owner")).toBe(false);
  });
});

// The fixture of routes/workspace_members.rs: the Captain, two Admins, a
// Member, a Viewer and a guest admitted as Member.
const captain = { id: "captain", access: "captain" } as const;
const admin = { id: "admin", access: "admin" } as const;
const memberActor = { id: "member", access: "member" } as const;
const viewerActor = { id: "viewer", access: "viewer" } as const;
const guestActor = { id: "guest", access: "member" } as const;

const target = (userId: string, teamRole: TeamRole | null, access: MemberWriteTarget["access"]): MemberWriteTarget => ({
  userId,
  teamRole,
  access,
  isGuest: teamRole === null,
});
const T = {
  captain: target("captain", "captain", "captain"),
  admin: target("admin", "admin", "admin"),
  otherAdmin: target("other-admin", "admin", "admin"),
  member: target("member", "member", "member"),
  viewer: target("viewer", "viewer", "viewer"),
  guest: target("guest", null, "member"),
};
const team = { mode: "team", archived: false } as const;
const archived = { mode: "team", archived: true } as const;

describe("the workspace member writes", () => {
  it("the Captain overrides a member up and down", () => {
    expect(canSetWorkspaceMemberRole(team, captain, T.member, "admin")).toBe(true);
    expect(canSetWorkspaceMemberRole(team, captain, T.member, "viewer")).toBe(true);
  });

  it("an Admin overrides below Admin up and down, and cannot give Admin", () => {
    expect(canSetWorkspaceMemberRole(team, admin, T.member, "viewer")).toBe(true);
    expect(canSetWorkspaceMemberRole(team, admin, T.viewer, "member")).toBe(true);
    expect(canSetWorkspaceMemberRole(team, admin, T.member, "admin")).toBe(false);
  });

  it("an Admin cannot act on another Admin", () => {
    expect(canSetWorkspaceMemberRole(team, admin, T.otherAdmin, "viewer")).toBe(false);
    expect(canExcludeWorkspaceMember(team, admin, T.otherAdmin)).toBe(false);
  });

  it("an Admin cannot give back an Admin team role, the Captain can", () => {
    const heldDown = target("other-admin", "admin", "member");
    const excluded = target("other-admin", "admin", "excluded");
    expect(canClearRoleOverride(team, admin, heldDown)).toBe(false);
    expect(canLiftExclusion(team, admin, excluded)).toBe(false);
    expect(canClearRoleOverride(team, captain, heldDown)).toBe(true);
    expect(canLiftExclusion(team, captain, excluded)).toBe(true);
  });

  it("an Admin lifts the exclusion of a member below Admin", () => {
    expect(canLiftExclusion(team, admin, target("member", "member", "excluded"))).toBe(true);
  });

  it("nobody acts on the Captain or on themselves", () => {
    expect(canSetWorkspaceMemberRole(team, admin, T.captain, "viewer")).toBe(false);
    expect(canExcludeWorkspaceMember(team, captain, T.captain)).toBe(false);
    expect(canSetWorkspaceMemberRole(team, admin, T.admin, "viewer")).toBe(false);
  });

  it("members, viewers and guests have no access", () => {
    for (const actor of [memberActor, viewerActor, guestActor]) {
      expect(canSetWorkspaceMemberRole(team, actor, T.viewer, "member")).toBe(false);
      expect(canExcludeWorkspaceMember(team, actor, T.member)).toBe(false);
      expect(canRemoveWorkspaceGuest(team, actor, T.guest)).toBe(false);
    }
  });

  it("a solo workspace refuses every member write", () => {
    const solo = { mode: "solo", archived: false } as const;
    const owner = { id: "owner", access: "owner" } as const;
    expect(canSetWorkspaceMemberRole(solo, owner, T.member, "viewer")).toBe(false);
    expect(canExcludeWorkspaceMember(solo, owner, T.member)).toBe(false);
    expect(canRemoveWorkspaceGuest(solo, owner, T.guest)).toBe(false);
  });

  it("an archived workspace allows only taking access away", () => {
    expect(canRemoveWorkspaceGuest(archived, admin, T.guest)).toBe(true);
    expect(canExcludeWorkspaceMember(archived, admin, T.member)).toBe(true);
    expect(canSetWorkspaceMemberRole(archived, captain, T.otherAdmin, "viewer")).toBe(false);
    expect(canClearRoleOverride(archived, captain, T.otherAdmin)).toBe(false);
    expect(canLiftExclusion(archived, captain, target("viewer", "viewer", "excluded"))).toBe(false);
  });

  it("a guest's role is capped at Member, and a guest can be removed", () => {
    expect(canSetWorkspaceMemberRole(team, captain, T.guest, "admin")).toBe(false);
    expect(canSetWorkspaceMemberRole(team, captain, T.guest, "viewer")).toBe(true);
    expect(canRemoveWorkspaceGuest(team, admin, T.guest)).toBe(true);
  });

  it("a write for the other kind of member is refused", () => {
    expect(canExcludeWorkspaceMember(team, admin, T.guest)).toBe(false);
    expect(canClearRoleOverride(team, admin, T.guest)).toBe(false);
    expect(canRemoveWorkspaceGuest(team, admin, T.member)).toBe(false);
  });

  it("someone neither on the team nor a guest is no target", () => {
    const stranger = { userId: "stranger", teamRole: null, access: "member", isGuest: false } as const;
    expect(canSetWorkspaceMemberRole(team, captain, stranger, "viewer")).toBe(false);
  });
});
