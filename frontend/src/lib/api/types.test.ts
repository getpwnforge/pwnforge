// src/lib/api/types.test.ts
import { describe, expectTypeOf, it } from "vitest";

import type { operations, paths } from "@/lib/api/schema.gen";
import type {
  CreatedTeamInvitation,
  EffectiveMember,
  Platform,
  TeamInvitationResponse,
  TeamLinkResponse,
  TeamMemberResponse,
  WorkspaceInvitationResponse,
  WorkspaceSummary,
} from "@/lib/api/types";

// Type assertions: they hold or fail when `pnpm typecheck` compiles this file
// (tsconfig.test.json); vitest only runs the empty bodies.

/** The JSON body of an operation's answer for one status. */
type Answer<Op extends keyof operations, Status extends keyof operations[Op]["responses"]> =
  operations[Op]["responses"][Status] extends { content: { "application/json": infer Body } }
    ? Body
    : never;

describe("the generated API types", () => {
  it("spell the platforms as the API does", () => {
    expectTypeOf<Platform>().toEqualTypeOf<"htb" | "thm" | "root_me" | "ctftime" | "other">();
  });

  it("keep the working mode apart from the workspace type", () => {
    expectTypeOf<WorkspaceSummary["mode"]>().toEqualTypeOf<"solo" | "team">();
    expectTypeOf<WorkspaceSummary["type"]>().toEqualTypeOf<"platform" | "event">();
  });

  it("answer the workspace list with summaries", () => {
    expectTypeOf<
      paths["/api/v1/workspaces"]["get"]["responses"][200]["content"]["application/json"]
    >().toEqualTypeOf<WorkspaceSummary[]>();
  });

  it("tell the team roster from the workspace roster", () => {
    expectTypeOf<Answer<"list_team_members", 200>>().toEqualTypeOf<TeamMemberResponse[]>();
    expectTypeOf<Answer<"list_workspace_members", 200>>().toEqualTypeOf<EffectiveMember[]>();
  });

  it("give a token to a generic team link only", () => {
    expectTypeOf<TeamLinkResponse>().toHaveProperty("token").toEqualTypeOf<string>();
    expectTypeOf<TeamInvitationResponse>().not.toHaveProperty("token");
    expectTypeOf<WorkspaceInvitationResponse>().not.toHaveProperty("token");
    expectTypeOf<Answer<"create_team_invitation", 201>>().toEqualTypeOf<CreatedTeamInvitation>();
  });
});
