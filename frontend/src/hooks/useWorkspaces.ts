// src/hooks/useWorkspaces.ts
import { skipToken, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { useMatch } from "react-router";

import {
  addFavorite,
  archiveWorkspace,
  clearWorkspaceRoleOverride,
  createWorkspace,
  deleteWorkspace,
  deleteWorkspaceAvatar,
  excludeWorkspaceMember,
  getWorkspace,
  leaveWorkspace,
  liftWorkspaceExclusion,
  listWorkspaceExclusions,
  listWorkspaceMembers,
  listWorkspaces,
  removeFavorite,
  removeWorkspaceGuest,
  setWorkspaceMemberRole,
  unarchiveWorkspace,
  updateWorkspace,
  uploadWorkspaceAvatar,
  workspaceKeys,
} from "@/api/workspaces";
import type {
  CreateWorkspaceInput,
  InvitedByQuery,
  UpdateWorkspaceInput,
  WorkspaceRole,
  WorkspaceSummary,
} from "@/lib/api/types";
import { isApiError } from "@/lib/api-error";
import { retryUnlessNotFound } from "@/lib/query-client";
import { WORKSPACE_ROUTE_MATCH } from "@/lib/routes";
import { forgetRecentWorkspace, useRecentWorkspaces } from "@/stores/recent-workspaces";

// Every write below says what it changes in the cache: what it sets from its
// answer, what it marks stale, what it removes. A query nobody watches is only
// marked, and refetched the next time something reads it.

/** Every workspace the caller can see, as the server sorts them. */
export function useWorkspaces() {
  return useQuery({ queryKey: workspaceKeys.list(), queryFn: listWorkspaces });
}

/**
 * One workspace, once there is a slug to ask for: nothing is sent without one.
 * A 404, gone or out of reach, takes the slug out of the recent workspaces.
 */
export function useWorkspace(slug: string | undefined) {
  return useQuery({
    queryKey: workspaceKeys.detail(slug ?? ""),
    queryFn: slug
      ? async () => {
          try {
            return await getWorkspace(slug);
          } catch (error) {
            if (isApiError(error) && error.status === 404) forgetRecentWorkspace(slug);
            throw error;
          }
        }
      : skipToken,
    retry: retryUnlessNotFound,
  });
}

/**
 * Loads the workspace a page shows and, once it has loaded, records the visit
 * in the recent workspaces, under the slug the server answered with.
 */
export function useRecordWorkspaceVisit(slug: string | undefined) {
  const workspace = useWorkspace(slug);
  const visit = useRecentWorkspaces((state) => state.visit);
  const visited = workspace.data?.slug;

  useEffect(() => {
    if (visited) visit(visited);
  }, [visited, visit]);

  return workspace;
}

/** Everyone with access, optionally those one account brought in. */
export function useWorkspaceMembers(slug: string | undefined, query?: InvitedByQuery) {
  return useQuery({
    queryKey: workspaceKeys.members(slug ?? "", query),
    queryFn: slug ? () => listWorkspaceMembers(slug, query) : skipToken,
    retry: retryUnlessNotFound,
  });
}

/** The team members excluded from the workspace. */
export function useWorkspaceExclusions(slug: string | undefined) {
  return useQuery({
    queryKey: workspaceKeys.exclusions(slug ?? ""),
    queryFn: slug ? () => listWorkspaceExclusions(slug) : skipToken,
    retry: retryUnlessNotFound,
  });
}

/** The current workspace's slug from any nested route, or `null` outside one. */
export function useWorkspaceSlug(): string | null {
  const match = useMatch(WORKSPACE_ROUTE_MATCH);
  return match?.params.slug ?? null;
}

/** Sets the new workspace's detail from the answer; the list is stale. */
export function useCreateWorkspace() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateWorkspaceInput) => createWorkspace(body),
    onSuccess: (workspace) => {
      queryClient.setQueryData(workspaceKeys.detail(workspace.slug), workspace);
      return queryClient.invalidateQueries({ queryKey: workspaceKeys.list() });
    },
  });
}

/** Sets the detail from the answer; the list is stale. */
export function useUpdateWorkspace() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ slug, patch }: { slug: string; patch: UpdateWorkspaceInput }) =>
      updateWorkspace(slug, patch),
    onSuccess: (workspace, { slug }) => {
      queryClient.setQueryData(workspaceKeys.detail(slug), workspace);
      return queryClient.invalidateQueries({ queryKey: workspaceKeys.list() });
    },
  });
}

/**
 * What leaves the caller's reach for good: its detail goes, and its slug
 * leaves the recent workspaces; the list is stale.
 */
function forgetWorkspace(queryClient: QueryClient, slug: string) {
  queryClient.removeQueries({ queryKey: workspaceKeys.detail(slug) });
  forgetRecentWorkspace(slug);
  return queryClient.invalidateQueries({ queryKey: workspaceKeys.list() });
}

/**
 * Removes the detail, and with it its members, exclusions and invitations;
 * the slug leaves the recent workspaces.
 */
export function useDeleteWorkspace() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (slug: string) => deleteWorkspace(slug),
    onSuccess: (_answer, slug) => forgetWorkspace(queryClient, slug),
  });
}

