// src/api/invitations.ts
import { api } from "@/lib/api";
import type {
  CreateInvitationRequest,
  CreateWorkspaceInvitationRequest,
  CreatedTeamInvitation,
  InvitedByQuery,
  TeamInvitationPreview,
  TeamInvitationResponse,
  TeamResponse,
  WorkspaceInvitationPreview,
  WorkspaceInvitationResponse,
  WorkspaceResponse,
} from "@/lib/api/types";
import { segment, withInvitedBy } from "@/api/paths";

/** What an invitation is to: a team, or one workspace as a guest. */
export type InvitationKind = "team" | "workspace";

/**
 * Query keys for the invitee's side of invitations. The Admins' lists hang
 * off the entity they belong to, `teamKeys.invitations(slug)` and
 * `workspaceKeys.invitations(slug)`, so that removing a team or a workspace
 * takes them along.
 *
 * - `all`: every invitee-side query.
 * - `pending(kind)`: `GET /teams/invitations/pending` or
 *   `GET /workspaces/invitations/pending`, the caller's own.
 * - `preview(kind, token)`: what a mailed link offers, before it is accepted.
 */
export const invitationKeys = {
  all: ["invitations"] as const,
  pending: (kind: InvitationKind) => [...invitationKeys.all, "pending", kind] as const,
  preview: (kind: InvitationKind, token: string) =>
    [...invitationKeys.all, "preview", kind, token] as const,
};

// ---- Teams, the Admins' side ----------------------------------------------

/** The team's pending invitations, whatever they name. Admins and the Captain. */
export async function listTeamInvitations(
  slug: string,
  query?: InvitedByQuery,
): Promise<TeamInvitationResponse[]> {
  const { data } = await api.get<TeamInvitationResponse[]>(
    withInvitedBy(`/teams/${segment(slug)}/invitations`, query),
  );
  return data;
}

/**
 * Invites someone by username or address, or makes a generic link when the
 * body names nobody. Only a generic link's answer carries its token, once:
 * no listing shows it again.
 */
export async function createTeamInvitation(
  slug: string,
  body: CreateInvitationRequest,
): Promise<CreatedTeamInvitation> {
  const { data } = await api.post<CreatedTeamInvitation>(
    `/teams/${segment(slug)}/invitations`,
    body,
  );
  return data;
}

export async function revokeTeamInvitation(slug: string, invitationId: string): Promise<void> {
  await api.delete(`/teams/${segment(slug)}/invitations/${segment(invitationId)}`);
}

// ---- Teams, the invitee's side --------------------------------------------

/** The invitations addressed to the caller, by account or by verified address. */
export async function listPendingTeamInvitations(): Promise<TeamInvitationResponse[]> {
  const { data } = await api.get<TeamInvitationResponse[]>("/teams/invitations/pending");
  return data;
}

/** Joins the team an invitation addressed to the caller names. */
export async function acceptTeamInvitation(invitationId: string): Promise<TeamResponse> {
  const { data } = await api.post<TeamResponse>(
    `/teams/invitations/${segment(invitationId)}/accept`,
  );
  return data;
}

export async function rejectTeamInvitation(invitationId: string): Promise<void> {
  await api.post(`/teams/invitations/${segment(invitationId)}/reject`);
}

/**
 * What a link offers before it is followed. The token travels in the body,
 * never in the URL, to stay out of the access logs.
 */
export async function previewTeamInvitation(token: string): Promise<TeamInvitationPreview> {
  const { data } = await api.post<TeamInvitationPreview>("/teams/invitations/preview", {
    token,
  });
  return data;
}

/** Joins by a link's token: a generic link, or a mailed one for the caller. */
export async function joinTeam(token: string): Promise<TeamResponse> {
  const { data } = await api.post<TeamResponse>("/teams/join", { token });
  return data;
}

// ---- Workspaces, the inviting side -----------------------------------------

/**
 * The workspace's invitations, every status, newest first: all of them for
 * the Captain and the Admins, their own for any other member who may invite.
 */
export async function listWorkspaceInvitations(
  slug: string,
  query?: InvitedByQuery,
): Promise<WorkspaceInvitationResponse[]> {
  const { data } = await api.get<WorkspaceInvitationResponse[]>(
    withInvitedBy(`/workspaces/${segment(slug)}/invitations`, query),
  );
  return data;
}

/** Invites one account or one address as a guest. The answer never holds a token. */
export async function createWorkspaceInvitation(
  slug: string,
  body: CreateWorkspaceInvitationRequest,
): Promise<WorkspaceInvitationResponse> {
  const { data } = await api.post<WorkspaceInvitationResponse>(
    `/workspaces/${segment(slug)}/invitations`,
    body,
  );
  return data;
}

/** Withdraws a pending invitation: its author, the Captain or an Admin. */
export async function cancelWorkspaceInvitation(slug: string, invitationId: string): Promise<void> {
  await api.delete(`/workspaces/${segment(slug)}/invitations/${segment(invitationId)}`);
}

// ---- Workspaces, the invitee's side ----------------------------------------

export async function listPendingWorkspaceInvitations(): Promise<WorkspaceInvitationResponse[]> {
  const { data } = await api.get<WorkspaceInvitationResponse[]>(
    "/workspaces/invitations/pending",
  );
  return data;
}

export async function acceptWorkspaceInvitation(invitationId: string): Promise<WorkspaceResponse> {
  const { data } = await api.post<WorkspaceResponse>(
    `/workspaces/invitations/${segment(invitationId)}/accept`,
  );
  return data;
}

export async function rejectWorkspaceInvitation(invitationId: string): Promise<void> {
  await api.post(`/workspaces/invitations/${segment(invitationId)}/reject`);
}

export async function previewWorkspaceInvitation(
  token: string,
): Promise<WorkspaceInvitationPreview> {
  const { data } = await api.post<WorkspaceInvitationPreview>(
    "/workspaces/invitations/preview",
    { token },
  );
  return data;
}

/** Joins by a mailed link's token, as the account it is for. */
export async function joinWorkspace(token: string): Promise<WorkspaceResponse> {
  const { data } = await api.post<WorkspaceResponse>("/workspaces/join", { token });
  return data;
}
