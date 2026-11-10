import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route } from "react-router";
import { describe, expect, it } from "vitest";

import i18n from "@/lib/i18n";
import { WorkspacesPage } from "@/pages/app/workspaces/WorkspacesPage";
import { teamSummary, workspaceInvitation, workspaceSummary } from "@/test/api-fixtures";
import { deferred, fakeApi, type FakeReply, type FakeRoute } from "@/test/fake-api";
import { renderRoute } from "@/test/render-route";

const t = (key: string, options: Record<string, unknown> = {}) => i18n.t(key, options);

const team = teamSummary({ id: "0000000a-0000-7000-8000-000000000001", slug: "alpha-k2m4", name: "Team Alpha", color: "blue" });
const hexagon = workspaceSummary({ id: "00000001-0000-7000-8000-000000000001", slug: "hexagon-ab12", name: "CTF Hexagon", my_role: "captain", team, platform: null });
const rootme = workspaceSummary({ id: "00000002-0000-7000-8000-000000000002", slug: "root-me-cd34", name: "Root-Me", my_role: "owner", mode: "solo", type: "platform", team: null, favorite: true });
const old = workspaceSummary({ id: "00000003-0000-7000-8000-000000000003", slug: "old-ctf-ef56", name: "Old CTF", my_role: "admin", team, archived: true });
const invite = workspaceInvitation({ id: "0000000e-0000-7000-8000-000000000001", workspace_name: "CTF Omega" });

/** The page with its usual answers; `routes` add to them or replace them. */
function renderPage(routes: Record<string, FakeRoute> = {}) {
  const server = fakeApi({
    "GET /workspaces": { status: 200, data: [rootme, hexagon, old] },
    "GET /workspaces/invitations/pending": { status: 200, data: [] },
    // The creation dialog is mounted closed and already lists my teams.
    "GET /teams": { status: 200, data: [] },
    ...routes,
  });
  renderRoute("/workspaces", (
    <>
      <Route path="/workspaces" element={<WorkspacesPage />} />
      <Route path="*" element={null} />
    </>
  ));
  return { server, user: userEvent.setup() };
}

/** The <section> under a group heading ("Favorites", "All workspaces"). */
function group(heading: string): HTMLElement {
  const section = screen.getByRole("heading", { name: heading }).closest("section");
  if (!section) throw new Error(`no section under "${heading}"`);
  return section;
}

const search = () => screen.getByRole("searchbox", { name: t("workspaces:list.searchPlaceholder") });
const tab = (key: string) => screen.getByRole("tab", { name: t(key) });
/** The dashed card: the only "new" button whose name also holds the hint. */
const newCard = () =>
  screen.queryByRole("button", {
    name: (name) => name.startsWith(t("workspaces:list.newCard.title")) && name.includes(t("workspaces:list.newCard.hint")),
  });

