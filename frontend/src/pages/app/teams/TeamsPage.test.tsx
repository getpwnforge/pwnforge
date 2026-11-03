import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route } from "react-router";
import { describe, expect, it } from "vitest";

import i18n from "@/lib/i18n";
import { TeamsPage } from "@/pages/app/teams/TeamsPage";
import { teamInvitation, teamResponse, teamSummary, workspaceSummary } from "@/test/api-fixtures";
import { fakeApi, lastBody } from "@/test/fake-api";
import { renderRoute } from "@/test/render-route";
import { ALPHA } from "@/test/render-team";

const t = (key: string, options?: Record<string, unknown>) => i18n.t(key, options);
const bravo = teamResponse({ id: "0000000a-0000-7000-8000-000000000002", slug: "bravo-p9q1", name: "Team Bravo", my_role: "member", member_count: 5 });
const bravoRef = teamSummary({ id: bravo.id, slug: bravo.slug, name: bravo.name, color: bravo.color });
const INVITE_ID = "0000000e-0000-7000-8000-000000000007";
const invite = teamInvitation({ id: INVITE_ID, team_name: "Team Charlie", default_role: "member" });

function renderPage(routes: Parameters<typeof fakeApi>[0] = {}) {
  const server = fakeApi({
    "GET /teams": { status: 200, data: [ALPHA, bravo] },
    "GET /workspaces": { status: 200, data: [] },
    "GET /teams/invitations/pending": { status: 200, data: [] },
    "GET /workspaces/invitations/pending": { status: 200, data: [] },
    ...routes,
  });
  renderRoute("/teams", (
    <>
      <Route path="/teams" element={<TeamsPage />} />
      <Route path="*" element={null} />
    </>
  ));
  return { server, user: userEvent.setup() };
}

const landsOn = (path: string) => waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(path));

/** The card whose title links to `name`. */
async function card(name: string): Promise<HTMLElement> {
  const found = (await screen.findByRole("link", { name })).closest<HTMLElement>("[data-slot=card]");
  if (!found) throw new Error(`no card for "${name}"`);
  return found;
}

/** Opens the creation dialog from the header button. */
async function openCreate(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: t("teams:list.new") }));
  return screen.getByRole("dialog", { name: t("teams:create.title") });
}

