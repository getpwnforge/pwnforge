// src/test/api-fixtures.ts
import type {
  EffectiveMember,
  ExcludedMember,
  TeamInvitationResponse,
  TeamMemberResponse,
  TeamResponse,
  TeamSummary,
  UserResponse,
  WorkspaceInvitationResponse,
  WorkspaceResponse,
  WorkspaceSummary,
} from "@/lib/api/types";

// Complete answers, typed by the generated schema, with only what a test
// cares about given: a field the backend adds fails the typecheck here first.

/** The account the tests sign in as. */
export const ME_ID = "0000000f-0000-7000-8000-000000000001";

export function userResponse(overrides: Partial<UserResponse> = {}): UserResponse {
  return {
    id: ME_ID,
    username: "enzo",
    email: "enzo@example.test",
    email_verified: true,
    first_name: "Enzo",
    last_name: "P",
    avatar_version: null,
    created_at: "2026-09-01T10:00:00Z",
    ...overrides,
  };
}

export function teamResponse(overrides: Partial<TeamResponse> = {}): TeamResponse {
  return {
    id: "01900000-0000-7000-8000-000000000001",
    slug: "red-team-ab12",
    name: "Red Team",
    color: "rose",
    avatar_version: null,
    created_at: "2026-10-01T10:00:00Z",
    my_role: "captain",
    member_count: 3,
    description: null,
    members_can_create_workspaces: false,
    ...overrides,
  };
}

/** A team as a workspace names it. */
export function teamSummary(overrides: Partial<TeamSummary> = {}): TeamSummary {
  return {
    id: "01900000-0000-7000-8000-000000000001",
    slug: "red-team-ab12",
    name: "Red Team",
    color: "rose",
    avatar_version: null,
    ...overrides,
  };
}

export function teamMember(overrides: Partial<TeamMemberResponse> = {}): TeamMemberResponse {
  return {
    user_id: ME_ID,
    username: "enzo",
    first_name: "Enzo",
    last_name: "P",
    avatar_version: null,
    role: "captain",
    joined_at: "2026-09-01T10:00:00Z",
    invited_by: null,
    ...overrides,
  };
}

export function workspaceSummary(overrides: Partial<WorkspaceSummary> = {}): WorkspaceSummary {
  return {
    id: "01900000-0000-7000-8000-000000000101",
    slug: "ctf-2026-cd34",
    name: "CTF 2026",
    color: "blue",
    avatar_version: null,
    type: "event",
    mode: "team",
    archived: false,
    favorite: false,
    starts_at: null,
    ends_at: null,
    platform: null,
    my_role: "admin",
    team: null,
    ...overrides,
  };
}

export function workspaceResponse(overrides: Partial<WorkspaceResponse> = {}): WorkspaceResponse {
  return {
    id: "01900000-0000-7000-8000-000000000101",
    slug: "ctf-2026-cd34",
    name: "CTF 2026",
    description: null,
    color: "blue",
    avatar_version: null,
    type: "event",
    platform: null,
    mode: "team",
    team_id: "01900000-0000-7000-8000-000000000001",
    starts_at: null,
    ends_at: null,
    lock_after_end: false,
    default_writeup_visibility: "team",
    archived: false,
    created_at: "2026-10-01T10:00:00Z",
    my_role: "admin",
    ...overrides,
  };
}

export function effectiveMember(overrides: Partial<EffectiveMember> = {}): EffectiveMember {
  return {
    user_id: ME_ID,
    handle: "enzo",
    first_name: "Enzo",
    last_name: "P",
    avatar_version: null,
    role: "captain",
    role_source: "team",
    is_guest: false,
    joined_at: "2026-09-01T10:00:00Z",
    invited_by: null,
    ...overrides,
  };
}

export function excludedMember(overrides: Partial<ExcludedMember> = {}): ExcludedMember {
  return {
    user_id: "0000000f-0000-7000-8000-000000000009",
    handle: "nora",
    first_name: "Nora",
    last_name: "",
    avatar_version: null,
    ...overrides,
  };
}

export function teamInvitation(overrides: Partial<TeamInvitationResponse> = {}): TeamInvitationResponse {
  return {
    id: "0000000e-0000-7000-8000-000000000001",
    created_at: "2026-10-28T10:00:00Z",
    expires_at: "2026-11-04T10:00:00Z",
    default_role: "member",
    target: "username",
    invited_username: "nora",
    invited_user_id: "0000000f-0000-7000-8000-000000000009",
    invited_email: null,
    invited_by: ME_ID,
    invited_by_username: "enzo",
    max_uses: 1,
    uses: 0,
    team_id: "01900000-0000-7000-8000-000000000001",
    team_name: "Red Team",
    team_color: "rose",
    team_avatar_version: null,
    ...overrides,
  };
}

export function workspaceInvitation(overrides: Partial<WorkspaceInvitationResponse> = {}): WorkspaceInvitationResponse {
  return {
    id: "0000000e-0000-7000-8000-000000000002",
    created_at: "2026-10-28T10:00:00Z",
    expires_at: "2026-11-04T10:00:00Z",
    role: "member",
    status: "pending",
    target: "username",
    team_name: "Red Team",
    invited_username: "nora",
    invited_user_id: "0000000f-0000-7000-8000-000000000009",
    invited_email: null,
    invited_by: ME_ID,
    invited_by_username: "enzo",
    workspace_id: "01900000-0000-7000-8000-000000000101",
    workspace_name: "CTF 2026",
    workspace_color: "blue",
    workspace_avatar_version: null,
    ...overrides,
  };
}
