import { cleanup, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import i18n from "@/lib/i18n";
import { ROUTES } from "@/lib/routes";
import { InvitationLinkPage } from "@/pages/invitations/InvitationLinkPage";
import { teamResponse } from "@/test/api-fixtures";
import { fakeApi, lastBody, type FakeRoute } from "@/test/fake-api";
import { renderRoute } from "@/test/render-route";

const t = (key: string, options: Record<string, unknown> = {}) => i18n.t(key, options);
const TOKEN = "aK7d9Qm2vT4xR8wPq3Zt";
// To check against src/api/calls.ts: the preview sends the token in its body.
const PREVIEW = "POST /teams/invitations/preview";
const JOIN = "POST /teams/join";
const PREVIEWED = {
  team_name: "Les Croquettes",
  role: "member",
  invited_by_username: "l3a",
  expires_at: new Date(Date.now() + 5 * 86_400_000).toISOString(),
};

/**
 * Mounts the team link page. useLinkToken reads the real address bar, not the router's location:
 * the fragment goes there, and the router starts on the bare path.
 */
function renderLink(hash: string, routes: Record<string, FakeRoute> = {}, options?: Parameters<typeof renderRoute>[2]) {
  window.history.replaceState(null, "", `${ROUTES.teamInvitation}${hash}`);
  const server = fakeApi(routes);
  renderRoute(ROUTES.teamInvitation, (
    <>
      <Route path={ROUTES.teamInvitation} element={<InvitationLinkPage kind="team" />} />
      <Route path="*" element={null} />
    </>
  ), options);
  return { server, user: userEvent.setup() };
}
const withToken = `#token=${TOKEN}`;

describe("InvitationLinkPage", () => {
  afterEach(() => window.sessionStorage.clear());

  it("says the link is incomplete without a token", () => {
    renderLink("");
    expect(screen.getByText(t("common:invitationLink.missing.title"))).toBeInTheDocument();
  });

  it("asks a visitor to sign in, and asks the API nothing", () => {
    const { server } = renderLink(withToken, {}, { user: null });
    expect(screen.getByText(t("common:invitationLink.anonymous.title"))).toBeInTheDocument();
    expect(screen.getByRole("link", { name: t("common:invitationLink.anonymous.signIn") })).toHaveAttribute(
      "href",
      `${ROUTES.login}?from=${encodeURIComponent(ROUTES.teamInvitation)}`,
    );
    expect(server.count(PREVIEW)).toBe(0);
  });

  it("comes back from signing in with the token", async () => {
    renderLink(withToken, {}, { user: null });
    cleanup();

    // Back from /login: the bare path, signed in.
    renderLink("", { [PREVIEW]: { status: 200, data: PREVIEWED } });

    expect(await screen.findByRole("heading", { name: t("common:invitationLink.team.title", { name: "Les Croquettes" }) })).toBeInTheDocument();
  });

  it("shows who invites to which team, with the role given", async () => {
    renderLink(withToken, { [PREVIEW]: { status: 200, data: PREVIEWED } });
    expect(await screen.findByRole("heading", { name: t("common:invitationLink.team.title", { name: "Les Croquettes" }) })).toBeInTheDocument();
    expect(screen.getByText(t("common:invitationLink.team.lead", { author: "@l3a" }))).toBeInTheDocument();
    expect(screen.getByText(t("common:roles.member"))).toBeInTheDocument();
  });

  it("keeps the token out of the address bar", async () => {
    renderLink(withToken, { [PREVIEW]: { status: 200, data: PREVIEWED } });
    await screen.findByRole("heading", { name: t("common:invitationLink.team.title", { name: "Les Croquettes" }) });
    expect(window.location.href).not.toContain(TOKEN);
  });

  it("joins, then opens the team", async () => {
    const { server, user } = renderLink(withToken, {
      [PREVIEW]: { status: 200, data: PREVIEWED },
      [JOIN]: { status: 200, data: teamResponse({ slug: "croquettes-k2m4", name: "Les Croquettes" }) },
    });
    await user.click(await screen.findByRole("button", { name: t("common:invitationLink.team.join") }));

    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(/^\/teams\/croquettes-k2m4$/));
    // Used up: nothing left for this tab to replay.
    expect(window.sessionStorage).toHaveLength(0);
    expect(lastBody(server, JOIN)).toEqual({ token: TOKEN });
  });

  it("leads an existing member to the teams", async () => {
    const { user } = renderLink(withToken, {
      [PREVIEW]: { status: 200, data: PREVIEWED },
      [JOIN]: { status: 409, data: { error: "already_member" } },
    });
    await user.click(await screen.findByRole("button", { name: t("common:invitationLink.team.join") }));

    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(/^\/teams$/));
  });

  it("says the link no longer works on a 404, whatever the reason", async () => {
    renderLink(withToken, { [PREVIEW]: { status: 404, data: { error: "not_found" } } });
    expect(await screen.findByText(t("common:invitationLink.invalid.title"))).toBeInTheDocument();
    expect(screen.getByRole("link", { name: t("common:invitationLink.invalid.pending") })).toHaveAttribute("href", ROUTES.teams);
    await waitFor(() => expect(window.sessionStorage).toHaveLength(0));
  });
});
