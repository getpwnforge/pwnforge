// src/lib/api.ts
import axios, { type InternalAxiosRequestConfig } from "axios";
import { toApiError } from "@/lib/api-error";

/** Same origin: the dev server proxies `/api` to the backend (see vite.config.ts). */
export const API_BASE_URL = "/api/v1";

declare module "axios" {
  export interface AxiosRequestConfig {
    /**
     * Opt a request out of the refresh-and-replay dance. Set on the refresh
     * call itself, and on anything that legitimately answers 401 without the
     * session being at fault.
     */
    skipAuthRefresh?: boolean;
  }
}

export const api = axios.create({
  baseURL: API_BASE_URL,
  // The session lives in httpOnly cookies; nothing readable from JS is ever
  // attached to a request.
  withCredentials: true,
  headers: { "Content-Type": "application/json" },
});

/**
 * Routes where a 401 is an answer, not an expired access token.
 *
 * Refreshing on those would turn a wrong password into two requests and, for
 * `/auth/refresh` itself, an unbounded recursion.
 */
const NO_REFRESH_PATHS = [
  "/auth/login",
  "/auth/register",
  "/auth/refresh",
  "/auth/logout",
  "/auth/password/forgot",
  "/auth/password/reset",
  "/auth/email/verify",
  "/setup",
];

function isRefreshable(url: string | undefined): boolean {
  if (!url) return false;
  // Tolerate an absolute URL as well as the usual relative one.
  const path = url.startsWith(API_BASE_URL) ? url.slice(API_BASE_URL.length) : url;
  return !NO_REFRESH_PATHS.some((prefix) => path.startsWith(prefix));
}

let refreshInFlight: Promise<void> | null = null;

/**
 * Bumped when the session is ended on purpose (see `endSession`): a refresh
 * that started before must not replay its request once it lands.
 */
let sessionGeneration = 0;

/** Set while a logout is under way: no refresh may start then. */
let endingSession = false;

/** Shared by every tab of this origin. */
const REFRESH_LOCK = "pwnforge-auth-refresh";

/**
 * One rotation across every tab: the next waits for the cookie the previous
 * one set, instead of presenting the same refresh token a moment later.
 * Without the Web Locks API, older browsers or a page served over plain
 * HTTP, only this tab is serialized; the backend then refuses a second tab
 * arriving with the token just rotated, with a 401 rather than by revoking
 * every session (see `session_service::ROTATION_GRACE_SECS`).
 */
function rotate(): Promise<void> {
  const call = () =>
    api.post("/auth/refresh", null, { skipAuthRefresh: true }).then(() => undefined);

  if (typeof navigator !== "undefined" && "locks" in navigator) {
    return navigator.locks.request(REFRESH_LOCK, call);
  }

  return call();
}

/**
 * Rotates the session, at most once at a time.
 *
 * The backend revokes the presented refresh token on every rotation and treats
 * a token revoked more than a few seconds ago as a replay, killing the whole
 * chain (see `session_service`, reuse detection). Two concurrent 401s must
 * therefore share one rotation instead of each starting their own.
 */
function refreshSession(): Promise<void> {
  if (endingSession) {
    return Promise.reject(new Error("the session is being ended"));
  }

  let pending = refreshInFlight;

  if (!pending) {
    pending = rotate().finally(() => {
      refreshInFlight = null;
    });

    refreshInFlight = pending;
  }

  return pending;
}

/**
 * Ends the session through `request` (the logout call), after any refresh
 * in flight: its new cookies land first, so the logout clears them and
 * revokes the token they carry, rather than the refresh restoring them
 * once the logout is done. No refresh starts until the call has answered,
 * and a request waiting on an earlier refresh is not replayed.
 */
export async function endSession(request: () => Promise<unknown>): Promise<void> {
  sessionGeneration += 1;
  endingSession = true;

  try {
    await refreshInFlight?.catch(() => undefined);
    await request();
  } finally {
    endingSession = false;
  }
}

type UnauthenticatedHandler = () => void;

let onUnauthenticated: UnauthenticatedHandler | null = null;

/**
 * Registers the callback fired when the session is gone for good — refresh
 * refused, or a 401 on a route that cannot be retried.
 *
 * Returns an unsubscribe function, so it can be used directly as a
 * `useEffect` body.
 */
export function setUnauthenticatedHandler(handler: UnauthenticatedHandler): () => void {
  onUnauthenticated = handler;

  return () => {
    if (onUnauthenticated === handler) onUnauthenticated = null;
  };
}

type RetriedConfig = InternalAxiosRequestConfig & { retriedAfterRefresh?: boolean };

api.interceptors.response.use(
  (response) => response,
  async (error: unknown) => {
    const apiError = toApiError(error);
    const config = axios.isAxiosError(error)
      ? (error.config as RetriedConfig | undefined)
      : undefined;

    if (apiError.status !== 401 || !config || config.skipAuthRefresh || !isRefreshable(config.url)) {
      throw apiError;
    }

    // Replayed once already and still refused: the access token was not the
    // problem, so the session is gone.
    if (config.retriedAfterRefresh) {
      onUnauthenticated?.();
      throw apiError;
    }

    // One attempt per request, whatever the outcome.
    config.retriedAfterRefresh = true;
    const generation = sessionGeneration;

    try {
      await refreshSession();
    } catch {
      onUnauthenticated?.();
      // Surface the original failure, not the refresh's: the caller asked for
      // the first request.
      throw apiError;
    }

    // The session was ended while this one waited: the request belonged to
    // it, and must not go out under whatever comes next.
    if (generation !== sessionGeneration) {
      throw apiError;
    }

    return api.request(config);
  },
);
