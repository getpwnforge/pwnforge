// src/lib/api/types.ts
import type { components, operations } from "@/lib/api/schema.gen";

/**
 * Named shortcuts into the types generated from the backend's OpenAPI spec.
 *
 * Aliases only, never shapes written out again: a field the backend adds,
 * renames or drops reaches every use through `schema.gen.ts` alone, which
 * `make api-types` regenerates from `backend/openapi.json`.
 */
type Schemas = components["schemas"];

/**
 * `?invited_by=<user id>` on the member and invitation lists: only the rows
 * that account brought in. Read off one operation; every list taking it
 * declares the same parameter.
 */
export type InvitedByQuery = NonNullable<operations["list_team_members"]["parameters"]["query"]>;

// Accounts and setup
export type UserResponse = Schemas["UserResponse"];
export type LoginRequest = Schemas["LoginRequest"];
export type RegisterRequest = Schemas["RegisterRequest"];
export type PasswordForgotRequest = Schemas["PasswordForgotRequest"];
export type PasswordResetRequest = Schemas["PasswordResetRequest"];
export type EmailResendRequest = Schemas["EmailResendRequest"];
export type SetupStatusResponse = Schemas["SetupStatusResponse"];
export type SetupRequest = Schemas["SetupRequest"];
export type EmailConfigResponse = Schemas["EmailConfigResponse"];

// Workspaces
export type WorkspaceSummary = Schemas["WorkspaceSummary"];
export type WorkspaceResponse = Schemas["WorkspaceResponse"];
export type CreateWorkspaceInput = Schemas["CreateWorkspaceInput"];
export type UpdateWorkspaceInput = Schemas["UpdateWorkspaceInput"];
/** Body of `PATCH /workspaces/{slug}/members/{user_id}`. */
export type SetMemberRoleRequest = Schemas["SetMemberRoleRequest"];
/** A role one workspace can give: every team role but Captain. */
export type WorkspaceRole = Schemas["WorkspaceRole"];
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
export type WriteupVisibility = Schemas["WriteupVisibility"];

// Teams
export type TeamResponse = Schemas["TeamResponse"];
export type CreateTeamRequest = Schemas["CreateTeamRequest"];
export type TeamPatch = Schemas["TeamPatch"];
/** A team as a workspace names it: its monogram, nothing about its members. */
export type TeamSummary = Schemas["TeamSummary"];
export type TeamMemberResponse = Schemas["TeamMemberResponse"];
export type TeamRole = Schemas["TeamRole"];
export type ChangeMemberRoleRequest = Schemas["ChangeMemberRoleRequest"];
export type TransferCaptaincyRequest = Schemas["TransferCaptaincyRequest"];

// Invitations
export type InvitationTarget = Schemas["InvitationTarget"];
/** The token a mailed link or a generic team link carries. */
export type InvitationTokenRequest = Schemas["InvitationTokenRequest"];
/** Body of `POST /teams/join`. */
export type JoinTeamRequest = Schemas["JoinTeamRequest"];
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
