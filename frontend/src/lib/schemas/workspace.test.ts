import { describe, expect, it } from "vitest";

import {
  createWorkspaceSchema,
  toWorkspacePatch,
  updateWorkspaceSchema,
  type CreateWorkspaceValues,
  type UpdateWorkspaceValues,
} from "@/lib/schemas/workspace";

const valid: CreateWorkspaceValues = {
  name: "Root-Me grind", description: "", color: "ember", type: "platform", platform: "root_me",
  mode: "solo", team_id: null, starts_at: null, ends_at: null, lock_after_end: false,
};

/** Each issue as "path: message", or [] when the values pass. */
function issues(over: Partial<CreateWorkspaceValues>): string[] {
  const result = createWorkspaceSchema.safeParse({ ...valid, ...over });
  return result.success ? [] : result.error.issues.map((i) => `${i.path.join(".")}: ${i.message}`);
}

describe("createWorkspaceSchema", () => {
  it("accepts a solo platform workspace", () => {
    expect(issues({})).toEqual([]);
  });

  it("wants a name that is not blank", () => {
    expect(issues({ name: "   " })).toEqual(["name: workspaces:validation.nameRequired"]);
  });

  it("wants a platform for a platform workspace, not for an event", () => {
    expect(issues({ platform: null })).toEqual(["platform: workspaces:validation.platformRequired"]);
    expect(issues({ type: "event", platform: null })).toEqual([]);
  });

  it("knows only the five platforms of the API", () => {
    expect(issues({ platform: "rm" as never })).not.toEqual([]);
  });

  it("wants a team in team mode", () => {
    expect(issues({ mode: "team" })).toEqual(["team_id: workspaces:validation.teamRequired"]);
    expect(issues({ mode: "team", team_id: "0000000a-0000-7000-8000-000000000001" })).toEqual([]);
  });

  it("refuses an end that is not after the start", () => {
    const event = { type: "event" as const, starts_at: "2026-11-10T10:00" };
    expect(issues({ ...event, ends_at: "2026-11-09T10:00" })).toEqual(["ends_at: workspaces:validation.endsBeforeStart"]);
    expect(issues({ ...event, ends_at: "2026-11-10T10:00" })).toEqual(["ends_at: workspaces:validation.endsBeforeStart"]);
    expect(issues({ ...event, ends_at: "2026-11-12T18:00" })).toEqual([]);
  });
});

const current: UpdateWorkspaceValues = {
  name: "CTF Hexagon", description: "Autumn CTF", color: "rose",
  starts_at: "2026-11-10T10:00", ends_at: "2026-11-12T18:00", lock_after_end: false,
  default_writeup_visibility: "team",
};

describe("toWorkspacePatch", () => {
  it("sends only what changed", () => {
    expect(toWorkspacePatch({ ...current, name: "CTF Hexagon 2026" }, { name: true })).toEqual({ name: "CTF Hexagon 2026" });
    expect(toWorkspacePatch(current, {})).toEqual({});
  });

  it("clears an emptied description with null", () => {
    expect(toWorkspacePatch({ ...current, description: "   " }, { description: true })).toEqual({ description: null });
  });

  it("sends the dates as UTC instants, or null once cleared", () => {
    expect(toWorkspacePatch({ ...current, starts_at: "2026-11-11T09:00" }, { starts_at: true }))
      .toEqual({ starts_at: new Date("2026-11-11T09:00").toISOString() });
    expect(toWorkspacePatch({ ...current, ends_at: null }, { ends_at: true })).toEqual({ ends_at: null });
  });

  it("passes the other fields as they are", () => {
    expect(toWorkspacePatch({ ...current, color: "teal", lock_after_end: true }, { color: true, lock_after_end: true }))
      .toEqual({ color: "teal", lock_after_end: true });
  });
});

describe("updateWorkspaceSchema", () => {
  it("refuses an end that is not after the start", () => {
    const result = updateWorkspaceSchema.safeParse({ ...current, ends_at: "2026-11-09T10:00" });
    expect(result.success).toBe(false);
    expect(result.error?.issues[0]?.message).toBe("workspaces:validation.endsBeforeStart");
  });
});
