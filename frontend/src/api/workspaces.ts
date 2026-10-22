// src/api/workspaces.ts
import { api } from "@/lib/api";
import type {
  CreateWorkspaceInput,
  EffectiveMember,
  ExcludedMember,
  InvitedByQuery,
  SetMemberRoleRequest,
  UpdateWorkspaceInput,
  WorkspaceResponse,
  WorkspaceSummary,
} from "@/lib/api/types";
import { segment, withInvitedBy } from "@/api/paths";

/**
 * Query keys for workspaces, from the most general to the most precise, so
 * that one prefix reaches everything under it: removing `detail(slug)` takes
 * the workspace's members, exclusions and invitations along.
 *
 * - `all`: every workspace query.
 * - `list()`: `GET /workspaces`, what the caller can see.
 * - `detail(slug)`: `GET /workspaces/{slug}`.
 * - `members(slug, query?)`: `GET /workspaces/{slug}/members`, one entry per filter.
 * - `exclusions(slug)`: `GET /workspaces/{slug}/exclusions`.
 * - `invitations(slug, query?)`: `GET /workspaces/{slug}/invitations`.
 */
export const workspaceKeys = {
  all: ["workspaces"] as const,
  list: () => [...workspaceKeys.all, "list"] as const,
  detail: (slug: string) => [...workspaceKeys.all, "detail", slug] as const,
  members: (slug: string, query?: InvitedByQuery) =>
    query?.invited_by
      ? ([...workspaceKeys.detail(slug), "members", query] as const)
      : ([...workspaceKeys.detail(slug), "members"] as const),
  exclusions: (slug: string) => [...workspaceKeys.detail(slug), "exclusions"] as const,
  invitations: (slug: string, query?: InvitedByQuery) =>
    query?.invited_by
      ? ([...workspaceKeys.detail(slug), "invitations", query] as const)
      : ([...workspaceKeys.detail(slug), "invitations"] as const),
};

/** Every workspace the caller can see: favourites first, archived last, then by name. */
export async function listWorkspaces(): Promise<WorkspaceSummary[]> {
  const { data } = await api.get<WorkspaceSummary[]>("/workspaces");
  return data;
}

/** One workspace. No access is a 404, as a missing one. */
export async function getWorkspace(slug: string): Promise<WorkspaceResponse> {
  const { data } = await api.get<WorkspaceResponse>(`/workspaces/${segment(slug)}`);
  return data;
}

/** Creates a solo workspace, or one under a team the caller is an Admin of. */
export async function createWorkspace(body: CreateWorkspaceInput): Promise<WorkspaceResponse> {
  const { data } = await api.post<WorkspaceResponse>("/workspaces", body);
  return data;
}

/** Changes what a PATCH may change: never the mode, the team or the owner. */
export async function updateWorkspace(
  slug: string,
  patch: UpdateWorkspaceInput,
): Promise<WorkspaceResponse> {
  const { data } = await api.patch<WorkspaceResponse>(`/workspaces/${segment(slug)}`, patch);
  return data;
}

/** Deletes the workspace. Its Captain, or the owner of a solo one. */
export async function deleteWorkspace(slug: string): Promise<void> {
  await api.delete(`/workspaces/${segment(slug)}`);
}

/** Makes the workspace read-only; its pending invitations are cancelled. */
export async function archiveWorkspace(slug: string): Promise<WorkspaceResponse> {
  const { data } = await api.post<WorkspaceResponse>(`/workspaces/${segment(slug)}/archive`);
  return data;
}

export async function unarchiveWorkspace(slug: string): Promise<WorkspaceResponse> {
  const { data } = await api.post<WorkspaceResponse>(`/workspaces/${segment(slug)}/unarchive`);
  return data;
}

/** Marks the workspace a favourite of the caller. Idempotent. */
export async function addFavorite(slug: string): Promise<void> {
  await api.post(`/workspaces/${segment(slug)}/favorite`);
}

export async function removeFavorite(slug: string): Promise<void> {
  await api.delete(`/workspaces/${segment(slug)}/favorite`);
}

/** Everyone with access, with their role here and where it comes from. */
export async function listWorkspaceMembers(
  slug: string,
  query?: InvitedByQuery,
): Promise<EffectiveMember[]> {
  const { data } = await api.get<EffectiveMember[]>(
    withInvitedBy(`/workspaces/${segment(slug)}/members`, query),
  );
  return data;
}

/** A team member's role here, overriding the team's, or a guest's role. Answers no body. */
export async function setWorkspaceMemberRole(
  slug: string,
  userId: string,
  body: SetMemberRoleRequest,
): Promise<void> {
  await api.patch(`/workspaces/${segment(slug)}/members/${segment(userId)}`, body);
}

/** A team member back to their team role. */
export async function clearWorkspaceRoleOverride(slug: string, userId: string): Promise<void> {
  await api.delete(`/workspaces/${segment(slug)}/members/${segment(userId)}/role-override`);
}

/** A team member out of this workspace, their team membership kept. */
export async function excludeWorkspaceMember(slug: string, userId: string): Promise<void> {
  await api.post(`/workspaces/${segment(slug)}/members/${segment(userId)}/exclusion`);
}

/** An exclusion lifted: the member is back on their team role. */
export async function liftWorkspaceExclusion(slug: string, userId: string): Promise<void> {
  await api.delete(`/workspaces/${segment(slug)}/members/${segment(userId)}/exclusion`);
}

/** A guest out of the workspace. */
export async function removeWorkspaceGuest(slug: string, userId: string): Promise<void> {
  await api.delete(`/workspaces/${segment(slug)}/guests/${segment(userId)}`);
}

/** The caller out of the workspace: a guest's place goes, a team member is excluded. */
export async function leaveWorkspace(slug: string): Promise<void> {
  await api.post(`/workspaces/${segment(slug)}/leave`);
}

/** The team members excluded from the workspace. The Captain and the Admins. */
export async function listWorkspaceExclusions(slug: string): Promise<ExcludedMember[]> {
  const { data } = await api.get<ExcludedMember[]>(`/workspaces/${segment(slug)}/exclusions`);
  return data;
}

/**
 * Replaces the workspace's avatar with `image`, sent as raw bytes, as
 * `uploadTeamAvatar` does and for the same reason returns nothing.
 */
export async function uploadWorkspaceAvatar(workspaceId: string, image: Blob): Promise<void> {
  await api.post(`/avatars/workspaces/${segment(workspaceId)}`, image, {
    headers: { "Content-Type": image.type || "application/octet-stream" },
  });
}

/** Clears the workspace's avatar. */
export async function deleteWorkspaceAvatar(workspaceId: string): Promise<void> {
  await api.delete(`/avatars/workspaces/${segment(workspaceId)}`);
}
