import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import i18n from "@/lib/i18n";
import { teamMember } from "@/test/api-fixtures";
import { ALPHA, renderTeam } from "@/test/render-team";

const t = (key: string, options: Record<string, unknown> = {}) => i18n.t(key, options);
const MEMBERS = { "GET /teams/alpha-k2m4/members": { status: 200, data: [teamMember()] } };
const tabs = () => within(screen.getByRole("tablist", { name: t("teams:detail.tabsLabel") }));
/** A tab by its label; counts follow the label in the same tab. */
const tab = (key: string) => tabs().getByRole("tab", { name: (name) => name.startsWith(t(key)) });

describe("TeamPage", () => {
  it("shows the team and its tabs, Members first and open", async () => {
    renderTeam("", MEMBERS);
    expect(await screen.findByRole("heading", { name: "Team Alpha" })).toBeInTheDocument();
    expect(screen.getByText(t("teams:card.members", { count: 2 }))).toBeInTheDocument();

    expect(tabs().getAllByRole("tab").map((el) => el.firstChild?.textContent)).toEqual([
      t("teams:detail.tabs.members"), t("teams:detail.tabs.workspaces"), t("teams:detail.tabs.roles"),
      t("teams:detail.tabs.invitations"), t("teams:detail.tabs.settings"),
    ]);
    expect(tab("teams:detail.tabs.members")).toHaveAttribute("aria-selected", "true");
  });

  it("opens the tab named in ?tab=", async () => {
    renderTeam("?tab=settings", MEMBERS);
    await screen.findByRole("heading", { name: "Team Alpha" });
    expect(tab("teams:detail.tabs.settings")).toHaveAttribute("aria-selected", "true");
  });

  it("puts the open tab in the URL, and Members back to a bare URL", async () => {
    const user = userEvent.setup();
    renderTeam("", MEMBERS);
    await screen.findByRole("heading", { name: "Team Alpha" });

    await user.click(tab("teams:detail.tabs.workspaces"));
    expect(screen.getByTestId("location")).toHaveTextContent("/teams/alpha-k2m4?tab=workspaces");

    await user.click(tab("teams:detail.tabs.members"));
    expect(screen.getByTestId("location")).toHaveTextContent(/^\/teams\/alpha-k2m4$/);
  });

  it("hides the Invitations tab from a Member, and falls back to Members when asked for it", async () => {
    renderTeam("?tab=invitations", MEMBERS, { team: { ...ALPHA, my_role: "member" } });
    await screen.findByRole("heading", { name: "Team Alpha" });
    expect(tabs().queryByRole("tab", { name: (name) => name.startsWith(t("teams:detail.tabs.invitations")) })).not.toBeInTheDocument();
    expect(tab("teams:detail.tabs.members")).toHaveAttribute("aria-selected", "true");
  });

  it("falls back to Members on an unknown tab", async () => {
    renderTeam("?tab=nope", MEMBERS);
    await screen.findByRole("heading", { name: "Team Alpha" });
    expect(tab("teams:detail.tabs.members")).toHaveAttribute("aria-selected", "true");
  });

  it("says the team is not found on a 404", async () => {
    renderTeam("", { "GET /teams/alpha-k2m4": { status: 404, data: { error: "team_not_found" } } });
    expect(await screen.findByText(t("teams:notFound.title"))).toBeInTheDocument();
    expect(screen.queryByRole("tablist")).not.toBeInTheDocument();
  });

  it("shows any other error as it came", async () => {
    renderTeam("", { "GET /teams/alpha-k2m4": { status: 503, data: { error: "service_unavailable" } } });
    expect(await screen.findByText(t("errors:api.service_unavailable"))).toBeInTheDocument();
  });
});
