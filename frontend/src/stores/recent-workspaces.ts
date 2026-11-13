// src/stores/recent-workspaces.ts
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

/** How many recent workspaces are kept. */
export const RECENT_WORKSPACES_LIMIT = 5;

/** Where they are kept, in this browser only. */
export const RECENT_WORKSPACES_STORAGE_KEY = "pwnforge-recent-workspaces";

// The shape the backend gives slugs: lowercase words joined by hyphens.
const SLUG = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const SLUG_MAX = 128;

function isSlug(value: unknown): value is string {
  return typeof value === "string" && value.length <= SLUG_MAX && SLUG.test(value);
}

/**
 * What a stored value holds of a list of recent slugs: the slugs, in order,
 * without repeats, at most `RECENT_WORKSPACES_LIMIT`. Anything else, a value
 * that is no list, or an entry that is no slug, is dropped: storage is
 * whatever another version, an extension or a hand left there.
 */
export function readRecentSlugs(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  const slugs: string[] = [];
  for (const entry of value) {
    if (isSlug(entry) && !slugs.includes(entry)) slugs.push(entry);
    if (slugs.length === RECENT_WORKSPACES_LIMIT) break;
  }
  return slugs;
}

interface RecentWorkspacesState {
  /** Most recent first. */
  slugs: string[];
  /** Puts `slug` first, once, dropping the oldest past the limit. */
  visit: (slug: string) => void;
  /** Drops `slug`: it answered 404, or was left or deleted. */
  forget: (slug: string) => void;
}

/**
 * The workspaces visited last, by slug and nothing else: names, colours and
 * access come from the workspace list, so nothing stored here can show a
 * workspace the caller no longer sees.
 */
export const useRecentWorkspaces = create<RecentWorkspacesState>()(
  persist(
    (set) => ({
      slugs: [],
      visit: (slug) =>
        set((state) =>
          isSlug(slug) ? { slugs: readRecentSlugs([slug, ...state.slugs]) } : state,
        ),
      forget: (slug) => set((state) => ({ slugs: state.slugs.filter((kept) => kept !== slug) })),
    }),
    {
      name: RECENT_WORKSPACES_STORAGE_KEY,
      storage: createJSONStorage(() => localStorage),
      partialize: (state) => ({ slugs: state.slugs }),
      // What storage holds is read through `readRecentSlugs`, never taken as is.
      merge: (persisted, current) => ({
        ...current,
        slugs: readRecentSlugs((persisted as { slugs?: unknown } | null)?.slugs),
      }),
    },
  ),
);

/** Drops `slug` from the recent workspaces, outside React. */
export function forgetRecentWorkspace(slug: string): void {
  useRecentWorkspaces.getState().forget(slug);
}
