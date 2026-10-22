// src/api/paths.ts
import type { InvitedByQuery } from "@/lib/api/types";

/**
 * One path segment, encoded. Slugs and ids never need it today, but a value
 * taken from the address bar must not be able to add a segment of its own.
 */
export function segment(value: string): string {
  return encodeURIComponent(value);
}

/**
 * `path` with the `invited_by` filter the member and invitation lists take,
 * when one is given. Built into the URL rather than passed as axios
 * `params`, so a request reads the same in the network panel and in the
 * tests' fake server.
 */
export function withInvitedBy(path: string, query?: InvitedByQuery): string {
  if (!query?.invited_by) return path;
  return `${path}?${new URLSearchParams({ invited_by: query.invited_by })}`;
}
