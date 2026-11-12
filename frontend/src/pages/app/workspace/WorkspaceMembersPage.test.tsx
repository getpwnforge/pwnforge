import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import i18n from "@/lib/i18n";
import { effectiveMember, excludedMember, ME_ID, teamMember, workspaceInvitation } from "@/test/api-fixtures";
import { lastBody } from "@/test/fake-api";
import { ALPHA_TEAM, HEXAGON, renderWorkspace } from "@/test/render-workspace";

const t = (key: string, options: Record<string, unknown> = {}) => i18n.t(key, options);
const SLUG = HEXAGON.slug;

// To check against src/api/workspaces.ts and src/api/invitations.ts: the write routes.
const MEMBERS = `GET /workspaces/${SLUG}/members`;
const EXCLUSIONS = `GET /workspaces/${SLUG}/exclusions`;
const INVITATIONS = `GET /workspaces/${SLUG}/invitations`;
const ROSTER = `GET /teams/${ALPHA_TEAM.slug}/members`;
const MARC = "0000000f-0000-7000-8000-000000000002";
const JULIE = "0000000f-0000-7000-8000-000000000003";
const KENJI = "0000000f-0000-7000-8000-000000000004";
const NORA = "0000000f-0000-7000-8000-000000000009";
const SET_ROLE = `PATCH /workspaces/${SLUG}/members/${MARC}`;
const CLEAR_OVERRIDE = `DELETE /workspaces/${SLUG}/members/${JULIE}/role-override`;
const EXCLUDE = `POST /workspaces/${SLUG}/members/${MARC}/exclusion`;
const LIFT = `DELETE /workspaces/${SLUG}/members/${NORA}/exclusion`;
const INVITE = `POST /workspaces/${SLUG}/invitations`;
const INVITATION_ID = "0000000e-0000-7000-8000-000000000002";
const CANCEL = `DELETE /workspaces/${SLUG}/invitations/${INVITATION_ID}`;


const PEOPLE = [
  effectiveMember({ user_id: ME_ID, handle: "enzo", first_name: "Enzo", last_name: "P", role: "captain" }),
  effectiveMember({ user_id: MARC, handle: "m0nster", first_name: "Marc", last_name: "Dubois", role: "admin" }),
  effectiveMember({ user_id: JULIE, handle: "jkrn", first_name: "Julie", last_name: "Kern", role: "admin", role_source: "override" }),
  effectiveMember({
    user_id: KENJI, handle: "k3nji", first_name: "Kenji", last_name: "Sato", role: "member", role_source: "guest", is_guest: true, invited_by: MARC,
  }),
];
const TEAM_ROSTER = [
  teamMember({ user_id: ME_ID, role: "captain" }),
  teamMember({ user_id: MARC, username: "m0nster", role: "admin" }),
  teamMember({ user_id: JULIE, username: "jkrn", role: "member" }),
  teamMember({ user_id: NORA, username: "nora", role: "viewer" }),
];

/** A team workspace seen as `role`, with the lists every section reads. */
function renderMembers(role: "captain" | "admin" | "member" | "viewer", routes: Parameters<typeof renderWorkspace>[1] = {}) {
  return renderWorkspace(
    "/members",
    {
      [MEMBERS]: { status: 200, data: PEOPLE },
      [ROSTER]: { status: 200, data: TEAM_ROSTER },
      [EXCLUSIONS]: { status: 200, data: [excludedMember({ user_id: NORA })] },
      [INVITATIONS]: { status: 200, data: [workspaceInvitation({ invited_by: ME_ID })] },
      ...routes,
    },
    { workspace: { ...HEXAGON, my_role: role } },
  );
}

const actionsFor = (name: string) => screen.findByRole("button", { name: t("workspaces:members.actionsFor", { name }) });

