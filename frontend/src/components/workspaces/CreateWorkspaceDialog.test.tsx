import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent, { type UserEvent } from "@testing-library/user-event";
import { useState } from "react";
import { Route } from "react-router";
import { describe, expect, it } from "vitest";

import { CreateWorkspaceDialog } from "@/components/workspaces/CreateWorkspaceDialog";
import type { TeamResponse } from "@/lib/api/types";
import i18n from "@/lib/i18n";
import { teamResponse, workspaceResponse } from "@/test/api-fixtures";
import { fakeApi, lastBody, type FakeReply } from "@/test/fake-api";
import { renderRoute } from "@/test/render-route";

const t = (key: string, options?: Record<string, unknown>) => i18n.t(key, options);

const ALPHA_ID = "0000000a-0000-7000-8000-000000000001";
const alpha = teamResponse({ id: ALPHA_ID, slug: "alpha-k2m4", name: "Team Alpha", my_role: "captain" });
const bravo = teamResponse({ id: "0000000a-0000-7000-8000-000000000002", slug: "bravo-p9q1", name: "Team Bravo", my_role: "member" });
const CREATED: FakeReply = { status: 201, data: workspaceResponse({ slug: "root-me-grind-x9k2", name: "Root-Me grind" }) };

/** The dialog as a page holds it: open at first, closable, opened again by a button. */
function Harness({ defaultTeamId }: Readonly<{ defaultTeamId?: string }>) {
  const [open, setOpen] = useState(true);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>reopen</button>
      <CreateWorkspaceDialog open={open} onOpenChange={setOpen} defaultTeamId={defaultTeamId} />
    </>
  );
}

type Options = { teams?: TeamResponse[]; reply?: FakeReply; defaultTeamId?: string };

function renderDialog({ teams = [alpha], reply = CREATED, defaultTeamId }: Options = {}) {
  const server = fakeApi({
    "GET /teams": { status: 200, data: teams },
    "POST /workspaces": reply,
  });
  renderRoute("/workspaces", (
    <>
      <Route path="/workspaces" element={<Harness defaultTeamId={defaultTeamId} />} />
      <Route path="*" element={null} />
    </>
  ));
  return { server, user: userEvent.setup() };
}

/** An OptionCard radio, found by the start of its name (title, then description). */
const option = (titleKey: string) => screen.getByRole("radio", { name: (name) => name.startsWith(t(titleKey)) });
const button = (key: string) => screen.getByRole("button", { name: t(key) });
const nameInput = () => screen.getByLabelText(t("workspaces:create.details.name"));
const teamPicker = () => screen.getByRole("combobox", { name: t("workspaces:create.mode.pickTeam") });
/** Waits for the router to land on `path` (the mutation answers after the click). */
const landsOn = (path: string) => waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(path));

async function fillDetails(user: UserEvent, { name = "Root-Me grind", platform = "root_me" as string | null } = {}) {
  await user.type(nameInput(), name);
  if (platform) {
    await user.click(screen.getByRole("combobox", { name: t("workspaces:create.details.platform") }));
    await user.click(await screen.findByRole("option", { name: t(`nav:platforms.${platform}`) }));
  }
}

function setDates(starts: string, ends: string) {
  // jsdom does not type into datetime-local inputs: set the value directly.
  fireEvent.change(screen.getByLabelText(t("workspaces:create.details.startsAt")), { target: { value: starts } });
  fireEvent.change(screen.getByLabelText(t("workspaces:create.details.endsAt")), { target: { value: ends } });
}

