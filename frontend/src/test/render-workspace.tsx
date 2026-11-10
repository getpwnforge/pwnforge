import { Route } from "react-router";

import type { UserResponse, WorkspaceResponse } from "@/lib/api/types";
import { WorkspaceDashboardPage } from "@/pages/app/workspace/WorkspaceDashboardPage";
import { WorkspaceLayout } from "@/pages/app/workspace/WorkspaceLayout";
import { teamResponse, workspaceResponse } from "@/test/api-fixtures";
import { fakeApi } from "@/test/fake-api";
import { renderRoute } from "@/test/render-route";

export const ALPHA_TEAM = teamResponse({
  id: "01900000-0000-7000-8000-000000000001", slug: "alpha-k2m4", name: "Team Alpha", color: "blue", my_role: "admin",
});
export const HEXAGON = workspaceResponse({
  id: "00000001-0000-7000-8000-000000000001", slug: "hexagon-ab12", name: "CTF Hexagon", team_id: ALPHA_TEAM.id,
});

type Options = { workspace?: WorkspaceResponse; user?: UserResponse | null };

/**
 * Mounts /w/:slug as router.tsx does: WorkspaceLayout and its children (F5 adds members and
 * settings here). `path` follows the slug: "" for the dashboard, "/members" later. My teams
 * answer Team Alpha, which the workspace belongs to.
 */
export function renderWorkspace(path: string, routes: Parameters<typeof fakeApi>[0] = {}, { workspace = HEXAGON, user }: Options = {}) {
  const server = fakeApi({
    [`GET /workspaces/${workspace.slug}`]: { status: 200, data: workspace },
    "GET /teams": { status: 200, data: [ALPHA_TEAM] },
    ...routes,
  });
  const view = renderRoute(`/w/${workspace.slug}${path}`, (
    <>
      <Route path="/w/:slug" element={<WorkspaceLayout />}>
        <Route index element={<WorkspaceDashboardPage />} />
      </Route>
      <Route path="*" element={null} />
    </>
  ), { user });
  return { ...view, server };
}
