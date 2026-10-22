// src/lib/workspace-status.ts
import type { WorkspaceSummary } from "@/lib/api/types";

/** Where a workspace stands, for a badge or a filter. */
export type WorkspaceStatus = "archived" | "upcoming" | "live" | "ended" | "permanent";

/**
 * A workspace's status at `now`, from what the list and the detail both
 * carry. The backend has no such notion: this is the interface's reading of
 * the archive flag and the event window.
 *
 * Archived wins over the dates. With no date at all the workspace is
 * permanent (a platform, typically). Otherwise it is upcoming before
 * `starts_at`, ended from `ends_at` on, and live in between, a missing bound
 * leaving that side open.
 */
export function workspaceStatus(
  workspace: Pick<WorkspaceSummary, "archived" | "starts_at" | "ends_at">,
  now: Date,
): WorkspaceStatus {
  if (workspace.archived) return "archived";

  const starts = workspace.starts_at ? Date.parse(workspace.starts_at) : null;
  const ends = workspace.ends_at ? Date.parse(workspace.ends_at) : null;
  if (starts === null && ends === null) return "permanent";

  const at = now.getTime();
  if (starts !== null && at < starts) return "upcoming";
  if (ends !== null && at >= ends) return "ended";
  return "live";
}
