import type { Platform, WorkspaceMode, WorkspaceSummary, WorkspaceType } from "@/lib/api/types";

export type WorkspaceTab = "all" | "active" | "archived";

export type WorkspaceFacets = Readonly<{
  types: ReadonlySet<WorkspaceType>;
  modes: ReadonlySet<WorkspaceMode>;
  platforms: ReadonlySet<Platform>;
}>;
type Facet = keyof WorkspaceFacets;
type ValueOf<F extends Facet> = WorkspaceFacets[F] extends ReadonlySet<infer V> ? V : never;

export const NO_FACETS: WorkspaceFacets = { types: new Set(), modes: new Set(), platforms: new Set() };

/** Checks or unchecks one value, in a new object so React sees the change. */
export function toggleFacet<F extends Facet>(facets: WorkspaceFacets, facet: F, value: ValueOf<F>): WorkspaceFacets {
  const next = new Set<ValueOf<F>>(facets[facet] as ReadonlySet<ValueOf<F>>);
  if (next.has(value)) next.delete(value);
  else next.add(value);
  return { ...facets, [facet]: next };
}

/** How many boxes are checked, for the badge on the button. */
export function countFacets(facets: WorkspaceFacets): number {
  return facets.types.size + facets.modes.size + facets.platforms.size;
}

// An empty set means "no filter on this facet".
const keeps = <T,>(set: ReadonlySet<T>, value: T | null) => set.size === 0 || (value !== null && set.has(value));

/** Keeps the API order (favorites first, archived last, then by name); only filters. */
export function filterWorkspaces(
  list: readonly WorkspaceSummary[],
  { tab, query, facets = NO_FACETS }: { tab: WorkspaceTab; query: string; facets?: WorkspaceFacets },
): WorkspaceSummary[] {
  const q = query.trim().toLowerCase();
  return list.filter((ws) => {
    if (tab === "active" && ws.archived) return false;
    if (tab === "archived" && !ws.archived) return false;
    if (!keeps(facets.types, ws.type) || !keeps(facets.modes, ws.mode) || !keeps(facets.platforms, ws.platform)) return false;
    if (!q) return true;
    return [ws.name, ws.slug, ws.team?.name ?? ""].some((field) => field.toLowerCase().includes(q));
  });
}