describe("WorkspacesPage", () => {
  it("shows the active workspaces and hides the archived ones by default", async () => {
    renderPage();
    expect(await screen.findByText("CTF Hexagon")).toBeInTheDocument();
    expect(screen.getByText("Root-Me")).toBeInTheDocument();
    expect(screen.queryByText("Old CTF")).not.toBeInTheDocument();
  });

  it("puts the favorites in their own group", async () => {
    renderPage();
    await screen.findByText("CTF Hexagon");

    expect(within(group(t("workspaces:list.favorites"))).getByText("Root-Me")).toBeInTheDocument();
    expect(within(group(t("workspaces:list.allWorkspaces"))).getByText("CTF Hexagon")).toBeInTheDocument();
    expect(within(group(t("workspaces:list.allWorkspaces"))).queryByText("Root-Me")).not.toBeInTheDocument();
  });

  it("shows only the archived workspaces on the archived tab, without the invitations or the dashed card", async () => {
    const { user } = renderPage();
    await screen.findByText("CTF Hexagon");

    await user.click(tab("workspaces:list.tabs.archived"));

    expect(screen.getByRole("link", { name: "Old CTF" })).toBeInTheDocument();
    expect(screen.queryByText("CTF Hexagon")).not.toBeInTheDocument();
    expect(screen.queryByText(t("common:invitations.none"))).not.toBeInTheDocument();
    expect(newCard()).not.toBeInTheDocument();
  });

  it("says so when nothing is archived", async () => {
    const { user } = renderPage({ "GET /workspaces": { status: 200, data: [hexagon] } });
    await screen.findByText("CTF Hexagon");

    await user.click(tab("workspaces:list.tabs.archived"));

    expect(screen.getByText(t("workspaces:list.noArchived"))).toBeInTheDocument();
  });

  it("searches the name, the slug and the team name", async () => {
    const { user } = renderPage();
    await screen.findByText("CTF Hexagon");

    await user.type(search(), "alpha"); // team name of CTF Hexagon
    expect(screen.getByText("CTF Hexagon")).toBeInTheDocument();
    expect(screen.queryByText("Root-Me")).not.toBeInTheDocument();

    await user.clear(search());
    await user.type(search(), "root-me-cd"); // slug of Root-Me
    expect(screen.getByText("Root-Me")).toBeInTheDocument();
    expect(screen.queryByText("CTF Hexagon")).not.toBeInTheDocument();
  });

  it("says so when nothing matches, and clears the search", async () => {
    const { user } = renderPage();
    await screen.findByText("CTF Hexagon");

    await user.type(search(), "zzz");
    expect(screen.getByText(t("workspaces:list.noMatch"))).toBeInTheDocument();
    expect(newCard()).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: t("workspaces:list.clearFilters") }));
    expect(search()).toHaveValue("");
    expect(screen.getByText("CTF Hexagon")).toBeInTheDocument();
  });

  it("offers the invitations and the dashed card when there is no workspace yet", async () => {
    renderPage({ "GET /workspaces": { status: 200, data: [] } });

    expect(await screen.findByText(t("common:invitations.none"))).toBeInTheDocument();
    expect(newCard()).toBeInTheDocument();
  });

  it("lists a pending workspace invitation in the grid", async () => {
    renderPage({ "GET /workspaces/invitations/pending": { status: 200, data: [invite] } });

    expect(await screen.findByRole("button", { name: t("common:invitations.acceptNamed", { name: "CTF Omega" }) })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: t("common:invitations.rejectNamed", { name: "CTF Omega" }) })).toBeInTheDocument();
  });

  it("shows the error the API answered", async () => {
    renderPage({ "GET /workspaces": { status: 503, data: { error: "service_unavailable" } } });
    expect(await screen.findByText(t("errors:api.service_unavailable"))).toBeInTheDocument();
  });

  it("stars a workspace before the server answers, and undoes it on failure", async () => {
    const reply = deferred<FakeReply>();
    const { server, user } = renderPage({
      "GET /workspaces": { status: 200, data: [hexagon] },
      "POST /workspaces/hexagon-ab12/favorite": () => reply.promise,
    });
    await screen.findByText("CTF Hexagon");

    await user.click(screen.getByRole("button", { name: t("workspaces:card.favorite") }));

    // Optimistic: starred while the request is still held by the test.
    expect(screen.getByRole("button", { name: t("workspaces:card.unfavorite") })).toHaveAttribute("aria-pressed", "true");
    expect(server.count("POST /workspaces/hexagon-ab12/favorite")).toBe(1);

    reply.resolve({ status: 500, data: { error: "internal_error" } });

    await waitFor(() =>
      expect(screen.getByRole("button", { name: t("workspaces:card.favorite") })).toHaveAttribute("aria-pressed", "false"),
    );
  });

  it("offers archiving to the Captain and not to an Admin", async () => {
    const { user } = renderPage({ "GET /workspaces": { status: 200, data: [hexagon, old] } });
    await screen.findByText("CTF Hexagon");
    await user.click(tab("workspaces:list.tabs.all"));

    await user.click(screen.getByRole("button", { name: t("workspaces:card.menu", { name: "CTF Hexagon" }) }));
    expect(screen.getByRole("menuitem", { name: t("workspaces:card.archive") })).toBeInTheDocument();
    await user.keyboard("{Escape}");

    await user.click(screen.getByRole("button", { name: t("workspaces:card.menu", { name: "Old CTF" }) }));
    expect(screen.queryByRole("menuitem", { name: t("workspaces:card.unarchive") })).not.toBeInTheDocument();
  });

  it("opens the creation dialog from the header and from the dashed card, without leaving the page", async () => {
    const { user } = renderPage();
    await screen.findByText("CTF Hexagon");

    await user.click(screen.getByRole("button", { name: t("workspaces:list.newWorkspace") }));
    expect(screen.getByRole("dialog", { name: t("workspaces:create.title") })).toBeInTheDocument();
    await user.keyboard("{Escape}");

    const dashed = newCard();
    if (!dashed) throw new Error("no dashed card");
    await user.click(dashed);
    expect(screen.getByRole("dialog", { name: t("workspaces:create.title") })).toBeInTheDocument();
    expect(screen.getByTestId("location")).toHaveTextContent(/^\/workspaces$/);
  });

  it("opens a workspace from its name", async () => {
    const { user } = renderPage({ "GET /workspaces": { status: 200, data: [hexagon] } });

    await user.click(await screen.findByRole("link", { name: "CTF Hexagon" }));

    expect(screen.getByTestId("location")).toHaveTextContent("/w/hexagon-ab12");
  });
});
