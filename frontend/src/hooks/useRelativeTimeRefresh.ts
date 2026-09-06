// hooks/useRelativeTimeRefresh.ts

// Schedules the next re-render exactly when the relative-time label would
// change, instead of polling on a
// fixed interval. Mirrors the adaptive refresh strategy of GitHub's
// <relative-time> element without the dependency.
import { useEffect, useReducer } from "react";

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

function nextRefreshDelay(elapsedMs: number): number | null {
  if (elapsedMs < MINUTE) return 1_000; // seconds still matter
  if (elapsedMs < HOUR) return MINUTE;  // minute granularity
  if (elapsedMs < DAY) return HOUR;     // hour granularity
  return null;                          // day+ granularity, no live refresh needed
}

export function useRelativeTimeRefresh(timestamp: string) {
  const [, forceUpdate] = useReducer((n: number) => n + 1, 0);

  useEffect(() => {
    let timeoutId: ReturnType<typeof setTimeout> | undefined;

    const schedule = () => {
      const elapsed = Date.now() - new Date(timestamp).getTime();
      const delay = nextRefreshDelay(elapsed);
      if (delay === null) return;

      timeoutId = setTimeout(() => {
        forceUpdate();
        schedule();
      }, delay);
    };

    schedule();
    return () => clearTimeout(timeoutId);
  }, [timestamp]);
}
