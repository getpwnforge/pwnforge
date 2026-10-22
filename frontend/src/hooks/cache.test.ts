// src/hooks/cache.test.ts
import { act, renderHook } from "@testing-library/react";
import type { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";

import { AUTH_ME_QUERY_KEY } from "@/api/auth";
import { invitationKeys } from "@/api/invitations";
import { teamKeys } from "@/api/teams";
import { workspaceKeys } from "@/api/workspaces";
import * as invitations from "@/hooks/useInvitations";
import * as teams from "@/hooks/useTeams";
import * as workspaces from "@/hooks/useWorkspaces";
import { teamResponse, workspaceResponse, workspaceSummary } from "@/test/api-fixtures";
import { fakeApi, type FakeRoute } from "@/test/fake-api";
import { testQueryClient, withClient } from "@/test/query";

const S = "red-team-ab12";
const W = "ctf-2026-cd34";
const ME = "01900000-0000-7000-8000-0000000000ee";
const OTHER = "01900000-0000-7000-8000-0000000000aa";
const INV = "01900000-0000-7000-8000-0000000000bb";
const IMAGE = new Blob([new Uint8Array([0x89, 0x50])], { type: "image/png" });

/** Every entry the cache is seeded with before each write. */
const KEYS = {
  teamList: teamKeys.list(),
  team: teamKeys.detail(S),
  teamMembers: teamKeys.members(S),
  teamInvitations: teamKeys.invitations(S),
  otherTeam: teamKeys.detail("other-team"),
  workspaceList: workspaceKeys.list(),
  workspace: workspaceKeys.detail(W),
  workspaceMembers: workspaceKeys.members(W),
  workspaceExclusions: workspaceKeys.exclusions(W),
  workspaceInvitations: workspaceKeys.invitations(W),
  otherWorkspace: workspaceKeys.detail("other-ws"),
  pendingTeam: invitationKeys.pending("team"),
  pendingWorkspace: invitationKeys.pending("workspace"),
  previewTeam: invitationKeys.preview("team", "tok"),
  previewWorkspace: invitationKeys.preview("workspace", "tok"),
  me: AUTH_ME_QUERY_KEY,
} as const;

type Name = keyof typeof KEYS;
/** Untouched, marked stale, removed, or replaced by the write's answer. */
type State = "kept" | "stale" | "removed" | "set";

function seed(client: QueryClient): Map<Name, unknown> {
  const seeded = new Map<Name, unknown>();
  for (const name of Object.keys(KEYS) as Name[]) {
    let value: unknown = { seeded: name };
    if (name === "workspaceList") {
      // `W` is one of team `S`'s workspaces, `other-ws` a solo one.
      value = [
        workspaceSummary({ slug: W, team: { id: "t", slug: S, name: "Red Team", color: "rose" } }),
        workspaceSummary({ slug: "other-ws", mode: "solo", team: null }),
      ];
    }
    if (name === "me") value = { id: ME, username: "me" };
    client.setQueryData(KEYS[name], value);
    seeded.set(name, value);
  }
  return seeded;
}

function states(client: QueryClient, seeded: Map<Name, unknown>): Record<Name, State> {
  const out = {} as Record<Name, State>;
  for (const name of Object.keys(KEYS) as Name[]) {
    const query = client.getQueryCache().find({ queryKey: KEYS[name], exact: true });
    if (!query) out[name] = "removed";
    else if (query.state.isInvalidated) out[name] = "stale";
    else if (query.state.data !== seeded.get(name)) out[name] = "set";
    else out[name] = "kept";
  }
  return out;
}

type Case = {
  name: string;
  run: (client: QueryClient) => Promise<unknown>;
  routes: Record<string, FakeRoute>;
  expected: Partial<Record<Name, State>>;
};

/** One write, the variables it is called with, and what it must do to the cache. */
function write<V>(
  name: string,
  hook: () => { mutateAsync: (variables: V) => Promise<unknown> },
  variables: V,
  routes: Record<string, FakeRoute>,
  expected: Partial<Record<Name, State>>,
): Case {
  return {
    name,
    routes,
    expected,
    run: async (client) => {
      const { result } = renderHook(hook, { wrapper: withClient(client) });
      await act(() => result.current.mutateAsync(variables));
    },
  };
}

const ok = { status: 204 };
const allWorkspaceQueries = {
  workspaceList: "stale",
  workspace: "stale",
  workspaceMembers: "stale",
  workspaceExclusions: "stale",
  workspaceInvitations: "stale",
  otherWorkspace: "stale",
} as const;
const workspaceWWithAllUnder = {
  workspace: "removed",
  workspaceMembers: "removed",
  workspaceExclusions: "removed",
  workspaceInvitations: "removed",
} as const;

const cases: Case[] = [
  // Teams
  write("create a team", teams.useCreateTeam, { name: "Red", color: "rose" },
    { "POST /teams": { status: 201, data: teamResponse({ slug: S }) } },
    { team: "set", teamList: "stale" }),
  write("update a team", teams.useUpdateTeam, { slug: S, patch: { name: "Blue" } },
    { [`PATCH /teams/${S}`]: { status: 200, data: teamResponse({ slug: S }) } },
    { team: "set", teamList: "stale", workspaceList: "stale" }),
  write("delete a team", teams.useDeleteTeam, S,
    { [`DELETE /teams/${S}`]: ok },
    { team: "removed", teamMembers: "removed", teamInvitations: "removed", teamList: "stale",
      ...workspaceWWithAllUnder, workspaceList: "stale" }),
  write("change a member's role", teams.useChangeTeamMemberRole, { slug: S, userId: OTHER, role: "viewer" },
    { [`PATCH /teams/${S}/members/${OTHER}`]: { status: 200, data: {} } },
    { team: "stale", teamMembers: "stale", teamList: "stale", ...allWorkspaceQueries }),
  write("remove another member", teams.useRemoveTeamMember, { slug: S, userId: OTHER },
    { [`DELETE /teams/${S}/members/${OTHER}`]: ok },
    { team: "stale", teamMembers: "stale", teamList: "stale", ...allWorkspaceQueries }),
  write("leave a team", teams.useRemoveTeamMember, { slug: S, userId: ME },
    { [`DELETE /teams/${S}/members/${ME}`]: ok },
    { team: "removed", teamMembers: "removed", teamInvitations: "removed", teamList: "stale",
      ...workspaceWWithAllUnder, workspaceList: "stale" }),
  write("transfer the Captain role", teams.useTransferCaptaincy, { slug: S, userId: OTHER },
    { [`POST /teams/${S}/transfer`]: ok },
    { team: "stale", teamMembers: "stale", teamList: "stale", ...allWorkspaceQueries }),
  write("upload a team avatar", teams.useUploadTeamAvatar, { teamId: "t", slug: S, image: IMAGE },
    { "POST /avatars/teams/t": { status: 200, data: {} } },
    { team: "stale", teamList: "stale", workspaceList: "stale" }),
  write("delete a team avatar", teams.useDeleteTeamAvatar, { teamId: "t", slug: S },
    { "DELETE /avatars/teams/t": ok },
    { team: "stale", teamList: "stale", workspaceList: "stale" }),

  // Workspaces
  write("create a workspace", workspaces.useCreateWorkspace,
    { name: "CTF", color: "blue", type: "event", mode: "solo" },
    { "POST /workspaces": { status: 201, data: workspaceResponse({ slug: W }) } },
    { workspace: "set", workspaceList: "stale" }),
  write("update a workspace", workspaces.useUpdateWorkspace, { slug: W, patch: { name: "CTF 2" } },
    { [`PATCH /workspaces/${W}`]: { status: 200, data: workspaceResponse({ slug: W }) } },
    { workspace: "set", workspaceList: "stale" }),
  write("delete a workspace", workspaces.useDeleteWorkspace, W,
    { [`DELETE /workspaces/${W}`]: ok },
    { ...workspaceWWithAllUnder, workspaceList: "stale" }),
  write("archive a workspace", workspaces.useArchiveWorkspace, W,
    { [`POST /workspaces/${W}/archive`]: { status: 200, data: workspaceResponse({ slug: W, archived: true }) } },
    { workspace: "set", workspaceList: "stale", workspaceInvitations: "stale" }),
  write("unarchive a workspace", workspaces.useUnarchiveWorkspace, W,
    { [`POST /workspaces/${W}/unarchive`]: { status: 200, data: workspaceResponse({ slug: W }) } },
    { workspace: "set", workspaceList: "stale" }),
  write("favourite a workspace", workspaces.useSetFavorite, { slug: W, favorite: true },
    { [`POST /workspaces/${W}/favorite`]: ok },
    { workspaceList: "stale" }),
  write("set a member's role", workspaces.useSetWorkspaceMemberRole, { slug: W, userId: OTHER, role: "viewer" },
    { [`PATCH /workspaces/${W}/members/${OTHER}`]: ok },
    { workspaceMembers: "stale" }),
  write("clear a role override", workspaces.useClearWorkspaceRoleOverride, { slug: W, userId: OTHER },
    { [`DELETE /workspaces/${W}/members/${OTHER}/role-override`]: ok },
    { workspaceMembers: "stale" }),
  write("exclude a member", workspaces.useExcludeWorkspaceMember, { slug: W, userId: OTHER },
    { [`POST /workspaces/${W}/members/${OTHER}/exclusion`]: ok },
    { workspaceMembers: "stale", workspaceExclusions: "stale" }),
  write("lift an exclusion", workspaces.useLiftWorkspaceExclusion, { slug: W, userId: OTHER },
    { [`DELETE /workspaces/${W}/members/${OTHER}/exclusion`]: ok },
    { workspaceMembers: "stale", workspaceExclusions: "stale" }),
  write("remove a guest", workspaces.useRemoveWorkspaceGuest, { slug: W, userId: OTHER },
    { [`DELETE /workspaces/${W}/guests/${OTHER}`]: ok },
    { workspaceMembers: "stale" }),
  write("leave a workspace", workspaces.useLeaveWorkspace, W,
    { [`POST /workspaces/${W}/leave`]: ok },
    { ...workspaceWWithAllUnder, workspaceList: "stale" }),
  write("upload a workspace avatar", workspaces.useUploadWorkspaceAvatar, { workspaceId: "w", slug: W, image: IMAGE },
    { "POST /avatars/workspaces/w": { status: 200, data: {} } },
    { workspace: "stale", workspaceList: "stale" }),
  write("delete a workspace avatar", workspaces.useDeleteWorkspaceAvatar, { workspaceId: "w", slug: W },
    { "DELETE /avatars/workspaces/w": ok },
    { workspace: "stale", workspaceList: "stale" }),

  // Team invitations
  write("invite to a team", invitations.useCreateTeamInvitation, { slug: S, body: { default_role: "member" } },
    { [`POST /teams/${S}/invitations`]: { status: 201, data: {} } },
    { teamInvitations: "stale" }),
  write("revoke a team invitation", invitations.useRevokeTeamInvitation, { slug: S, invitationId: INV },
    { [`DELETE /teams/${S}/invitations/${INV}`]: ok },
    { teamInvitations: "stale" }),
  write("accept a team invitation", invitations.useAcceptTeamInvitation, INV,
    { [`POST /teams/invitations/${INV}/accept`]: { status: 200, data: teamResponse({ slug: S }) } },
    { team: "set", teamList: "stale", pendingTeam: "stale", ...allWorkspaceQueries }),
  write("reject a team invitation", invitations.useRejectTeamInvitation, INV,
    { [`POST /teams/invitations/${INV}/reject`]: ok },
    { pendingTeam: "stale" }),
  write("join a team by link", invitations.useJoinTeam, "tok",
    { "POST /teams/join": { status: 200, data: teamResponse({ slug: S }) } },
    { team: "set", teamList: "stale", pendingTeam: "stale", previewTeam: "removed", ...allWorkspaceQueries }),

  // Workspace invitations
  write("invite to a workspace", invitations.useCreateWorkspaceInvitation,
    { slug: W, body: { role: "viewer", username: "alice" } },
    { [`POST /workspaces/${W}/invitations`]: { status: 201, data: {} } },
    { workspaceInvitations: "stale" }),
  write("cancel a workspace invitation", invitations.useCancelWorkspaceInvitation, { slug: W, invitationId: INV },
    { [`DELETE /workspaces/${W}/invitations/${INV}`]: ok },
    { workspaceInvitations: "stale" }),
  write("accept a workspace invitation", invitations.useAcceptWorkspaceInvitation, INV,
    { [`POST /workspaces/invitations/${INV}/accept`]: { status: 200, data: workspaceResponse({ slug: W }) } },
    { workspace: "set", workspaceList: "stale", pendingWorkspace: "stale" }),
  write("reject a workspace invitation", invitations.useRejectWorkspaceInvitation, INV,
    { [`POST /workspaces/invitations/${INV}/reject`]: ok },
    { pendingWorkspace: "stale" }),
  write("join a workspace by link", invitations.useJoinWorkspace, "tok",
    { "POST /workspaces/join": { status: 200, data: workspaceResponse({ slug: W }) } },
    { workspace: "set", workspaceList: "stale", pendingWorkspace: "stale", previewWorkspace: "removed" }),
];

describe("what each write does to the cache", () => {
  it.each(cases)("$name", async ({ run, routes, expected }) => {
    fakeApi(routes);
    const client = testQueryClient();
    const seeded = seed(client);

    await run(client);

    const kept = Object.fromEntries(Object.keys(KEYS).map((name) => [name, "kept"]));
    expect(states(client, seeded)).toEqual({ ...kept, ...expected });
  });
});
