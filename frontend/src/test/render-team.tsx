import { Route } from "react-router";

import type { TeamResponse, UserResponse } from "@/lib/api/types";
import { TeamPage } from "@/pages/app/teams/TeamPage";
import { teamResponse } from "@/test/api-fixtures";
import { fakeApi } from "@/test/fake-api";
import { renderRoute } from "@/test/render-route";

export const ALPHA = teamResponse({
  id: "0000000a-0000-7000-8000-000000000001", slug: "alpha-k2m4", name: "Team Alpha", color: "blue",
  my_role: "captain", member_count: 2, avatar_version: null,
});

type Options = { team?: TeamResponse; user?: UserResponse | null };

/**
 * Mounts /teams/:slug as router.tsx does, at `/teams/<slug><search>` (`""` for Members,
 * `"?tab=settings"` for a tab). `routes` add to (or override) the team's own GET.
 */
export function renderTeam(search: string, routes: Parameters<typeof fakeApi>[0] = {}, { team = ALPHA, user }: Options = {}) {
  const server = fakeApi({ [`GET /teams/${team.slug}`]: { status: 200, data: team }, ...routes });
  const view = renderRoute(`/teams/${team.slug}${search}`, (
    <>
      <Route path="/teams/:slug" element={<TeamPage />} />
      <Route path="*" element={null} />
    </>
  ), { user });
  return { ...view, server };
}
