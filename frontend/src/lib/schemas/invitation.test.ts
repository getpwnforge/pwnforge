import { describe, expect, it } from "vitest";

import {
  createTeamLinkSchema,
  invitableRolesFor,
  inviteToTeamSchema,
  toInviteRequest,
  toLinkRequest,
} from "@/lib/schemas/invitation";

const NOW = Date.parse("2026-11-07T12:00:00Z");

describe("inviteToTeamSchema", () => {
  it("asks for someone", () => {
    const result = inviteToTeamSchema.safeParse({ target: "username", who: "  ", role: "member" });
    expect(result.success).toBe(false);
  });

  it("checks the address only when inviting by address", () => {
    expect(inviteToTeamSchema.safeParse({ target: "email", who: "nora", role: "member" }).success).toBe(false);
    expect(inviteToTeamSchema.safeParse({ target: "username", who: "nora", role: "member" }).success).toBe(true);
    expect(inviteToTeamSchema.safeParse({ target: "email", who: "nora@example.test", role: "member" }).success).toBe(true);
  });

  it("never offers Captain", () => {
    expect(inviteToTeamSchema.safeParse({ target: "username", who: "nora", role: "captain" }).success).toBe(false);
  });
});

describe("toInviteRequest", () => {
  it("sends a username without its @", () => {
    expect(toInviteRequest({ target: "username", who: " @nora ", role: "viewer" })).toEqual({ default_role: "viewer", username: "nora" });
  });

  it("sends an address as it was typed, trimmed", () => {
    expect(toInviteRequest({ target: "email", who: " nora@example.test ", role: "member" })).toEqual({
      default_role: "member",
      invited_email: "nora@example.test",
    });
  });
});

describe("createTeamLinkSchema", () => {
  it.each(["", "1", "25"])("accepts %j as a limit", (max_uses) => {
    expect(createTeamLinkSchema.safeParse({ role: "member", days: "7", max_uses }).success).toBe(true);
  });

  it.each(["0", "-2", "1.5", "abc"])("refuses %j as a limit", (max_uses) => {
    expect(createTeamLinkSchema.safeParse({ role: "member", days: "7", max_uses }).success).toBe(false);
  });
});

describe("toLinkRequest", () => {
  it("leaves the default lifetime and an empty limit to the server", () => {
    expect(toLinkRequest({ role: "member", days: "7", max_uses: "" }, NOW)).toEqual({ default_role: "member" });
  });

  it("sends a chosen lifetime a few minutes short, so 30 days never passes the limit", () => {
    const body = toLinkRequest({ role: "viewer", days: "30", max_uses: "10" }, NOW);
    expect(body.max_uses).toBe(10);
    const expires = Date.parse(body.expires_at ?? "");
    expect(expires).toBeLessThan(NOW + 30 * 86_400_000);
    expect(expires).toBeGreaterThan(NOW + 30 * 86_400_000 - 10 * 60_000);
  });
});

describe("invitableRolesFor", () => {
  it("lets the Captain give Admin, and an Admin no more than Member", () => {
    expect(invitableRolesFor("captain")).toEqual(["admin", "member", "viewer"]);
    expect(invitableRolesFor("admin")).toEqual(["member", "viewer"]);
  });
});
