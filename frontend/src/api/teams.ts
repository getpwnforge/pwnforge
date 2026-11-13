// src/api/teams.ts
import { api } from "@/lib/api";
import type {
  ChangeMemberRoleRequest,
  CreateTeamRequest,
  InvitedByQuery,
  TeamMemberResponse,
  TeamPatch,
  TeamResponse,
  TransferCaptaincyRequest,
} from "@/lib/api/types";
import { segment, withInvitedBy } from "@/api/paths";

/**
 * Query keys for teams, from the most general to the most precise, so that
 * one prefix reaches everything under it: invalidating or removing
 * `detail(slug)` takes the team's members and invitations along.
 *
 * - `all`: every team query.
 * - `list()`: `GET /teams`, the caller's teams.
 * - `detail(slug)`: `GET /teams/{slug}`.
 * - `members(slug, query?)`: `GET /teams/{slug}/members`, one entry per filter.
 * - `invitations(slug, query?)`: `GET /teams/{slug}/invitations`, the Admins' list.
 */
export const teamKeys = {
  all: ["teams"] as const,
  list: () => [...teamKeys.all, "list"] as const,
  detail: (slug: string) => [...teamKeys.all, "detail", slug] as const,
  members: (slug: string, query?: InvitedByQuery) =>
    query?.invited_by
      ? ([...teamKeys.detail(slug), "members", query] as const)
      : ([...teamKeys.detail(slug), "members"] as const),
  invitations: (slug: string, query?: InvitedByQuery) =>
    query?.invited_by
      ? ([...teamKeys.detail(slug), "invitations", query] as const)
      : ([...teamKeys.detail(slug), "invitations"] as const),
};

/** The caller's teams, each with their own role and its member count. */
export async function listTeams(): Promise<TeamResponse[]> {
  const { data } = await api.get<TeamResponse[]>("/teams");
  return data;
}

/** One team the caller is on. A team they are not on is a 404, as a missing one. */
export async function getTeam(slug: string): Promise<TeamResponse> {
  const { data } = await api.get<TeamResponse>(`/teams/${segment(slug)}`);
  return data;
}

/** Creates a team with the caller as its Captain. The slug is the server's. */
export async function createTeam(body: CreateTeamRequest): Promise<TeamResponse> {
  const { data } = await api.post<TeamResponse>("/teams", body);
  return data;
}

/** Renames or recolours a team. Admins and the Captain. */
export async function updateTeam(slug: string, patch: TeamPatch): Promise<TeamResponse> {
  const { data } = await api.patch<TeamResponse>(`/teams/${segment(slug)}`, patch);
  return data;
}

/** Deletes a team and every workspace under it. The Captain only. */
export async function deleteTeam(slug: string): Promise<void> {
  await api.delete(`/teams/${segment(slug)}`);
}

/** The team's members, optionally only those one account brought in. */
export async function listTeamMembers(
  slug: string,
  query?: InvitedByQuery,
): Promise<TeamMemberResponse[]> {
  const { data } = await api.get<TeamMemberResponse[]>(
    withInvitedBy(`/teams/${segment(slug)}/members`, query),
  );
  return data;
}

/** Gives a member another role. Captain is never one: it moves by transfer. */
export async function changeTeamMemberRole(
  slug: string,
  userId: string,
  body: ChangeMemberRoleRequest,
): Promise<TeamMemberResponse> {
  const { data } = await api.patch<TeamMemberResponse>(
    `/teams/${segment(slug)}/members/${segment(userId)}`,
    body,
  );
  return data;
}

/** Removes a member, or the caller themselves: that is how one leaves a team. */
export async function removeTeamMember(slug: string, userId: string): Promise<void> {
  await api.delete(`/teams/${segment(slug)}/members/${segment(userId)}`);
}

/** Names another member Captain; the Captain becomes an Admin. */
export async function transferCaptaincy(
  slug: string,
  body: TransferCaptaincyRequest,
): Promise<void> {
  await api.post(`/teams/${segment(slug)}/transfer`, body);
}

/**
 * Replaces the team's avatar with `image`, sent as raw bytes: the route takes
 * no multipart envelope and reads the format from the bytes themselves. Its
 * answer carries the new `avatar_version`, but the spec gives that body no
 * schema, so nothing is returned: the caller refetches the team instead.
 */
export async function uploadTeamAvatar(teamId: string, image: Blob): Promise<void> {
  await api.post(`/avatars/teams/${segment(teamId)}`, image, {
    headers: { "Content-Type": image.type || "application/octet-stream" },
  });
}

/** Clears the team's avatar. */
export async function deleteTeamAvatar(teamId: string): Promise<void> {
  await api.delete(`/avatars/teams/${segment(teamId)}`);
}
