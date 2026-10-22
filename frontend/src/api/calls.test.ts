// src/api/calls.test.ts
import { describe, expect, it } from "vitest";

import * as invitations from "@/api/invitations";
import * as teams from "@/api/teams";
import * as workspaces from "@/api/workspaces";
import { fakeApi } from "@/test/fake-api";

const T = "red-team-ab12";
const W = "ctf-2026-cd34";
const USER = "01900000-0000-7000-8000-0000000000aa";
const INV = "01900000-0000-7000-8000-0000000000bb";
const ID = "01900000-0000-7000-8000-0000000000cc";
const AVATAR = new Blob([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], { type: "image/png" });

/** A call, the route it must hit, the body it must send. */
type Case = [name: string, call: () => Promise<unknown>, route: string, body: unknown];

const cases: Case[] = [
  // Teams
  ["listTeams", () => teams.listTeams(), "GET /teams", null],
  ["getTeam", () => teams.getTeam(T), `GET /teams/${T}`, null],
  ["createTeam", () => teams.createTeam({ name: "Red", color: "rose" }), "POST /teams", { name: "Red", color: "rose" }],
  ["updateTeam", () => teams.updateTeam(T, { name: "Blue" }), `PATCH /teams/${T}`, { name: "Blue" }],
  ["deleteTeam", () => teams.deleteTeam(T), `DELETE /teams/${T}`, null],
  ["listTeamMembers", () => teams.listTeamMembers(T), `GET /teams/${T}/members`, null],
  [
    "listTeamMembers by author",
    () => teams.listTeamMembers(T, { invited_by: USER }),
    `GET /teams/${T}/members?invited_by=${USER}`,
    null,
  ],
  [
    "changeTeamMemberRole",
    () => teams.changeTeamMemberRole(T, USER, { role: "viewer" }),
    `PATCH /teams/${T}/members/${USER}`,
    { role: "viewer" },
  ],
  ["removeTeamMember", () => teams.removeTeamMember(T, USER), `DELETE /teams/${T}/members/${USER}`, null],
  [
    "transferCaptaincy",
    () => teams.transferCaptaincy(T, { user_id: USER }),
    `POST /teams/${T}/transfer`,
    { user_id: USER },
  ],
  ["uploadTeamAvatar", () => teams.uploadTeamAvatar(ID, AVATAR), `POST /avatars/teams/${ID}`, AVATAR],
  ["deleteTeamAvatar", () => teams.deleteTeamAvatar(ID), `DELETE /avatars/teams/${ID}`, null],

  // Workspaces
  ["listWorkspaces", () => workspaces.listWorkspaces(), "GET /workspaces", null],
  ["getWorkspace", () => workspaces.getWorkspace(W), `GET /workspaces/${W}`, null],
  [
    "createWorkspace",
    () => workspaces.createWorkspace({ name: "CTF", color: "blue", type: "event", mode: "solo" }),
    "POST /workspaces",
    { name: "CTF", color: "blue", type: "event", mode: "solo" },
  ],
  [
    "updateWorkspace",
    () => workspaces.updateWorkspace(W, { description: null }),
    `PATCH /workspaces/${W}`,
    { description: null },
  ],
  ["deleteWorkspace", () => workspaces.deleteWorkspace(W), `DELETE /workspaces/${W}`, null],
  ["archiveWorkspace", () => workspaces.archiveWorkspace(W), `POST /workspaces/${W}/archive`, null],
  ["unarchiveWorkspace", () => workspaces.unarchiveWorkspace(W), `POST /workspaces/${W}/unarchive`, null],
  ["addFavorite", () => workspaces.addFavorite(W), `POST /workspaces/${W}/favorite`, null],
  ["removeFavorite", () => workspaces.removeFavorite(W), `DELETE /workspaces/${W}/favorite`, null],
  ["listWorkspaceMembers", () => workspaces.listWorkspaceMembers(W), `GET /workspaces/${W}/members`, null],
  [
    "listWorkspaceMembers by author",
    () => workspaces.listWorkspaceMembers(W, { invited_by: USER }),
    `GET /workspaces/${W}/members?invited_by=${USER}`,
    null,
  ],
  [
    "setWorkspaceMemberRole",
    () => workspaces.setWorkspaceMemberRole(W, USER, { role: "viewer" }),
    `PATCH /workspaces/${W}/members/${USER}`,
    { role: "viewer" },
  ],
  [
    "clearWorkspaceRoleOverride",
    () => workspaces.clearWorkspaceRoleOverride(W, USER),
    `DELETE /workspaces/${W}/members/${USER}/role-override`,
    null,
  ],
  [
    "excludeWorkspaceMember",
    () => workspaces.excludeWorkspaceMember(W, USER),
    `POST /workspaces/${W}/members/${USER}/exclusion`,
    null,
  ],
  [
    "liftWorkspaceExclusion",
    () => workspaces.liftWorkspaceExclusion(W, USER),
    `DELETE /workspaces/${W}/members/${USER}/exclusion`,
    null,
  ],
  [
    "removeWorkspaceGuest",
    () => workspaces.removeWorkspaceGuest(W, USER),
    `DELETE /workspaces/${W}/guests/${USER}`,
    null,
  ],
  ["leaveWorkspace", () => workspaces.leaveWorkspace(W), `POST /workspaces/${W}/leave`, null],
  [
    "listWorkspaceExclusions",
    () => workspaces.listWorkspaceExclusions(W),
    `GET /workspaces/${W}/exclusions`,
    null,
  ],
  [
    "uploadWorkspaceAvatar",
    () => workspaces.uploadWorkspaceAvatar(ID, AVATAR),
    `POST /avatars/workspaces/${ID}`,
    AVATAR,
  ],
  [
    "deleteWorkspaceAvatar",
    () => workspaces.deleteWorkspaceAvatar(ID),
    `DELETE /avatars/workspaces/${ID}`,
    null,
  ],

  // Team invitations
  ["listTeamInvitations", () => invitations.listTeamInvitations(T), `GET /teams/${T}/invitations`, null],
  [
    "listTeamInvitations by author",
    () => invitations.listTeamInvitations(T, { invited_by: USER }),
    `GET /teams/${T}/invitations?invited_by=${USER}`,
    null,
  ],
  [
    "createTeamInvitation",
    () => invitations.createTeamInvitation(T, { default_role: "member", username: "alice" }),
    `POST /teams/${T}/invitations`,
    { default_role: "member", username: "alice" },
  ],
  [
    "revokeTeamInvitation",
    () => invitations.revokeTeamInvitation(T, INV),
    `DELETE /teams/${T}/invitations/${INV}`,
    null,
  ],
  ["listPendingTeamInvitations", () => invitations.listPendingTeamInvitations(), "GET /teams/invitations/pending", null],
  [
    "acceptTeamInvitation",
    () => invitations.acceptTeamInvitation(INV),
    `POST /teams/invitations/${INV}/accept`,
    null,
  ],
  [
    "rejectTeamInvitation",
    () => invitations.rejectTeamInvitation(INV),
    `POST /teams/invitations/${INV}/reject`,
    null,
  ],
  [
    "previewTeamInvitation",
    () => invitations.previewTeamInvitation("tok"),
    "POST /teams/invitations/preview",
    { token: "tok" },
  ],
  ["joinTeam", () => invitations.joinTeam("tok"), "POST /teams/join", { token: "tok" }],

  // Workspace invitations
  [
    "listWorkspaceInvitations",
    () => invitations.listWorkspaceInvitations(W),
    `GET /workspaces/${W}/invitations`,
    null,
  ],
  [
    "createWorkspaceInvitation",
    () => invitations.createWorkspaceInvitation(W, { role: "viewer", invited_email: "a@example.test" }),
    `POST /workspaces/${W}/invitations`,
    { role: "viewer", invited_email: "a@example.test" },
  ],
  [
    "cancelWorkspaceInvitation",
    () => invitations.cancelWorkspaceInvitation(W, INV),
    `DELETE /workspaces/${W}/invitations/${INV}`,
    null,
  ],
  [
    "listPendingWorkspaceInvitations",
    () => invitations.listPendingWorkspaceInvitations(),
    "GET /workspaces/invitations/pending",
    null,
  ],
  [
    "acceptWorkspaceInvitation",
    () => invitations.acceptWorkspaceInvitation(INV),
    `POST /workspaces/invitations/${INV}/accept`,
    null,
  ],
  [
    "rejectWorkspaceInvitation",
    () => invitations.rejectWorkspaceInvitation(INV),
    `POST /workspaces/invitations/${INV}/reject`,
    null,
  ],
  [
    "previewWorkspaceInvitation",
    () => invitations.previewWorkspaceInvitation("tok"),
    "POST /workspaces/invitations/preview",
    { token: "tok" },
  ],
  ["joinWorkspace", () => invitations.joinWorkspace("tok"), "POST /workspaces/join", { token: "tok" }],
];