/**
 * Sets the detail from the answer; the list (its order and flag) is stale,
 * and so are the workspace's invitations, which archiving cancels.
 */
export function useArchiveWorkspace() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (slug: string) => archiveWorkspace(slug),
    onSuccess: (workspace, slug) => {
      queryClient.setQueryData(workspaceKeys.detail(slug), workspace);
      return Promise.all([
        queryClient.invalidateQueries({ queryKey: workspaceKeys.list() }),
        queryClient.invalidateQueries({ queryKey: workspaceKeys.invitations(slug) }),
      ]);
    },
  });
}

/** Sets the detail from the answer; the list is stale. Cancelled invitations stay so. */
export function useUnarchiveWorkspace() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (slug: string) => unarchiveWorkspace(slug),
    onSuccess: (workspace, slug) => {
      queryClient.setQueryData(workspaceKeys.detail(slug), workspace);
      return queryClient.invalidateQueries({ queryKey: workspaceKeys.list() });
    },
  });
}

/**
 * Favourite or not, shown at once: the cached list is changed before the
 * request leaves, put back as it was if the request fails, and refetched
 * either way for the order the server gives favourites.
 */
export function useSetFavorite() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ slug, favorite }: { slug: string; favorite: boolean }) =>
      favorite ? addFavorite(slug) : removeFavorite(slug),
    onMutate: async ({ slug, favorite }) => {
      await queryClient.cancelQueries({ queryKey: workspaceKeys.list() });
      const previous = queryClient.getQueryData<WorkspaceSummary[]>(workspaceKeys.list());
      queryClient.setQueryData<WorkspaceSummary[]>(workspaceKeys.list(), (list) =>
        list?.map((workspace) => (workspace.slug === slug ? { ...workspace, favorite } : workspace)),
      );
      return { previous };
    },
    onError: (_error, _variables, context) => {
      if (context?.previous) {
        queryClient.setQueryData(workspaceKeys.list(), context.previous);
      }
    },
    onSettled: () => queryClient.invalidateQueries({ queryKey: workspaceKeys.list() }),
  });
}

type MemberTarget = { slug: string; userId: string };

/** The members are stale. */
export function useSetWorkspaceMemberRole() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ slug, userId, role }: MemberTarget & { role: WorkspaceRole }) =>
      setWorkspaceMemberRole(slug, userId, { role }),
    onSuccess: (_answer, { slug }) =>
      queryClient.invalidateQueries({ queryKey: workspaceKeys.members(slug) }),
  });
}

/** The members are stale. */
export function useClearWorkspaceRoleOverride() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ slug, userId }: MemberTarget) => clearWorkspaceRoleOverride(slug, userId),
    onSuccess: (_answer, { slug }) =>
      queryClient.invalidateQueries({ queryKey: workspaceKeys.members(slug) }),
  });
}

/** The members and the exclusions are stale. */
export function useExcludeWorkspaceMember() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ slug, userId }: MemberTarget) => excludeWorkspaceMember(slug, userId),
    onSuccess: (_answer, { slug }) =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: workspaceKeys.members(slug) }),
        queryClient.invalidateQueries({ queryKey: workspaceKeys.exclusions(slug) }),
      ]),
  });
}

/** The members and the exclusions are stale. */
export function useLiftWorkspaceExclusion() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ slug, userId }: MemberTarget) => liftWorkspaceExclusion(slug, userId),
    onSuccess: (_answer, { slug }) =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: workspaceKeys.members(slug) }),
        queryClient.invalidateQueries({ queryKey: workspaceKeys.exclusions(slug) }),
      ]),
  });
}

/** The members are stale. */
export function useRemoveWorkspaceGuest() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ slug, userId }: MemberTarget) => removeWorkspaceGuest(slug, userId),
    onSuccess: (_answer, { slug }) =>
      queryClient.invalidateQueries({ queryKey: workspaceKeys.members(slug) }),
  });
}

/**
 * The workspace leaves the caller's reach: its detail goes, its slug leaves
 * the recent workspaces, the list is stale.
 */
export function useLeaveWorkspace() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (slug: string) => leaveWorkspace(slug),
    onSuccess: (_answer, slug) => forgetWorkspace(queryClient, slug),
  });
}

/** The detail and the list both show the avatar. */
function invalidateWorkspaceAvatar(queryClient: QueryClient, slug: string) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: workspaceKeys.detail(slug), exact: true }),
    queryClient.invalidateQueries({ queryKey: workspaceKeys.list() }),
  ]);
}

export function useUploadWorkspaceAvatar() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ workspaceId, image }: { workspaceId: string; slug: string; image: Blob }) =>
      uploadWorkspaceAvatar(workspaceId, image),
    onSuccess: (_answer, { slug }) => invalidateWorkspaceAvatar(queryClient, slug),
  });
}

export function useDeleteWorkspaceAvatar() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ workspaceId }: { workspaceId: string; slug: string }) =>
      deleteWorkspaceAvatar(workspaceId),
    onSuccess: (_answer, { slug }) => invalidateWorkspaceAvatar(queryClient, slug),
  });
}
