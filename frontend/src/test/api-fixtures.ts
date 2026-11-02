// src/test/api-fixtures.ts
import type {
  TeamResponse,
  WorkspaceResponse,
  WorkspaceSummary,
} from "@/lib/api/types";

// Complete answers, typed by the generated schema, with only what a test
// cares about given: a field the backend adds fails the typecheck here first.

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