describe("TeamsPage", () => {
  it("lists my teams with my role, their size and their workspaces", async () => {
    renderPage({
      "GET /workspaces": {
        status: 200,
        data: [
          workspaceSummary({ id: "00000001-0000-7000-8000-000000000001", slug: "a-1", team: bravoRef }),
          workspaceSummary({ id: "00000002-0000-7000-8000-000000000002", slug: "a-2", team: bravoRef }),
          workspaceSummary({ id: "00000003-0000-7000-8000-000000000003", slug: "solo-3", mode: "solo", team: null }),
        ],
      },
    });
    const bravoCard = within(await card("Team Bravo"));
    expect(bravoCard.getByText(t("common:roles.member"))).toBeInTheDocument();
    expect(bravoCard.getByText(t("teams:card.members", { count: 5 }))).toBeInTheDocument();
    expect(await bravoCard.findByText(t("teams:card.workspaces", { count: 2 }))).toBeInTheDocument();
  });

  it("links each card to the team, its workspaces and its settings", async () => {
    renderPage();
    const bravoCard = within(await card("Team Bravo"));
    expect(bravoCard.getByRole("link", { name: "Team Bravo" })).toHaveAttribute("href", "/teams/bravo-p9q1");
    expect(bravoCard.getByRole("link", { name: t("teams:card.openWorkspaces") })).toHaveAttribute("href", "/teams/bravo-p9q1?tab=workspaces");
    expect(bravoCard.getByRole("link", { name: t("teams:card.settings", { name: "Team Bravo" }) })).toHaveAttribute("href", "/teams/bravo-p9q1?tab=settings");
  });

  it("says so when no invitation is waiting", async () => {
    renderPage();
    expect(await screen.findByText(t("common:invitations.none"))).toBeInTheDocument();
  });

  it("accepts an invitation and opens the team", async () => {
    const { server, user } = renderPage({
      "GET /teams/invitations/pending": { status: 200, data: [invite] },
      [`POST /teams/invitations/${INVITE_ID}/accept`]: {
        status: 200, data: teamResponse({ slug: "charlie-z7y8", name: "Team Charlie" }),
      },
    });
    await user.click(await screen.findByRole("button", { name: t("common:invitations.acceptNamed", { name: "Team Charlie" }) }));

    await landsOn("/teams/charlie-z7y8");
    expect(server.count(`POST /teams/invitations/${INVITE_ID}/accept`)).toBe(1);
  });

  it("declines an invitation and stays on the list", async () => {
    const { server, user } = renderPage({
      "GET /teams/invitations/pending": { status: 200, data: [invite] },
      [`POST /teams/invitations/${INVITE_ID}/reject`]: { status: 204 },
    });
    await user.click(await screen.findByRole("button", { name: t("common:invitations.rejectNamed", { name: "Team Charlie" }) }));

    await waitFor(() => expect(server.count(`POST /teams/invitations/${INVITE_ID}/reject`)).toBe(1));
    expect(screen.getByTestId("location")).toHaveTextContent(/^\/teams$/);
  });

  it("opens the creation dialog from the dashed card too", async () => {
    const { user } = renderPage();
    await user.click(await screen.findByRole("button", { name: new RegExp(t("teams:list.newCard.title")) }));
    expect(screen.getByRole("dialog", { name: t("teams:create.title") })).toBeInTheDocument();
  });

  it("creates a team and opens it", async () => {
    const { server, user } = renderPage({
      "POST /teams": { status: 201, data: teamResponse({ slug: "omega-a1b2", name: "Team Omega" }) },
    });
    const dialog = await openCreate(user);

    await user.click(within(dialog).getByRole("button", { name: t("teams:create.submit") }));
    expect(await within(dialog).findByText(t("teams:validation.nameRequired"))).toBeInTheDocument();

    await user.type(within(dialog).getByLabelText(t("teams:create.name")), "  Team Omega ");
    await user.click(within(dialog).getByRole("radio", { name: t("common:colors.teal") }));
    await user.click(within(dialog).getByRole("button", { name: t("teams:create.submit") }));

    await landsOn("/teams/omega-a1b2");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(lastBody(server, "POST /teams")).toEqual({ name: "Team Omega", color: "teal" });
  });

  it("puts a reserved name under the name field", async () => {
    const { user } = renderPage({ "POST /teams": { status: 422, data: { error: "team_name_reserved" } } });
    const dialog = await openCreate(user);
    await user.type(within(dialog).getByLabelText(t("teams:create.name")), "admin");
    await user.click(within(dialog).getByRole("button", { name: t("teams:create.submit") }));

    expect(await within(dialog).findByText(t("errors:api.team_name_reserved"))).toBeInTheDocument();
    expect(within(dialog).getByLabelText(t("teams:create.name"))).toHaveAttribute("aria-invalid", "true");
    expect(within(dialog).getByLabelText(t("teams:create.name"))).toHaveFocus();
  });

  it("says in the dialog that the quota is reached", async () => {
    const { user } = renderPage({ "POST /teams": { status: 409, data: { error: "team_quota_reached" } } });
    const dialog = await openCreate(user);
    await user.type(within(dialog).getByLabelText(t("teams:create.name")), "Team Omega");
    await user.click(within(dialog).getByRole("button", { name: t("teams:create.submit") }));

    expect(await within(dialog).findByText(t("errors:api.team_quota_reached"))).toBeInTheDocument();
    expect(screen.getByTestId("location")).toHaveTextContent(/^\/teams$/);
  });

  it("starts the creation form over after closing", async () => {
    const { user } = renderPage();
    let dialog = await openCreate(user);
    await user.type(within(dialog).getByLabelText(t("teams:create.name")), "Draft");
    await user.click(within(dialog).getByRole("button", { name: t("teams:create.cancel") }));

    dialog = await openCreate(user);
    expect(within(dialog).getByLabelText(t("teams:create.name"))).toHaveValue("");
  });
});
