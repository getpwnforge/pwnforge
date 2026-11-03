import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import i18n from "@/lib/i18n";
import { ME_ID, teamMember } from "@/test/api-fixtures";
import { lastBody } from "@/test/fake-api";
import { ALPHA, renderTeam } from "@/test/render-team";

const t = (key: string, options?: Record<string, unknown>) => i18n.t(key, options);
const NORA_ID = "0000000f-0000-7000-8000-000000000009";
const MEMBERS = {
  "GET /teams/alpha-k2m4/members": {
    status: 200,
    data: [teamMember(), teamMember({ user_id: NORA_ID, username: "nora", first_name: "Nora", last_name: "", role: "admin" })],
  },
};
const button = (key: string) => screen.queryByRole("button", { name: t(key) });
const landsOn = (path: RegExp) => waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(path));

describe("TeamSettingsTab", () => {
  it("gives the Captain transfer and delete, but not leave", async () => {
    renderTeam("?tab=settings", MEMBERS);
    expect(await screen.findByRole("button", { name: t("teams:settings.transfer") })).toBeInTheDocument();
    expect(button("teams:settings.delete")).toBeInTheDocument();
    expect(button("teams:settings.leave")).not.toBeInTheDocument();
  });

  it("gives a Member only leave", async () => {
    renderTeam("?tab=settings", MEMBERS, { team: { ...ALPHA, my_role: "member" } });
    expect(await screen.findByRole("button", { name: t("teams:settings.leave") })).toBeInTheDocument();
    expect(button("teams:settings.delete")).not.toBeInTheDocument();
    expect(button("teams:settings.transfer")).not.toBeInTheDocument();
    expect(screen.queryByText(t("teams:settings.identity"))).not.toBeInTheDocument();
  });

  it("renames the team, sending only the name", async () => {
    const user = userEvent.setup();
    const { server } = renderTeam("?tab=settings", { ...MEMBERS, "PATCH /teams/alpha-k2m4": { status: 200, data: { ...ALPHA, name: "Team Omega" } } });
    const name = await screen.findByLabelText(t("teams:settings.name"));

    await user.clear(name);
    await user.type(name, "Team Omega");
    await user.click(screen.getByRole("button", { name: t("teams:settings.save") }));

    await waitFor(() => expect(lastBody(server, "PATCH /teams/alpha-k2m4")).toEqual({ name: "Team Omega" }));
  });

  it("clears the description with null", async () => {
    const user = userEvent.setup();
    const team = { ...ALPHA, description: "CTF team from Nantes" };
    const { server } = renderTeam("?tab=settings", { ...MEMBERS, "PATCH /teams/alpha-k2m4": { status: 200, data: { ...team, description: null } } }, { team });
    const description = await screen.findByLabelText(t("teams:settings.description"));
    expect(description).toHaveValue("CTF team from Nantes");

    await user.clear(description);
    await user.click(screen.getByRole("button", { name: t("teams:settings.save") }));

    await waitFor(() => expect(lastBody(server, "PATCH /teams/alpha-k2m4")).toEqual({ description: null }));
  });

  it("lets the members create workspaces once switched on", async () => {
    const user = userEvent.setup();
    const { server } = renderTeam("?tab=settings", {
      ...MEMBERS,
      "PATCH /teams/alpha-k2m4": { status: 200, data: { ...ALPHA, members_can_create_workspaces: true } },
    });
    const toggle = await screen.findByRole("switch", { name: t("teams:settings.membersCreate") });
    expect(toggle).not.toBeChecked();

    await user.click(toggle);
    await user.click(screen.getByRole("button", { name: t("teams:settings.save") }));

    await waitFor(() => expect(lastBody(server, "PATCH /teams/alpha-k2m4")).toEqual({ members_can_create_workspaces: true }));
  });

  it("deletes the team once its name is typed, then leaves for /teams", async () => {
    const user = userEvent.setup();
    const { server } = renderTeam("?tab=settings", { ...MEMBERS, "DELETE /teams/alpha-k2m4": { status: 204 } });
    await user.click(await screen.findByRole("button", { name: t("teams:settings.delete") }));
    const dialog = screen.getByRole("alertdialog");
    const confirm = within(dialog).getByRole("button", { name: t("teams:settings.delete") });

    expect(confirm).toBeDisabled();
    await user.type(within(dialog).getByRole("textbox"), "Team Alpha");
    await user.click(confirm);

    await landsOn(/^\/teams$/);
    expect(server.count("DELETE /teams/alpha-k2m4")).toBe(1);
  });

  it("lets a Member leave, then goes to /teams", async () => {
    const user = userEvent.setup();
    const { server } = renderTeam("?tab=settings", { ...MEMBERS, [`DELETE /teams/alpha-k2m4/members/${ME_ID}`]: { status: 204 } },
      { team: { ...ALPHA, my_role: "member" } });
    await user.click(await screen.findByRole("button", { name: t("teams:settings.leave") }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: t("teams:settings.leave") }));

    await landsOn(/^\/teams$/);
    expect(server.count(`DELETE /teams/alpha-k2m4/members/${ME_ID}`)).toBe(1);
  });

  it("hands the captaincy to another member, never to me", async () => {
    const user = userEvent.setup();
    const { server } = renderTeam("?tab=settings", { ...MEMBERS, "POST /teams/alpha-k2m4/transfer": { status: 204 } });
    await user.click(await screen.findByRole("button", { name: t("teams:settings.transfer") }));
    const dialog = screen.getByRole("dialog");
    const confirm = within(dialog).getByRole("button", { name: t("teams:settings.transferConfirm") });
    expect(confirm).toBeDisabled();

    await user.click(within(dialog).getByRole("combobox", { name: t("teams:settings.transferTarget") }));
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["Nora"]);
    await user.click(screen.getByRole("option", { name: "Nora" }));
    await user.click(confirm);

    await waitFor(() => expect(lastBody(server, "POST /teams/alpha-k2m4/transfer")).toEqual({ user_id: NORA_ID }));
  });

  it("says why the API refused the transfer", async () => {
    const user = userEvent.setup();
    renderTeam("?tab=settings", { ...MEMBERS, "POST /teams/alpha-k2m4/transfer": { status: 409, data: { error: "target_pending_deletion" } } });
    await user.click(await screen.findByRole("button", { name: t("teams:settings.transfer") }));
    const dialog = screen.getByRole("dialog");
    await user.click(within(dialog).getByRole("combobox", { name: t("teams:settings.transferTarget") }));
    await user.click(screen.getByRole("option", { name: "Nora" }));
    await user.click(within(dialog).getByRole("button", { name: t("teams:settings.transferConfirm") }));

    expect(await within(dialog).findByText(t("errors:api.target_pending_deletion"))).toBeInTheDocument();
  });
});
