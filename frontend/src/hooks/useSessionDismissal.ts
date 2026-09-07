// src/hooks/useSessionDismissal.ts
import { useState } from "react";

/**
 * Tracks which banners the user closed during this browser session.
 *
 * `sessionStorage`, not `localStorage`: a dismissal must not outlive the
 * session. For the legal banner in particular, permanently hiding the notice
 * would deprive the user of the very information tacit acceptance relies on.
 *
 * Dismissals are keyed per item, never a single boolean flag, so a *new*
 * alert or a *second* revision still surfaces after an earlier one was
 * closed.
 */
export function useSessionDismissal(storageKey: string) {
  // Read once at mount: sessionStorage does not change from outside this tab.
  const [dismissed, setDismissed] = useState<string[]>(() => read(storageKey));

  const dismiss = (key: string) => {
    setDismissed((current) => {
      if (current.includes(key)) return current;

      const next = [...current, key];
      try {
        sessionStorage.setItem(storageKey, JSON.stringify(next));
      } catch {
        // Private mode or a full quota: keep the in-memory state so the
        // banner still closes for this render, and let it come back on
        // reload rather than breaking the shell.
      }
      return next;
    });
  };

  const isDismissed = (key: string) => dismissed.includes(key);

  return { isDismissed, dismiss };
}

function read(storageKey: string): string[] {
  try {
    const raw = sessionStorage.getItem(storageKey);
    if (!raw) return [];

    const parsed: unknown = JSON.parse(raw);
    // A value written by an older build was a bare id string, not an array.
    // Treat anything unexpected as "nothing dismissed" rather than throwing
    // during the initial render.
    return Array.isArray(parsed) ? parsed.filter((v) => typeof v === "string") : [];
  } catch {
    return [];
  }
}
