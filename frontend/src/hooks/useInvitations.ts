// src/hooks/useInvitations.ts
import { skipToken, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { QueryClient } from "@tanstack/react-query";

import {
  acceptTeamInvitation,
  acceptWorkspaceInvitation,
  cancelWorkspaceInvitation,
  createTeamInvitation,
  createWorkspaceInvitation,
  invitationKeys,
  joinTeam,
  joinWorkspace,
  listPendingTeamInvitations,
  listPendingWorkspaceInvitations,
  listTeamInvitations,
  listWorkspaceInvitations,
  previewTeamInvitation,
  previewWorkspaceInvitation,
  rejectTeamInvitation,
  rejectWorkspaceInvitation,
  revokeTeamInvitation,
} from "@/api/invitations";
import { teamKeys } from "@/api/teams";
import { workspaceKeys } from "@/api/workspaces";
import type {
  CreateInvitationRequest,
  CreateWorkspaceInvitationRequest,
  InvitedByQuery,
  TeamResponse,
  WorkspaceResponse,
} from "@/lib/api/types";
import { retryUnlessNotFound } from "@/lib/query-client";

// Every write below says what it changes in the cache: what it sets from its
// answer, what it marks stale, what it removes. A query nobody watches is only
// marked, and refetched the next time something reads it.

// ---- Reads ----------------------------------------------------------------

/** The team's pending invitations, for its Admins and Captain. */
export function useTeamInvitations(slug: string | undefined, query?: InvitedByQuery) {
  return useQuery({
    queryKey: teamKeys.invitations(slug ?? "", query),
    queryFn: slug ? () => listTeamInvitations(slug, query) : skipToken,
    retry: retryUnlessNotFound,
  });
}

/** The workspace's invitations, as much of them as the caller may see. */
export function useWorkspaceInvitations(slug: string | undefined, query?: InvitedByQuery) {
  return useQuery({
    queryKey: workspaceKeys.invitations(slug ?? "", query),
    queryFn: slug ? () => listWorkspaceInvitations(slug, query) : skipToken,
    retry: retryUnlessNotFound,
  });
}

/** The team invitations addressed to the caller. */
export function usePendingTeamInvitations() {
  return useQuery({ queryKey: invitationKeys.pending("team"), queryFn: listPendingTeamInvitations });
}

/** The workspace invitations addressed to the caller. */
export function usePendingWorkspaceInvitations() {
  return useQuery({
    queryKey: invitationKeys.pending("workspace"),
    queryFn: listPendingWorkspaceInvitations,
  });
}

// A preview is read once per link, and its token is kept no longer than the
// page that shows it: `gcTime: 0` drops the entry, token and all, as soon as
// nothing watches it. A 404 is an unknown, expired or foreign link, not a
// fault worth a second try.
const PREVIEW_OPTIONS = { retry: false, staleTime: Infinity, gcTime: 0 } as const;

/** What a team link offers, once there is a token: nothing is sent without one. */
export function useTeamInvitationPreview(token: string | null | undefined) {
  return useQuery({
    queryKey: invitationKeys.preview("team", token ?? ""),
    queryFn: token ? () => previewTeamInvitation(token) : skipToken,
    ...PREVIEW_OPTIONS,
  });
}

/** What a workspace link offers, once there is a token. */
export function useWorkspaceInvitationPreview(token: string | null | undefined) {
  return useQuery({
    queryKey: invitationKeys.preview("workspace", token ?? ""),
    queryFn: token ? () => previewWorkspaceInvitation(token) : skipToken,
    ...PREVIEW_OPTIONS,
  });
}

// ---- Teams, the Admins' side ----------------------------------------------

/** The team's invitations are stale. */
export function useCreateTeamInvitation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ slug, body }: { slug: string; body: CreateInvitationRequest }) =>
      createTeamInvitation(slug, body),
    onSuccess: (_invitation, { slug }) =>
      queryClient.invalidateQueries({ queryKey: teamKeys.invitations(slug) }),
  });
}

/** The team's invitations are stale. */
export function useRevokeTeamInvitation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ slug, invitationId }: { slug: string; invitationId: string }) =>
      revokeTeamInvitation(slug, invitationId),
    onSuccess: (_answer, { slug }) =>
      queryClient.invalidateQueries({ queryKey: teamKeys.invitations(slug) }),
  });
}

// ---- Teams, the invitee's side --------------------------------------------

/**
 * On the team now: its detail is set from the answer; the team list, the
 * pending invitations and every workspace query are stale, since the team's
 * workspaces open to the caller.
 */
function joinedTeam(queryClient: QueryClient, team: TeamResponse) {
  queryClient.setQueryData(teamKeys.detail(team.slug), team);
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: teamKeys.list() }),
    queryClient.invalidateQueries({ queryKey: invitationKeys.pending("team") }),
    queryClient.invalidateQueries({ queryKey: workspaceKeys.all }),
  ]);
}

export function useAcceptTeamInvitation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (invitationId: string) => acceptTeamInvitation(invitationId),
    onSuccess: (team) => joinedTeam(queryClient, team),
  });
}

/** The pending invitations are stale. */
export function useRejectTeamInvitation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (invitationId: string) => rejectTeamInvitation(invitationId),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: invitationKeys.pending("team") }),
  });
}

/** As an acceptance, and the link's preview, token and all, leaves the cache. */
export function useJoinTeam() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (token: string) => joinTeam(token),
    onSuccess: (team, token) => {
      queryClient.removeQueries({ queryKey: invitationKeys.preview("team", token) });
      return joinedTeam(queryClient, team);
    },
  });
}

// ---- Workspaces, the inviting side -----------------------------------------

/** The workspace's invitations are stale. */
export function useCreateWorkspaceInvitation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ slug, body }: { slug: string; body: CreateWorkspaceInvitationRequest }) =>
      createWorkspaceInvitation(slug, body),
    onSuccess: (_invitation, { slug }) =>
      queryClient.invalidateQueries({ queryKey: workspaceKeys.invitations(slug) }),
  });
}

/** The workspace's invitations are stale. */
export function useCancelWorkspaceInvitation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ slug, invitationId }: { slug: string; invitationId: string }) =>
      cancelWorkspaceInvitation(slug, invitationId),
    onSuccess: (_answer, { slug }) =>
      queryClient.invalidateQueries({ queryKey: workspaceKeys.invitations(slug) }),
  });
}

// ---- Workspaces, the invitee's side ----------------------------------------

/**
 * A guest now: the workspace's detail is set from the answer; the workspace
 * list and the pending invitations are stale.
 */
function joinedWorkspace(queryClient: QueryClient, workspace: WorkspaceResponse) {
  queryClient.setQueryData(workspaceKeys.detail(workspace.slug), workspace);
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: workspaceKeys.list() }),
    queryClient.invalidateQueries({ queryKey: invitationKeys.pending("workspace") }),
  ]);
}

export function useAcceptWorkspaceInvitation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (invitationId: string) => acceptWorkspaceInvitation(invitationId),
    onSuccess: (workspace) => joinedWorkspace(queryClient, workspace),
  });
}

/** The pending invitations are stale. */
export function useRejectWorkspaceInvitation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (invitationId: string) => rejectWorkspaceInvitation(invitationId),
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: invitationKeys.pending("workspace") }),
  });
}

/** As an acceptance, and the link's preview, token and all, leaves the cache. */
export function useJoinWorkspace() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (token: string) => joinWorkspace(token),
    onSuccess: (workspace, token) => {
      queryClient.removeQueries({ queryKey: invitationKeys.preview("workspace", token) });
      return joinedWorkspace(queryClient, workspace);
    },
  });
}