describe("CreateWorkspaceDialog", () => {
  it("shows the three sections at once, platform and solo chosen", () => {
    renderDialog();
    expect(screen.getByRole("dialog", { name: t("workspaces:create.title") })).toBeInTheDocument();
    expect(screen.getByRole("group", { name: t("workspaces:create.steps.type") })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: t("workspaces:create.steps.details") })).toBeInTheDocument();
    expect(screen.getByRole("group", { name: t("workspaces:create.steps.mode") })).toBeInTheDocument();
    expect(option("workspaces:create.type.platform")).toBeChecked();
    expect(option("workspaces:create.mode.solo")).toBeChecked();
  });

  it("creates a solo platform workspace, closes and opens it", async () => {
    const { server, user } = renderDialog();
    await fillDetails(user);
    await user.click(button("workspaces:create.submit"));

    await landsOn("/w/root-me-grind-x9k2");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(lastBody(server, "POST /workspaces")).toMatchObject({
      name: "Root-Me grind", type: "platform", platform: "root_me", mode: "solo", team_id: null, starts_at: null, ends_at: null,
    });
  });

  it("asks for a platform before sending anything", async () => {
    const { server, user } = renderDialog();
    await fillDetails(user, { platform: null });
    await user.click(button("workspaces:create.submit"));

    expect(await screen.findByText(t("workspaces:validation.platformRequired"))).toBeInTheDocument();
    expect(server.count("POST /workspaces")).toBe(0);
  });

  it("shows the dates only for an event", async () => {
    const { user } = renderDialog();
    expect(screen.queryByLabelText(t("workspaces:create.details.startsAt"))).not.toBeInTheDocument();

    await user.click(option("workspaces:create.type.event"));
    expect(screen.getByLabelText(t("workspaces:create.details.startsAt"))).toBeInTheDocument();
    expect(screen.getByLabelText(t("workspaces:create.details.endsAt"))).toBeInTheDocument();
  });

  it("refuses an event that ends before it starts", async () => {
    const { server, user } = renderDialog();
    await user.click(option("workspaces:create.type.event"));
    await fillDetails(user, { name: "CTF Hexagon", platform: null });
    setDates("2026-11-10T10:00", "2026-11-09T10:00");
    await user.click(button("workspaces:create.submit"));

    expect(await screen.findByText(t("workspaces:validation.endsBeforeStart"))).toBeInTheDocument();
    expect(server.count("POST /workspaces")).toBe(0);
  });

  it("sends the event dates as UTC instants", async () => {
    const { server, user } = renderDialog();
    await user.click(option("workspaces:create.type.event"));
    await fillDetails(user, { name: "CTF Hexagon", platform: null });
    setDates("2026-11-10T10:00", "2026-11-12T18:00");
    await user.click(button("workspaces:create.submit"));

    await landsOn("/w/root-me-grind-x9k2");
    expect(lastBody(server, "POST /workspaces")).toMatchObject({
      type: "event", platform: null,
      starts_at: new Date("2026-11-10T10:00").toISOString(),
      ends_at: new Date("2026-11-12T18:00").toISOString(),
    });
  });

  it("greys out the team option when no team lets me create a workspace", async () => {
    renderDialog({ teams: [bravo] });

    expect(await screen.findByText(t("workspaces:create.mode.noEligibleTeam"))).toBeInTheDocument();
    expect(option("workspaces:create.mode.team")).toBeDisabled();
  });

  it("creates a team workspace for the chosen team, and asks for one first", async () => {
    const { server, user } = renderDialog({ teams: [alpha, bravo] });
    await fillDetails(user);
    await waitFor(() => expect(option("workspaces:create.mode.team")).toBeEnabled());
    await user.click(option("workspaces:create.mode.team"));

    await user.click(button("workspaces:create.submit"));
    expect(await screen.findByText(t("workspaces:validation.teamRequired"))).toBeInTheDocument();

    await user.click(teamPicker());
    expect(screen.queryByRole("option", { name: /Team Bravo/ })).not.toBeInTheDocument(); // member only
    await user.click(screen.getByRole("option", { name: /Team Alpha/ }));
    expect(teamPicker()).toHaveTextContent("Team Alpha");
    await user.click(button("workspaces:create.submit"));

    await landsOn("/w/root-me-grind-x9k2");
    expect(lastBody(server, "POST /workspaces")).toMatchObject({ mode: "team", team_id: ALPHA_ID });
  });

  it("puts a reserved name back under the name field, dialog still open", async () => {
    const { user } = renderDialog({ reply: { status: 422, data: { error: "workspace_name_reserved" } } });
    await fillDetails(user, { name: "admin" });
    await user.click(button("workspaces:create.submit"));

    expect(await screen.findByText(t("errors:api.workspace_name_reserved"))).toBeInTheDocument();
    expect(nameInput()).toHaveAttribute("aria-invalid", "true");
    expect(nameInput()).toHaveFocus();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByTestId("location")).toHaveTextContent(/^\/workspaces$/);
  });

  it("preselects the team it is given", async () => {
    const { server, user } = renderDialog({ defaultTeamId: ALPHA_ID });
    expect(option("workspaces:create.mode.team")).toBeChecked();
    await waitFor(() => expect(teamPicker()).toHaveTextContent("Team Alpha"));

    await fillDetails(user);
    await user.click(button("workspaces:create.submit"));

    await landsOn("/w/root-me-grind-x9k2");
    expect(lastBody(server, "POST /workspaces")).toMatchObject({ mode: "team", team_id: ALPHA_ID });
  });

  it("closes on Cancel without sending anything", async () => {
    const { server, user } = renderDialog();
    await user.click(button("workspaces:create.cancel"));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(server.count("POST /workspaces")).toBe(0);
  });

  it("starts over when closed and opened again", async () => {
    const { user } = renderDialog();
    await user.click(option("workspaces:create.type.event"));
    await user.type(nameInput(), "Root-Me grind");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "reopen" }));
    expect(nameInput()).toHaveValue("");
    expect(option("workspaces:create.type.platform")).toBeChecked();
  });
});
