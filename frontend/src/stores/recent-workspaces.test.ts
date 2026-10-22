// src/stores/recent-workspaces.test.ts
import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { workspaceKeys } from "@/api/workspaces";
import { useDeleteTeam, useRemoveTeamMember } from "@/hooks/useTeams";
import {
  useDeleteWorkspace,
  useLeaveWorkspace,
  useRecordWorkspaceVisit,
  useWorkspace,
} from "@/hooks/useWorkspaces";
import { AUTH_ME_QUERY_KEY } from "@/api/auth";
import {
  RECENT_WORKSPACES_LIMIT,
  RECENT_WORKSPACES_STORAGE_KEY,
  readRecentSlugs,
  useRecentWorkspaces,
} from "@/stores/recent-workspaces";
import { workspaceResponse, workspaceSummary } from "@/test/api-fixtures";
import { fakeApi } from "@/test/fake-api";
import { testQueryClient, withClient } from "@/test/query";

const recent = () => useRecentWorkspaces.getState().slugs;
const visit = (...slugs: string[]) =>
  act(() => {
    for (const slug of slugs) useRecentWorkspaces.getState().visit(slug);
  });

describe("the recent workspaces", () => {
  it("put the latest visit first", () => {
    visit("alpha", "bravo", "charlie");
    expect(recent()).toEqual(["charlie", "bravo", "alpha"]);
  });

  it("keep each slug once, moved to the front when visited again", () => {
    visit("alpha", "bravo", "alpha");
    expect(recent()).toEqual(["alpha", "bravo"]);
  });

  it(`keep at most ${RECENT_WORKSPACES_LIMIT}, dropping the oldest`, () => {
    visit("a1", "a2", "a3", "a4", "a5", "a6");
    expect(recent()).toEqual(["a6", "a5", "a4", "a3", "a2"]);
  });

  it("forget a slug", () => {
    visit("alpha", "bravo");
    act(() => useRecentWorkspaces.getState().forget("alpha"));
    expect(recent()).toEqual(["bravo"]);
  });

  it("refuse what is no slug", () => {
    visit("alpha", "Not A Slug", "", "../etc");
    expect(recent()).toEqual(["alpha"]);
  });

  it("persist the slugs, and nothing else, under their own key", () => {
    visit("alpha");
    const stored = JSON.parse(localStorage.getItem(RECENT_WORKSPACES_STORAGE_KEY) ?? "null");
    expect(stored.state).toEqual({ slugs: ["alpha"] });
  });

  it.each([
    ["no JSON at all", "{not json"],
    ["no list", JSON.stringify({ state: { slugs: "alpha" }, version: 0 })],
    ["no state", JSON.stringify(null)],
  ])("ignore stored data that is %s", async (_case, raw) => {
    localStorage.setItem(RECENT_WORKSPACES_STORAGE_KEY, raw);
    await act(() => useRecentWorkspaces.persist.rehydrate());
    expect(recent()).toEqual([]);
  });

  it("keep only the slugs of a stored list, once each, and at most the limit", async () => {
    const slugs = ["alpha", 3, "alpha", "BAD", "bravo", "c", "d", "e", "f", "g"];
    localStorage.setItem(RECENT_WORKSPACES_STORAGE_KEY, JSON.stringify({ state: { slugs }, version: 0 }));
    await act(() => useRecentWorkspaces.persist.rehydrate());
    expect(recent()).toEqual(["alpha", "bravo", "c", "d", "e"]);
    expect(readRecentSlugs(slugs)).toEqual(recent());
  });
});

describe("a visit", () => {
  it("is recorded once the workspace has loaded, under the slug the server gave", async () => {
    fakeApi({ "GET /workspaces/ctf-2026-cd34": { status: 200, data: workspaceResponse() } });
    const client = testQueryClient();

    renderHook(() => useRecordWorkspaceVisit("ctf-2026-cd34"), { wrapper: withClient(client) });

    await waitFor(() => expect(recent()).toEqual(["ctf-2026-cd34"]));
  });

  it("is not recorded without a slug, nor for a workspace that does not load", async () => {
    fakeApi({ "GET /workspaces/gone-ab12": { status: 404, data: { error: "workspace_not_found" } } });
    const client = testQueryClient();

    const { result } = renderHook(() => useRecordWorkspaceVisit("gone-ab12"), { wrapper: withClient(client) });
    renderHook(() => useRecordWorkspaceVisit(undefined), { wrapper: withClient(client) });

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(recent()).toEqual([]);
  });
});

describe("a recent workspace leaves the list", () => {
  it("when it answers 404", async () => {
    visit("gone-ab12", "kept-cd34");
    fakeApi({ "GET /workspaces/gone-ab12": { status: 404, data: { error: "workspace_not_found" } } });
    const client = testQueryClient();

    const { result } = renderHook(() => useWorkspace("gone-ab12"), { wrapper: withClient(client) });

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(recent()).toEqual(["kept-cd34"]);
  });

  it("but not on another failure", async () => {
    visit("busy-ab12");
    fakeApi({ "GET /workspaces/busy-ab12": { status: 503, data: { error: "service_unavailable" } } });
    const client = testQueryClient();

    const { result } = renderHook(() => useWorkspace("busy-ab12"), { wrapper: withClient(client) });

    // Anything but a 404 gets one second chance, a second later.
    await waitFor(() => expect(result.current.isError).toBe(true), { timeout: 3000 });
    expect(recent()).toEqual(["busy-ab12"]);
  });

  it.each([
    ["left", useLeaveWorkspace, "POST /workspaces/ctf-2026-cd34/leave"],
    ["deleted", useDeleteWorkspace, "DELETE /workspaces/ctf-2026-cd34"],
  ])("when it is %s", async (_case, hook, route) => {
    visit("ctf-2026-cd34", "kept-cd34");
    fakeApi({ [route]: { status: 204 } });
    const client = testQueryClient();
    const { result } = renderHook(hook, { wrapper: withClient(client) });

    await act(() => result.current.mutateAsync("ctf-2026-cd34"));

    expect(recent()).toEqual(["kept-cd34"]);
  });

  it.each([
    ["its team is deleted", "delete"],
    ["the caller leaves its team", "leave"],
  ])("when %s", async (_case, how) => {
    const me = "01900000-0000-7000-8000-0000000000ee";
    visit("solo-ab12", "ctf-2026-cd34");
    fakeApi({
      "DELETE /teams/red-team-ab12": { status: 204 },
      [`DELETE /teams/red-team-ab12/members/${me}`]: { status: 204 },
    });
    const client = testQueryClient();
    client.setQueryData(AUTH_ME_QUERY_KEY, { id: me });
    client.setQueryData(workspaceKeys.list(), [
      workspaceSummary({ slug: "ctf-2026-cd34", team: { id: "t", slug: "red-team-ab12", name: "Red", color: "rose" } }),
      workspaceSummary({ slug: "solo-ab12", mode: "solo", team: null }),
    ]);

    if (how === "delete") {
      const { result } = renderHook(useDeleteTeam, { wrapper: withClient(client) });
      await act(() => result.current.mutateAsync("red-team-ab12"));
    } else {
      const { result } = renderHook(useRemoveTeamMember, { wrapper: withClient(client) });
      await act(() => result.current.mutateAsync({ slug: "red-team-ab12", userId: me }));
    }

    expect(recent()).toEqual(["solo-ab12"]);
  });
});
