// src/lib/api/types.ts
import type { components } from "@/lib/api/schema.gen";

/**
 * Named shortcuts into the types generated from the backend's OpenAPI spec.
 *
 * Aliases only, never shapes written out again: a field the backend adds,
 * renames or drops reaches every use through `schema.gen.ts` alone, which
 * `make api-types` regenerates from `backend/openapi.json`.
 */
type Schemas = components["schemas"];

// Workspaces
export type WorkspaceSummary = Schemas["WorkspaceSummary"];
export type WorkspaceResponse = Schemas["WorkspaceResponse"];
export type CreateWorkspaceInput = Schemas["CreateWorkspaceInput"];
export type UpdateWorkspaceInput = Schemas["UpdateWorkspaceInput"];
export type EffectiveMember = Schemas["EffectiveMember"];
/** Where a member's role comes from: owner, team, override or guest. */
export type RoleSource = Schemas["RoleSource"];
/** A team member excluded from one workspace. */
export type ExcludedMember = Schemas["ExcludedMember"];
export type WorkspaceType = Schemas["WorkspaceType"];
export type WorkspaceMode = Schemas["WorkspaceMode"];
export type Platform = Schemas["Platform"];
/** Access on one workspace: a team role, or `owner` for a solo workspace. */
export type Membership = Schemas["Membership"];
export type AccentColor = Schemas["AccentColor"];

// Teams
export type TeamResponse = Schemas["TeamResponse"];
/** A team as a workspace names it: its monogram, nothing about its members. */
export type TeamSummary = Schemas["TeamSummary"];
export type TeamMemberResponse = Schemas["TeamMemberResponse"];
export type TeamRole = Schemas["TeamRole"];

// Invitations
export type InvitationTarget = Schemas["InvitationTarget"];
/** Body of `POST /teams/{slug}/invitations`. */
export type CreateInvitationRequest = Schemas["CreateInvitationRequest"];
export type TeamInvitationResponse = Schemas["TeamInvitationResponse"];
/** A generic team link, the only answer that ever carries its token. */
export type TeamLinkResponse = Schemas["TeamLinkResponse"];
/** A generic link with its token, or a targeted invitation without one. */
export type CreatedTeamInvitation = Schemas["CreatedTeamInvitation"];
export type TeamInvitationPreview = Schemas["TeamInvitationPreview"];
export type CreateWorkspaceInvitationRequest = Schemas["CreateWorkspaceInvitationRequest"];
export type WorkspaceInvitationResponse = Schemas["WorkspaceInvitationResponse"];
export type WorkspaceInvitationPreview = Schemas["WorkspaceInvitationPreview"];
export type InvitationStatus = Schemas["InvitationStatus"];
/** The role a workspace invitation grants: never above `member`. */
export type GuestRole = Schemas["GuestRole"];
