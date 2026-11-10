import { useOutletContext } from "react-router";

import type { WorkspaceResponse } from "@/lib/api/types";

/**
 * The workspace WorkspaceLayout loaded, for the pages under /w/:slug. They read it here and
 * never call useWorkspace again: the layout already holds it, archived or not.
 */
export function useCurrentWorkspace(): WorkspaceResponse {
  return useOutletContext<WorkspaceResponse>();
}
