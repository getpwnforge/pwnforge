import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import i18n from "@/lib/i18n";
import { teamInvitation } from "@/test/api-fixtures";
import { lastBody } from "@/test/fake-api";
import { ALPHA, renderTeam } from "@/test/render-team";

const t = (key: string, options?: Record<string, unknown>) => i18n.t(key, options);
const LIST = "GET /teams/alpha-k2m4/invitations";
const CREATE = "POST /teams/alpha-k2m4/invitations";

const link = teamInvitation({
  id: "0000000e-0000-7000-8000-00000000000a", target: "link", invited_username: null, invited_user_id: null,
  max_uses: 10, uses: 3,
});
const nora = teamInvitation({ id: "0000000e-0000-7000-8000-00000000000b" });
const byMail = teamInvitation({
  id: "0000000e-0000-7000-8000-00000000000c", target: "email", invited_username: null, invited_user_id: null,
  invited_email: "thomas@example.test",
});
const ROUTES = { [LIST]: { status: 200, data: [link, nora, byMail] } };

const who = () => screen.getByLabelText(t("teams:invitations.invite.username"));
const invite = () => screen.getByRole("button", { name: t("teams:invitations.invite.submit") });

describe("TeamInvitationsTab", () => {
  it("lists the pending invitations: links with their uses, people by name or address", async () => {
    renderTeam("?tab=invitations", ROUTES);
    expect(await screen.findByText("@nora")).toBeInTheDocument();
    expect(screen.getByText("thomas@example.test")).toBeInTheDocument();
    expect(screen.getByText(t("teams:invitations.table.uses", { count: 3, max: 10 }))).toBeInTheDocument();
  });

  it("invites by username, without the @", async () => {
    const user = userEvent.setup();
    const { server } = renderTeam("?tab=invitations", { ...ROUTES, [CREATE]: { status: 201, data: nora } });
    await screen.findByText("@nora");

    await user.type(who(), "@kira");
    await user.click(invite());

    await waitFor(() => expect(lastBody(server, CREATE)).toEqual({ default_role: "member", username: "kira" }));
    expect(who()).toHaveValue("");
  });

  it("invites by address, and checks it first", async () => {
    const user = userEvent.setup();
    const { server } = renderTeam("?tab=invitations", { ...ROUTES, [CREATE]: { status: 201, data: byMail } });
    await screen.findByText("@nora");

    await user.click(screen.getByRole("tab", { name: t("teams:invitations.invite.byEmail") }));
    const email = screen.getByLabelText(t("teams:invitations.invite.email"));
    await user.type(email, "kira");
    await user.click(invite());
    expect(await screen.findByText(t("teams:invitations.validation.emailInvalid"))).toBeInTheDocument();
    expect(server.count(CREATE)).toBe(0);

    await user.clear(email);
    await user.type(email, "kira@example.test");
    await user.click(invite());

    await waitFor(() => expect(lastBody(server, CREATE)).toEqual({ default_role: "member", invited_email: "kira@example.test" }));
  });

  it("puts an already member refusal under the field", async () => {
    const user = userEvent.setup();
    renderTeam("?tab=invitations", { ...ROUTES, [CREATE]: { status: 409, data: { error: "already_member" } } });
    await screen.findByText("@nora");

    await user.type(who(), "enzo");
    await user.click(invite());

    expect(await screen.findByText(t("errors:api.already_member"))).toBeInTheDocument();
  });

  it("offers Admin to the Captain only", async () => {
    const user = userEvent.setup();
    renderTeam("?tab=invitations", ROUTES, { team: { ...ALPHA, my_role: "admin" } });
    await screen.findByText("@nora");

    await user.click(screen.getByRole("combobox", { name: t("teams:invitations.invite.role") }));
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual([t("common:roles.member"), t("common:roles.viewer")]);
  });

  it("shows a new link once, then forgets it", async () => {
    const user = userEvent.setup();
    const { server } = renderTeam("?tab=invitations", {
      ...ROUTES,
      [CREATE]: { status: 201, data: { ...link, uses: 0, max_uses: 5, token: "aK7d9Qm2vT4xR8wP" } },
    });
    await screen.findByText("@nora");

    await user.click(screen.getByRole("button", { name: t("teams:invitations.link.create") }));
    const dialog = screen.getByRole("dialog");
    await user.type(within(dialog).getByLabelText(t("teams:invitations.link.maxUses")), "5");
    await user.click(within(dialog).getByRole("button", { name: t("teams:invitations.link.submit") }));

    const url = await within(dialog).findByRole("textbox", { name: t("teams:invitations.link.url") });
    expect((url as HTMLInputElement).value).toMatch(/\/invitations\/team#token=aK7d9Qm2vT4xR8wP$/);
    expect(lastBody(server, CREATE)).toEqual({ default_role: "member", max_uses: 5 });

    await user.click(within(dialog).getByRole("button", { name: t("teams:invitations.link.done") }));
    await user.click(screen.getByRole("button", { name: t("teams:invitations.link.create") }));
    expect(within(screen.getByRole("dialog")).queryByRole("textbox", { name: t("teams:invitations.link.url") })).not.toBeInTheDocument();
  });

  it("revokes an invitation after confirming", async () => {
    const user = userEvent.setup();
    const revoke = `DELETE /teams/alpha-k2m4/invitations/${nora.id}`;
    const { server } = renderTeam("?tab=invitations", { ...ROUTES, [revoke]: { status: 204 } });
    await screen.findByText("@nora");

    await user.click(screen.getByRole("button", { name: t("teams:invitations.table.revokeNamed", { name: "@nora" }) }));
    expect(server.count(revoke)).toBe(0);
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: t("teams:invitations.table.revoke") }));

    await waitFor(() => expect(server.count(revoke)).toBe(1));
  });

  it("says when nothing is pending", async () => {
    renderTeam("?tab=invitations", { [LIST]: { status: 200, data: [] } });
    expect(await screen.findByText(t("teams:invitations.table.empty"))).toBeInTheDocument();
  });
});
