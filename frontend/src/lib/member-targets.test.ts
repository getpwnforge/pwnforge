import { describe, expect, it } from "vitest";

import { excludedMember, effectiveMember, teamMember } from "@/test/api-fixtures";
import { excludedTarget, memberTarget, teamRolesById } from "@/lib/member-targets";

const ROLES = teamRolesById([
  teamMember({ user_id: "u-julie", role: "member" }),
  teamMember({ user_id: "u-nora", role: "viewer" }),
]);

describe("memberTarget", () => {
  it("reads the team role off an inherited role", () => {
    expect(memberTarget(effectiveMember({ user_id: "u-marc", role: "admin", role_source: "team" }), ROLES)).toEqual({
      userId: "u-marc",
      teamRole: "admin",
      access: "admin",
      isGuest: false,
    });
  });

  it("takes an overridden member's team role from the team's list", () => {
    expect(memberTarget(effectiveMember({ user_id: "u-julie", role: "admin", role_source: "override" }), ROLES)).toEqual({
      userId: "u-julie",
      teamRole: "member",
      access: "admin",
      isGuest: false,
    });
  });

  it("cannot tell an override before the team's list is known", () => {
    expect(memberTarget(effectiveMember({ user_id: "u-julie", role: "admin", role_source: "override" }), new Map())).toBeNull();
  });

  it("gives a guest no team role", () => {
    expect(memberTarget(effectiveMember({ user_id: "u-kenji", role: "member", role_source: "guest", is_guest: true }), ROLES)).toEqual({
      userId: "u-kenji",
      teamRole: null,
      access: "member",
      isGuest: true,
    });
  });

  it("never targets a solo's owner", () => {
    expect(memberTarget(effectiveMember({ role: "owner", role_source: "owner" }), ROLES)).toBeNull();
  });
});

describe("excludedTarget", () => {
  it("is excluded, with the team role from the team's list", () => {
    expect(excludedTarget(excludedMember({ user_id: "u-nora" }), ROLES)).toEqual({
      userId: "u-nora",
      teamRole: "viewer",
      access: "excluded",
      isGuest: false,
    });
  });

  it("waits for the team's list", () => {
    expect(excludedTarget(excludedMember({ user_id: "u-nora" }), new Map())).toBeNull();
  });
});
