// src/lib/helpers.test.ts
import { describe, expect, it } from "vitest";

import { avatarUrl } from "@/lib/avatar";
import { invitationLink } from "@/lib/invitation-link";
import { workspaceStatus } from "@/lib/workspace-status";

describe("workspaceStatus", () => {
  const now = new Date("2026-10-22T12:00:00Z");
  const at = (overrides: { archived?: boolean; starts_at?: string | null; ends_at?: string | null }) =>
    workspaceStatus({ archived: false, starts_at: null, ends_at: null, ...overrides }, now);

  it.each([
    ["archived, whatever its dates", { archived: true, starts_at: "2026-10-21T00:00:00Z" }, "archived"],
    ["permanent with no date", {}, "permanent"],
    ["upcoming before it starts", { starts_at: "2026-10-23T00:00:00Z" }, "upcoming"],
    ["live from its start", { starts_at: "2026-10-22T12:00:00Z" }, "live"],
    ["live in its window", { starts_at: "2026-10-21T00:00:00Z", ends_at: "2026-10-23T00:00:00Z" }, "live"],
    ["live with only an end ahead", { ends_at: "2026-10-23T00:00:00Z" }, "live"],
    ["ended at its end", { starts_at: "2026-10-21T00:00:00Z", ends_at: "2026-10-22T12:00:00Z" }, "ended"],
    ["ended with only an end behind", { ends_at: "2026-10-21T00:00:00Z" }, "ended"],
  ] as const)("is %s", (_case, overrides, expected) => {
    expect(at(overrides)).toBe(expected);
  });
});

describe("avatarUrl", () => {
  const id = "01900000-0000-7000-8000-000000000001";
  const version = "01900000-0000-7000-8000-0000000000ff";

  it("builds the documented URL", () => {
    expect(avatarUrl("teams", id, version, 256)).toBe(`/api/v1/avatars/teams/${id}/${version}/256.webp`);
    expect(avatarUrl("users", id, version, 64)).toBe(`/api/v1/avatars/users/${id}/${version}/64.webp`);
  });

  it.each([null, undefined, ""])("is null when there is no version to show (%s)", (missing) => {
    expect(avatarUrl("workspaces", id, missing, 256)).toBeNull();
  });
});

describe("invitationLink", () => {
  it("has the mails' format, the token in the fragment", () => {
    expect(invitationLink("team", "aB3-_x", "https://pwnforge.example")).toBe(
      "https://pwnforge.example/invitations/team#token=aB3-_x",
    );
    expect(invitationLink("workspace", "tok", "https://pwnforge.example")).toBe(
      "https://pwnforge.example/invitations/workspace#token=tok",
    );
  });

  it("uses the page's origin by default, and encodes what the token holds", () => {
    expect(invitationLink("team", "a b")).toBe(`${window.location.origin}/invitations/team#token=a%20b`);
  });
});
