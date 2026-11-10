import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import i18n from "@/lib/i18n";
import { lastBody } from "@/test/fake-api";
import { HEXAGON, renderWorkspace } from "@/test/render-workspace";

const t = (key: string, options: Record<string, unknown> = {}) => i18n.t(key, options);
const SLUG = HEXAGON.slug;
// To check against src/api/calls.ts: the archive, unarchive and leave routes.
const PATCH = `PATCH /workspaces/${SLUG}`;
const ARCHIVE = `POST /workspaces/${SLUG}/archive`;
const UNARCHIVE = `POST /workspaces/${SLUG}/unarchive`;
const LEAVE = `POST /workspaces/${SLUG}/leave`;
const DELETE = `DELETE /workspaces/${SLUG}`;

const captain = { ...HEXAGON, my_role: "captain" as const };
const nameField = () => screen.findByLabelText(t("workspaces:settings.identity.workspaceName"));
const button = (key: string) => screen.queryByRole("button", { name: t(key) });
const navLink = (key: string) => screen.getByRole("link", { name: t(`workspaces:settings.nav.${key}`) });
const landsOn = (path: RegExp) => waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(path));

describe("WorkspaceSettingsPage", () => {
  it("renames the workspace, sending only the name", async () => {
    const user = userEvent.setup();
    const { server } = renderWorkspace("/settings", { [PATCH]: { status: 200, data: { ...HEXAGON, name: "Hexagon 2026" } } });
    const name = await nameField();

    await user.clear(name);
    await user.type(name, "Hexagon 2026");
    await user.click(screen.getByRole("button", { name: t("workspaces:settings.save") }));

    await waitFor(() => expect(lastBody(server, PATCH)).toEqual({ name: "Hexagon 2026" }));
  });

  it("clears the description with null", async () => {
    const user = userEvent.setup();
    const workspace = { ...HEXAGON, description: "Online, 48 hours" };
    const { server } = renderWorkspace("/settings", { [PATCH]: { status: 200, data: { ...workspace, description: null } } }, { workspace });
    const description = await screen.findByLabelText(t("workspaces:settings.identity.description"));

    await user.clear(description);
    await user.click(screen.getByRole("button", { name: t("workspaces:settings.save") }));

    await waitFor(() => expect(lastBody(server, PATCH)).toEqual({ description: null }));
  });

  it("shows the event's dates only for an event", async () => {
    renderWorkspace("/settings", {}, { workspace: { ...HEXAGON, type: "platform", platform: "root_me" } });
    await nameField();
    expect(screen.queryByLabelText(t("workspaces:settings.event.start"))).not.toBeInTheDocument();
  });

  it("keeps the bar idle until something changes", async () => {
    const user = userEvent.setup();
    renderWorkspace("/settings");
    const name = await nameField();
    expect(button("workspaces:settings.save")).toBeDisabled();
    expect(screen.queryByText(t("workspaces:settings.unsaved"))).not.toBeInTheDocument();

    await user.type(name, " 2026");
    expect(button("workspaces:settings.save")).toBeEnabled();
    expect(screen.getByText(t("workspaces:settings.unsaved"))).toBeInTheDocument();
  });

  it("saves the writeup visibility with the bar, not on change", async () => {
    const user = userEvent.setup();
    const { server } = renderWorkspace("/settings?section=visibility", {
      [PATCH]: { status: 200, data: { ...HEXAGON, default_writeup_visibility: "draft" } },
    });

    await user.click(await screen.findByRole("tab", { name: t("workspaces:settings.writeupVisibility.draft") }));
    expect(server.count(PATCH)).toBe(0);
    await user.click(screen.getByRole("button", { name: t("workspaces:settings.save") }));

    await waitFor(() => expect(lastBody(server, PATCH)).toEqual({ default_writeup_visibility: "draft" }));
  });

  it("asks before leaving unsaved changes, and stays or discards", async () => {
    const user = userEvent.setup();
    renderWorkspace("/settings");
    const name = await nameField();
    await user.type(name, " 2026");

    await user.click(navLink("visibility"));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: t("workspaces:settings.unsavedStay") }));
    expect(screen.getByTestId("location")).toHaveTextContent(/\/settings$/);
    expect(name).toHaveValue("CTF Hexagon 2026");

    await user.click(navLink("visibility"));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: t("workspaces:settings.unsavedDiscard") }));
    await landsOn(/\?section=visibility$/);
  });

  it("saves, then goes on, from the unsaved changes dialog", async () => {
    const user = userEvent.setup();
    const { server } = renderWorkspace("/settings", { [PATCH]: { status: 200, data: { ...HEXAGON, name: "CTF Hexagon 2026" } } });
    await user.type(await nameField(), " 2026");

    await user.click(navLink("visibility"));
    await user.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: t("workspaces:settings.unsavedSave") }));

    await landsOn(/\?section=visibility$/);
    expect(lastBody(server, PATCH)).toEqual({ name: "CTF Hexagon 2026" });
  });

  it("points a team member to the team's roles", async () => {
    renderWorkspace("/settings?section=visibility");
    const manage = await screen.findByRole("link", { name: t("workspaces:settings.roles.manage") });
    expect(manage).toHaveAttribute("href", "/teams/alpha-k2m4?tab=roles");
  });

  it("does not point a guest to a team they are not on", async () => {
    const { server } = renderWorkspace("/settings?section=visibility", { "GET /teams": { status: 200, data: [] } });
    await screen.findByRole("tab", { name: t("workspaces:settings.writeupVisibility.draft") });
    await waitFor(() => expect(server.count("GET /teams")).toBe(1));
    expect(screen.queryByRole("link", { name: t("workspaces:settings.roles.manage") })).not.toBeInTheDocument();
  });

  it("falls back to General on an unknown section", async () => {
    renderWorkspace("/settings?section=nope");
    expect(await nameField()).toBeInTheDocument();
  });

  it("lets a Member read but not edit, and offers only to leave", async () => {
    const user = userEvent.setup();
    renderWorkspace("/settings", {}, { workspace: { ...HEXAGON, my_role: "member" } });
    expect(await nameField()).toBeDisabled();
    expect(screen.getByText(t("workspaces:settings.readOnly"))).toBeInTheDocument();
    expect(button("workspaces:settings.save")).not.toBeInTheDocument();

    await user.click(navLink("dangerZone"));
    expect(button("workspaces:settings.leaveAction")).toBeInTheDocument();
    expect(button("workspaces:settings.archive")).not.toBeInTheDocument();
    expect(button("workspaces:settings.delete")).not.toBeInTheDocument();
  });

  it("leaves, then goes to the workspaces", async () => {
    const user = userEvent.setup();
    const { server } = renderWorkspace("/settings?section=danger", { [LEAVE]: { status: 204 } }, { workspace: { ...HEXAGON, my_role: "member" } });

    await user.click(await screen.findByRole("button", { name: t("workspaces:settings.leaveAction") }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: t("workspaces:settings.leaveAction") }));

    await landsOn(/^\/workspaces$/);
    expect(server.count(LEAVE)).toBe(1);
  });

  it("gives the Captain archive and delete, never leave", async () => {
    renderWorkspace("/settings?section=danger", {}, { workspace: captain });
    expect(await screen.findByRole("button", { name: t("workspaces:settings.archive") })).toBeInTheDocument();
    expect(button("workspaces:settings.delete")).toBeInTheDocument();
    expect(button("workspaces:settings.leaveAction")).not.toBeInTheDocument();
  });

  it("archives after confirming", async () => {
    const user = userEvent.setup();
    const { server } = renderWorkspace("/settings?section=danger", { [ARCHIVE]: { status: 200, data: { ...captain, archived: true } } }, { workspace: captain });

    await user.click(await screen.findByRole("button", { name: t("workspaces:settings.archive") }));
    expect(server.count(ARCHIVE)).toBe(0);
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: t("workspaces:settings.archive") }));

    await waitFor(() => expect(server.count(ARCHIVE)).toBe(1));
  });

  it("deletes once the slug is typed, then goes to the workspaces", async () => {
    const user = userEvent.setup();
    const { server } = renderWorkspace("/settings?section=danger", { [DELETE]: { status: 204 } }, { workspace: captain });

    await user.click(await screen.findByRole("button", { name: t("workspaces:settings.delete") }));
    const dialog = screen.getByRole("alertdialog");
    const confirm = within(dialog).getByRole("button", { name: t("workspaces:settings.delete") });
    expect(confirm).toBeDisabled();
    await user.type(within(dialog).getByRole("textbox"), SLUG);
    await user.click(confirm);

    await landsOn(/^\/workspaces$/);
    expect(server.count(DELETE)).toBe(1);
  });

  it("freezes an archived workspace, all but unarchiving", async () => {
    const user = userEvent.setup();
    const archived = { ...captain, archived: true };
    const { server } = renderWorkspace("/settings", { [UNARCHIVE]: { status: 200, data: { ...captain, archived: false } } }, { workspace: archived });
    expect(await nameField()).toBeDisabled();

    await user.click(navLink("dangerZone"));
    expect(button("workspaces:settings.archive")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: t("workspaces:settings.unarchive") }));

    await waitFor(() => expect(server.count(UNARCHIVE)).toBe(1));
  });

  it("never offers a solo's owner to leave", async () => {
    renderWorkspace("/settings?section=danger", {}, { workspace: { ...HEXAGON, mode: "solo", team_id: null, my_role: "owner" } });
    expect(await screen.findByRole("button", { name: t("workspaces:settings.delete") })).toBeInTheDocument();
    expect(button("workspaces:settings.leaveAction")).not.toBeInTheDocument();
  });
});
