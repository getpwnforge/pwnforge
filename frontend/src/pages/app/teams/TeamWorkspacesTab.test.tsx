import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import i18n from "@/lib/i18n";
import { teamMember, teamSummary, workspaceSummary } from "@/test/api-fixtures";
import { ALPHA, renderTeam } from "@/test/render-team";

const t = (key: string, options?: Record<string, unknown>) => i18n.t(key, options);
const alphaRef = teamSummary({ id: ALPHA.id, slug: ALPHA.slug, name: ALPHA.name, color: ALPHA.color });
const hexagon = workspaceSummary({ id: "00000001-0000-7000-8000-000000000001", slug: "hexagon-ab12", name: "CTF Hexagon", team: alphaRef });
const old = workspaceSummary({ id: "00000003-0000-7000-8000-000000000003", slug: "old-ctf-ef56", name: "Old CTF", team: alphaRef, archived: true });
const rootme = workspaceSummary({ id: "00000002-0000-7000-8000-000000000002", slug: "root-me-cd34", name: "Root-Me", mode: "solo", team: null });

const ROUTES = {
  "GET /workspaces": { status: 200, data: [hexagon, old, rootme] },
  "GET /teams/alpha-k2m4/members": { status: 200, data: [teamMember()] },
  "GET /teams": { status: 200, data: [ALPHA] },
};
const newButton = () => screen.queryByRole("button", { name: t("teams:workspaces.new") });

describe("TeamWorkspacesTab", () => {
  it("lists only the team's workspaces, with their status", async () => {
    renderTeam("?tab=workspaces", ROUTES);
    expect(await screen.findByRole("link", { name: "CTF Hexagon" })).toHaveAttribute("href", "/w/hexagon-ab12");
    expect(screen.getByRole("link", { name: "Old CTF" })).toBeInTheDocument();
    expect(screen.queryByText("Root-Me")).not.toBeInTheDocument();

    const oldRow = screen.getByRole("link", { name: "Old CTF" }).closest("tr");
    if (!oldRow) throw new Error("no row for Old CTF");
    expect(within(oldRow).getByText(t("workspaces:status.archived"))).toBeInTheDocument();
  });

  it("filters by name", async () => {
    const user = userEvent.setup();
    renderTeam("?tab=workspaces", ROUTES);
    await screen.findByRole("link", { name: "CTF Hexagon" });

    await user.type(screen.getByRole("searchbox", { name: t("teams:workspaces.filter") }), "old");
    expect(screen.queryByRole("link", { name: "CTF Hexagon" })).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Old CTF" })).toBeInTheDocument();
  });

  it("filters by status, and says when nothing matches", async () => {
    const user = userEvent.setup();
    renderTeam("?tab=workspaces", ROUTES);
    await screen.findByRole("link", { name: "CTF Hexagon" });

    await user.click(screen.getByRole("button", { name: (name) => name.startsWith(t("teams:workspaces.status")) }));
    await user.click(screen.getByRole("menuitemradio", { name: t("workspaces:status.archived") }));
    expect(screen.queryByRole("link", { name: "CTF Hexagon" })).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Old CTF" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: (name) => name.startsWith(t("teams:workspaces.status")) }));
    await user.click(screen.getByRole("menuitemradio", { name: t("workspaces:status.live") }));
    expect(screen.getByText(t("teams:workspaces.noMatch"))).toBeInTheDocument();
  });

  it("lets the Captain start a workspace for this team, already chosen", async () => {
    const user = userEvent.setup();
    renderTeam("?tab=workspaces", ROUTES);
    await screen.findByRole("link", { name: "CTF Hexagon" });

    await user.click(newButton()!);
    const dialog = screen.getByRole("dialog", { name: t("workspaces:create.title") });
    const team = within(dialog).getByRole("radio", { name: (name) => name.startsWith(t("workspaces:create.mode.team")) });
    expect(team).toBeChecked();
    await waitFor(() =>
      expect(within(dialog).getByRole("combobox", { name: t("workspaces:create.mode.pickTeam") })).toHaveTextContent(ALPHA.name),
    );
  });

  it("does not offer it to a Member by default", async () => {
    renderTeam("?tab=workspaces", ROUTES, { team: { ...ALPHA, my_role: "member" } });
    await screen.findByRole("link", { name: "CTF Hexagon" });
    expect(newButton()).not.toBeInTheDocument();
  });

  it("offers it to a Member when the team allows it", async () => {
    renderTeam("?tab=workspaces", ROUTES, { team: { ...ALPHA, my_role: "member", members_can_create_workspaces: true } });
    await screen.findByRole("link", { name: "CTF Hexagon" });
    expect(newButton()).toBeInTheDocument();
  });

  it("says when the team has no workspace yet", async () => {
    renderTeam("?tab=workspaces", { ...ROUTES, "GET /workspaces": { status: 200, data: [rootme] } });
    expect(await screen.findByText(t("teams:workspaces.empty"))).toBeInTheDocument();
  });
});
