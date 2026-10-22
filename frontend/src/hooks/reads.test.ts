// src/hooks/reads.test.ts
import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { workspaceKeys } from "@/api/workspaces";
import * as invitations from "@/hooks/useInvitations";
import * as teams from "@/hooks/useTeams";
import * as workspaces from "@/hooks/useWorkspaces";
import { workspaceSummary } from "@/test/api-fixtures";
import { deferred, fakeApi, type FakeReply } from "@/test/fake-api";
import { testQueryClient, withClient } from "@/test/query";

const S = "red-team-ab12";
const W = "ctf-2026-cd34";

/** A read hook given its argument, or nothing, and the request it sends. */
type Read = [name: string, use: (arg: string | undefined) => { fetchStatus: string }, route: string];

const reads: Read[] = [
  ["useTeam", (slug) => teams.useTeam(slug), `GET /teams/${S}`],
  ["useTeamMembers", (slug) => teams.useTeamMembers(slug), `GET /teams/${S}/members`],
  ["useTeamInvitations", (slug) => invitations.useTeamInvitations(slug), `GET /teams/${S}/invitations`],
  ["useWorkspace", (slug) => workspaces.useWorkspace(slug), `GET /workspaces/${W}`],
  ["useWorkspaceMembers", (slug) => workspaces.useWorkspaceMembers(slug), `GET /workspaces/${W}/members`],
  ["useWorkspaceExclusions", (slug) => workspaces.useWorkspaceExclusions(slug), `GET /workspaces/${W}/exclusions`],
  [
    "useWorkspaceInvitations",
    (slug) => invitations.useWorkspaceInvitations(slug),
    `GET /workspaces/${W}/invitations`,
  ],
  [
    "useTeamInvitationPreview",
    (token) => invitations.useTeamInvitationPreview(token),
    "POST /teams/invitations/preview",
  ],
  [
    "useWorkspaceInvitationPreview",
    (token) => invitations.useWorkspaceInvitationPreview(token),
    "POST /workspaces/invitations/preview",
  ],
];

/** The argument a read's route was written for. */
function argumentOf(route: string): string {
  if (route.includes("preview")) return "tok";
  return route.includes("/teams/") ? S : W;
}

describe("a read", () => {
  it.each(reads)("%s sends nothing until it has what to ask for", async (_name, use, route) => {
    const server = fakeApi({ [route]: { status: 200, data: [] } });
    const client = testQueryClient();

    const { result, rerender } = renderHook(({ arg }) => use(arg), {
      wrapper: withClient(client),
      initialProps: { arg: undefined as string | undefined },
    });
    expect(result.current.fetchStatus).toBe("idle");
    expect(server.requests).toEqual([]);

    rerender({ arg: argumentOf(route) });
    await waitFor(() => expect(server.count(route)).toBe(1));
  });

  it("of one resource takes a 404 for an answer and does not ask again", async () => {
    const server = fakeApi({ [`GET /workspaces/${W}`]: { status: 404, data: { error: "workspace_not_found" } } });
    // The app's default: one second chance, which a 404 must not get.
    const client = testQueryClient();
    client.setDefaultOptions({ queries: { retry: 1 } });

    const { result } = renderHook(() => workspaces.useWorkspace(W), { wrapper: withClient(client) });

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(server.count(`GET /workspaces/${W}`)).toBe(1);
    expect(result.current.error).toMatchObject({ status: 404, code: "workspace_not_found" });
  });

  it("of a list loads it", async () => {
    fakeApi({ "GET /teams": { status: 200, data: [] }, "GET /workspaces": { status: 200, data: [] } });
    const client = testQueryClient();

    const { result } = renderHook(() => [teams.useTeams(), workspaces.useWorkspaces()], {
      wrapper: withClient(client),
    });

    await waitFor(() => expect(result.current.every((query) => query.isSuccess)).toBe(true));
  });
});

describe("the favourite", () => {
  function seeded() {
    const client = testQueryClient();
    const list = [workspaceSummary({ slug: W, favorite: false }), workspaceSummary({ slug: "other-ws" })];
    client.setQueryData(workspaceKeys.list(), list);
    return { client, list };
  }

  const flag = (client: ReturnType<typeof testQueryClient>) =>
    client
      .getQueryData<ReturnType<typeof workspaceSummary>[]>(workspaceKeys.list())
      ?.find((workspace) => workspace.slug === W)?.favorite;

  it("shows at once, before the server answers, and is refetched after", async () => {
    const answer = deferred<FakeReply>();
    fakeApi({ [`POST /workspaces/${W}/favorite`]: () => answer.promise });
    const { client } = seeded();
    const { result } = renderHook(() => workspaces.useSetFavorite(), { wrapper: withClient(client) });

    act(() => result.current.mutate({ slug: W, favorite: true }));
    await waitFor(() => expect(flag(client)).toBe(true));

    answer.resolve({ status: 204 });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(flag(client)).toBe(true);
    expect(client.getQueryState(workspaceKeys.list())?.isInvalidated).toBe(true);
  });

  it("is put back as it was when the server refuses it", async () => {
    const answer = deferred<FakeReply>();
    fakeApi({ [`POST /workspaces/${W}/favorite`]: () => answer.promise });
    const { client, list } = seeded();
    const { result } = renderHook(() => workspaces.useSetFavorite(), { wrapper: withClient(client) });

    act(() => result.current.mutate({ slug: W, favorite: true }));
    await waitFor(() => expect(flag(client)).toBe(true));

    answer.resolve({ status: 500 });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(client.getQueryData(workspaceKeys.list())).toEqual(list);
  });
});
