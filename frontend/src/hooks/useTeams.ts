// src/hooks/useTeams.ts
import { skipToken, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { QueryClient } from "@tanstack/react-query";

import { AUTH_ME_QUERY_KEY } from "@/api/auth";
import {
  changeTeamMemberRole,
  createTeam,
  deleteTeam,
  deleteTeamAvatar,
  getTeam,
  listTeamMembers,
  listTeams,
  removeTeamMember,
  teamKeys,
  transferCaptaincy,
  updateTeam,
  uploadTeamAvatar,
} from "@/api/teams";
import { workspaceKeys } from "@/api/workspaces";
import type {
  CreateTeamRequest,
  InvitedByQuery,
  TeamPatch,
  TeamRole,
  WorkspaceSummary,
} from "@/lib/api/types";
import { retryUnlessNotFound } from "@/lib/query-client";
import { forgetRecentWorkspace } from "@/stores/recent-workspaces";
import type { User } from "@/types/api";

// Every write below says what it changes in the cache: what it sets from its
// answer, what it marks stale, what it removes. A query nobody watches is only
// marked, and refetched the next time something reads it.

/** The caller's teams. */
export function useTeams() {
  return useQuery({ queryKey: teamKeys.list(), queryFn: listTeams });
}

/** One team, once there is a slug to ask for: nothing is sent without one. */
export function useTeam(slug: string | undefined) {
  return useQuery({
    queryKey: teamKeys.detail(slug ?? ""),
    queryFn: slug ? () => getTeam(slug) : skipToken,
    retry: retryUnlessNotFound,
  });
}

/** The team's members, optionally those one account brought in. */
export function useTeamMembers(slug: string | undefined, query?: InvitedByQuery) {
  return useQuery({
    queryKey: teamKeys.members(slug ?? "", query),
    queryFn: slug ? () => listTeamMembers(slug, query) : skipToken,
    retry: retryUnlessNotFound,
  });
}

/** Sets the new team's detail from the answer; the list is stale. */
export function useCreateTeam() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateTeamRequest) => createTeam(body),
    onSuccess: (team) => {
      queryClient.setQueryData(teamKeys.detail(team.slug), team);
      return queryClient.invalidateQueries({ queryKey: teamKeys.list() });
    },
  });
}

/**
 * Sets the detail from the answer. The list is stale, and so is the
 * workspace list, which shows each workspace's team.
 */
export function useUpdateTeam() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ slug, patch }: { slug: string; patch: TeamPatch }) => updateTeam(slug, patch),
    onSuccess: (team, { slug }) => {
      queryClient.setQueryData(teamKeys.detail(slug), team);
      return Promise.all([
        queryClient.invalidateQueries({ queryKey: teamKeys.list() }),
        queryClient.invalidateQueries({ queryKey: workspaceKeys.list() }),
      ]);
    },
  });
}

/**
 * Forgets what the cache and the recent workspaces hold of the team's
 * workspaces, found in the cached workspace list: they are gone with the
 * team, or out of the caller's reach once they leave it.
 */
function forgetTeamWorkspaces(queryClient: QueryClient, slug: string) {
  const workspaces = queryClient.getQueryData<WorkspaceSummary[]>(workspaceKeys.list()) ?? [];
  for (const workspace of workspaces) {
    if (workspace.team?.slug === slug) {
      queryClient.removeQueries({ queryKey: workspaceKeys.detail(workspace.slug) });
      forgetRecentWorkspace(workspace.slug);
    }
  }
}

/**
 * Removes the team's detail, and with it its members and invitations, and
 * the detail of each of its workspaces, deleted with it. Both lists are stale.
 */
export function useDeleteTeam() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (slug: string) => deleteTeam(slug),
    onSuccess: (_answer, slug) => {
      forgetTeamWorkspaces(queryClient, slug);
      queryClient.removeQueries({ queryKey: teamKeys.detail(slug) });
      return Promise.all([
        queryClient.invalidateQueries({ queryKey: teamKeys.list() }),
        queryClient.invalidateQueries({ queryKey: workspaceKeys.list() }),
      ]);
    },
  });
}

/**
 * The team's detail, its members and the team list are stale: the caller's
 * own role may be the one that changed. So is every workspace query, since a
 * team role is what its workspaces inherit.
 */
export function useChangeTeamMemberRole() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ slug, userId, role }: { slug: string; userId: string; role: TeamRole }) =>
      changeTeamMemberRole(slug, userId, { role }),
    onSuccess: (_member, { slug }) =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: teamKeys.detail(slug), exact: true }),
        queryClient.invalidateQueries({ queryKey: teamKeys.members(slug) }),
        queryClient.invalidateQueries({ queryKey: teamKeys.list() }),
        queryClient.invalidateQueries({ queryKey: workspaceKeys.all }),
      ]),
  });
}

/**
 * Someone else removed: the team's detail, its members, the team list (the
 * count) and every workspace query (their rosters) are stale. The caller
 * themselves, which is leaving the team: its detail goes, with what the cache
 * holds of its workspaces, and both lists are stale.
 */
export function useRemoveTeamMember() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ slug, userId }: { slug: string; userId: string }) =>
      removeTeamMember(slug, userId),
    onSuccess: (_answer, { slug, userId }) => {
      const me = queryClient.getQueryData<User | null>(AUTH_ME_QUERY_KEY);
      if (me?.id === userId) {
        forgetTeamWorkspaces(queryClient, slug);
        queryClient.removeQueries({ queryKey: teamKeys.detail(slug) });
        return Promise.all([
          queryClient.invalidateQueries({ queryKey: teamKeys.list() }),
          queryClient.invalidateQueries({ queryKey: workspaceKeys.list() }),
        ]);
      }
      return Promise.all([
        queryClient.invalidateQueries({ queryKey: teamKeys.detail(slug), exact: true }),
        queryClient.invalidateQueries({ queryKey: teamKeys.members(slug) }),
        queryClient.invalidateQueries({ queryKey: teamKeys.list() }),
        queryClient.invalidateQueries({ queryKey: workspaceKeys.all }),
      ]);
    },
  });
}

/**
 * Two roles change, the caller's among them: the team's detail, its members,
 * the team list and every workspace query are stale.
 */
export function useTransferCaptaincy() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ slug, userId }: { slug: string; userId: string }) =>
      transferCaptaincy(slug, { user_id: userId }),
    onSuccess: (_answer, { slug }) =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: teamKeys.detail(slug), exact: true }),
        queryClient.invalidateQueries({ queryKey: teamKeys.members(slug) }),
        queryClient.invalidateQueries({ queryKey: teamKeys.list() }),
        queryClient.invalidateQueries({ queryKey: workspaceKeys.all }),
      ]),
  });
}

/** The team's detail, the team list and the workspace list all show its avatar. */
function invalidateTeamAvatar(queryClient: QueryClient, slug: string) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: teamKeys.detail(slug), exact: true }),
    queryClient.invalidateQueries({ queryKey: teamKeys.list() }),
    queryClient.invalidateQueries({ queryKey: workspaceKeys.list() }),
  ]);
}

export function useUploadTeamAvatar() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ teamId, image }: { teamId: string; slug: string; image: Blob }) =>
      uploadTeamAvatar(teamId, image),
    onSuccess: (_answer, { slug }) => invalidateTeamAvatar(queryClient, slug),
  });
}

export function useDeleteTeamAvatar() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ teamId }: { teamId: string; slug: string }) => deleteTeamAvatar(teamId),
    onSuccess: (_answer, { slug }) => invalidateTeamAvatar(queryClient, slug),
  });
}
