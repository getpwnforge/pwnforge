import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import i18n from "@/lib/i18n";
import { effectiveMember } from "@/test/api-fixtures";
import { HEXAGON, renderWorkspace } from "@/test/render-workspace";

const t = (key: string, options?: Record<string, unknown>) => i18n.t(key, options);

describe("WorkspaceDashboardPage", () => {
  it("names the workspace and links its team", async () => {
    renderWorkspace("");
    expect(await screen.findByRole("heading", { name: "CTF Hexagon" })).toBeInTheDocument();
    expect(await screen.findByRole("link", { name: "Team Alpha" })).toHaveAttribute("href", "/teams/alpha-k2m4");
  });

  it("stacks the members, counted as one group past the first five", async () => {
    const members = Array.from({ length: 7 }, (_, i) =>
      effectiveMember({ user_id: `0000000f-0000-7000-8000-00000000001${i}`, handle: `m${i}`, first_name: `Member${i}`, last_name: "" }),
    );
    renderWorkspace("", { [`GET /workspaces/${HEXAGON.slug}/members`]: { status: 200, data: members } });

    expect(await screen.findByRole("img", { name: t("workspaces:dashboard.members", { count: 7 }) })).toBeInTheDocument();
    expect(screen.getByText("+2")).toBeInTheDocument();
  });

  it("says solo for a workspace without a team", async () => {
    renderWorkspace("", {}, { workspace: { ...HEXAGON, team_id: null, mode: "solo", my_role: "owner" } });
    expect(await screen.findByText(t("workspaces:dashboard.solo"))).toBeInTheDocument();
  });

  it("says team without a name to a guest outside the team", async () => {
    renderWorkspace("", { "GET /teams": { status: 200, data: [] } });
    await screen.findByRole("heading", { name: "CTF Hexagon" });
    expect(await screen.findByText(t("workspaces:dashboard.team"))).toBeInTheDocument();
  });

  it("shows the platform and the dates", async () => {
    renderWorkspace("", {}, {
      workspace: { ...HEXAGON, type: "platform", platform: "root_me", starts_at: "2026-11-14T09:00:00Z", ends_at: "2026-11-16T18:00:00Z" },
    });
    expect(await screen.findByText(t("nav:platforms.root_me"))).toBeInTheDocument();
    expect(screen.getByText(/→/)).toBeInTheDocument();
  });

  it("announces the challenges to come", async () => {
    renderWorkspace("");
    expect(await screen.findByText(t("workspaces:dashboard.emptyTitle"))).toBeInTheDocument();
  });
});