describe("WorkspaceMembersPage", () => {
  it("says where each role comes from", async () => {
    renderMembers("captain");
    expect(await screen.findByText("Julie Kern")).toBeInTheDocument();
    expect(await screen.findByText(t("workspaces:members.via.override", { role: t("common:roles.member") }))).toBeInTheDocument();
    expect(screen.getByText(t("workspaces:members.via.invitedBy", { name: "Marc Dubois" }))).toBeInTheDocument();
  });

  it("lets the Captain change a role at once", async () => {
    const user = userEvent.setup();
    const { server } = renderMembers("captain", { [SET_ROLE]: { status: 200, data: {} } });

    await user.click(await actionsFor("Marc Dubois"));
    await user.click(screen.getByRole("menuitem", { name: t("common:roles.viewer") }));

    await waitFor(() => expect(lastBody(server, SET_ROLE)).toEqual({ role: "viewer" }));
  });

  it("clears an override when the team role is picked back", async () => {
    const user = userEvent.setup();
    const { server } = renderMembers("captain", { [CLEAR_OVERRIDE]: { status: 204 } });

    await user.click(await actionsFor("Julie Kern"));
    await user.click(await screen.findByRole("menuitem", { name: t("common:roles.member") }));

    await waitFor(() => expect(server.count(CLEAR_OVERRIDE)).toBe(1));
  });

  it("asks before excluding", async () => {
    const user = userEvent.setup();
    const { server } = renderMembers("captain", { [EXCLUDE]: { status: 204 } });

    await user.click(await actionsFor("Marc Dubois"));
    await user.click(screen.getByRole("menuitem", { name: t("workspaces:members.exclude") }));
    expect(server.count(EXCLUDE)).toBe(0);
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: t("workspaces:members.exclude") }));

    await waitFor(() => expect(server.count(EXCLUDE)).toBe(1));
  });

  it("gives an Admin nothing on another Admin", async () => {
    renderMembers("admin");
    await screen.findByText("Julie Kern");
    expect(screen.queryByRole("button", { name: t("workspaces:members.actionsFor", { name: "Marc Dubois" }) })).not.toBeInTheDocument();
  });

  it("lets a Member invite, see their invitations, and nothing else", async () => {
    renderMembers("member");
    expect(await screen.findByRole("button", { name: t("workspaces:members.invite.open") })).toBeInTheDocument();
    expect(await screen.findByText(t("workspaces:members.invitations.title"), { exact: false })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /actions/i })).not.toBeInTheDocument();
    expect(screen.queryByText(t("workspaces:members.exclusions.title"), { exact: false })).not.toBeInTheDocument();
  });

  it("keeps a Viewer from inviting", async () => {
    renderMembers("viewer");
    await screen.findByText("Julie Kern");
    expect(screen.queryByRole("button", { name: t("workspaces:members.invite.open") })).not.toBeInTheDocument();
    expect(screen.queryByText(t("workspaces:members.invitations.title"), { exact: false })).not.toBeInTheDocument();
  });

  it("invites a guest by username", async () => {
    const user = userEvent.setup();
    const { server } = renderMembers("member", { [INVITE]: { status: 201, data: workspaceInvitation() } });

    await user.click(await screen.findByRole("button", { name: t("workspaces:members.invite.open") }));
    await user.type(screen.getByLabelText(t("workspaces:members.invite.username")), "@kira");
    await user.click(screen.getByRole("button", { name: t("workspaces:members.invite.submit") }));

    await waitFor(() => expect(lastBody(server, INVITE)).toEqual({ username: "kira", role: "member" }));
  });

  it("refuses an address that is not one, before sending", async () => {
    const user = userEvent.setup();
    const { server } = renderMembers("member", { [INVITE]: { status: 201, data: workspaceInvitation() } });

    await user.click(await screen.findByRole("button", { name: t("workspaces:members.invite.open") }));
    await user.click(screen.getByRole("tab", { name: t("workspaces:members.invite.byEmail") }));
    await user.type(screen.getByLabelText(t("workspaces:members.invite.email")), "not-an-address");
    await user.click(screen.getByRole("button", { name: t("workspaces:members.invite.submit") }));

    expect(await screen.findByText(t("workspaces:members.invite.emailInvalid"))).toBeInTheDocument();
    expect(server.count(INVITE)).toBe(0);
  });

  it("lists the exclusions to the Captain, who can lift one", async () => {
    const user = userEvent.setup();
    const { server } = renderMembers("captain", { [LIFT]: { status: 204 } });

    await user.click(await screen.findByRole("button", { name: t("workspaces:members.exclusions.lift") }));

    await waitFor(() => expect(server.count(LIFT)).toBe(1));
  });

  it("cancels a pending invitation after confirming", async () => {
    const user = userEvent.setup();
    const { server } = renderMembers("captain", { [CANCEL]: { status: 204 } });

    await user.click(await screen.findByRole("button", { name: t("workspaces:members.invitations.cancel") }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: t("workspaces:members.invitations.cancel") }));

    await waitFor(() => expect(server.count(CANCEL)).toBe(1));
  });

  it("sends a solo workspace to its dashboard", async () => {
    renderWorkspace("/members", {}, { workspace: { ...HEXAGON, mode: "solo", team_id: null, my_role: "owner" } });
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(new RegExp(`^/w/${SLUG}$`)));
  });

  it("lists only the invitations still pending", async () => {
    renderMembers("captain", {
      [INVITATIONS]: {
        status: 200,
        data: [
          workspaceInvitation({ invited_username: "nora", status: "pending" }),
          workspaceInvitation({ id: "0000000e-0000-7000-8000-000000000003", invited_username: "kira", status: "accepted" }),
          workspaceInvitation({ id: "0000000e-0000-7000-8000-000000000004", invited_username: "lou", status: "expired" }),
        ],
      },
    });
    expect(await screen.findByText("@nora")).toBeInTheDocument();
    expect(screen.queryByText("@kira")).not.toBeInTheDocument();
    expect(screen.queryByText("@lou")).not.toBeInTheDocument();
  });
});
