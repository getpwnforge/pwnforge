import { screen } from "@testing-library/react";
import { Route } from "react-router";
import { describe, expect, it, vi } from "vitest";

import { WorkspaceArchivedBanner } from "@/components/layout/WorkspaceArchivedBanner";
import i18n from "@/lib/i18n";
import type { WorkspaceResponse } from "@/lib/api/types";
import { workspaceResponse } from "@/test/api-fixtures";
import { fakeApi } from "@/test/fake-api";
import { renderRoute } from "@/test/render-route";

const t = (key: string, options: Record<string, unknown> = {}) => i18n.t(key, options);
const HEXAGON = workspaceResponse({ slug: "hexagon-ab12", name: "CTF Hexagon" });

function renderBanner(url: string, workspace: WorkspaceResponse = HEXAGON) {
  const server = fakeApi({ [`GET /workspaces/${workspace.slug}`]: { status: 200, data: workspace } });
  renderRoute(url, <Route path="*" element={<WorkspaceArchivedBanner />} />);
  return server;
}

describe("WorkspaceArchivedBanner", () => {
  it("says an archived workspace is read-only", async () => {
    renderBanner("/w/hexagon-ab12", { ...HEXAGON, archived: true });
    expect(await screen.findByText(t("workspaces:archived.banner"))).toBeInTheDocument();
  });

  it("stays away from a live workspace", async () => {
    const server = renderBanner("/w/hexagon-ab12");
    await vi.waitFor(() => expect(server.count("GET /workspaces/hexagon-ab12")).toBe(1));
    expect(screen.queryByText(t("workspaces:archived.banner"))).not.toBeInTheDocument();
  });

  it("asks nothing outside a workspace", () => {
    const server = renderBanner("/workspaces");
    expect(server.count("GET /workspaces/hexagon-ab12")).toBe(0);
    expect(screen.queryByText(t("workspaces:archived.banner"))).not.toBeInTheDocument();
  });
});