/** The calls whose route answers no body, or one the spec gives no schema. */
const RETURN_NOTHING = new Set([
  "deleteTeam",
  "removeTeamMember",
  "transferCaptaincy",
  "uploadTeamAvatar",
  "deleteTeamAvatar",
  "deleteWorkspace",
  "addFavorite",
  "removeFavorite",
  "setWorkspaceMemberRole",
  "clearWorkspaceRoleOverride",
  "excludeWorkspaceMember",
  "liftWorkspaceExclusion",
  "removeWorkspaceGuest",
  "leaveWorkspace",
  "uploadWorkspaceAvatar",
  "deleteWorkspaceAvatar",
  "revokeTeamInvitation",
  "rejectTeamInvitation",
  "cancelWorkspaceInvitation",
  "rejectWorkspaceInvitation",
]);

describe("the API calls", () => {
  it.each(cases)("%s sends exactly its request and returns the answer's body", async (name, call, route, body) => {
    const answer = { answered: route };
    const server = fakeApi({ [route]: { status: 200, data: answer } });
    const [method, url] = route.split(" ");

    const returned = await call();

    expect(server.requests).toEqual([{ method, url, body }]);
    expect(returned).toEqual(RETURN_NOTHING.has(name.split(" ")[0]) ? undefined : answer);
  });

  it("encodes a path segment taken from elsewhere", async () => {
    const server = fakeApi({ "GET /teams/a%2Fb": { status: 200, data: {} } });

    await teams.getTeam("a/b");

    expect(server.requests[0].url).toBe("/teams/a%2Fb");
  });

  it("sends an avatar as raw bytes, never as JSON", async () => {
    const server = fakeApi({ [`POST /avatars/workspaces/${ID}`]: { status: 200, data: {} } });

    await workspaces.uploadWorkspaceAvatar(ID, AVATAR);

    expect(server.requests[0].body).toBe(AVATAR);
  });
});
