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
 * Rotates the session, at most once at a time.
 *
 * The backend revokes the presented refresh token on every rotation and treats
 * a revoked token as a replay — it then kills the whole chain (see
 * `session_service`, reuse detection). Two concurrent 401s must therefore share
 * one rotation instead of each starting their own, otherwise the second one
 * logs the user out of every device.
 */
function refreshSession(): Promise<void> {
  let pending = refreshInFlight;

  if (!pending) {
    pending = api
      .post("/auth/refresh", null, { skipAuthRefresh: true })
      .then(() => undefined)
      .finally(() => {
        refreshInFlight = null;
      });

    refreshInFlight = pending;
  }

  return pending;
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

    try {
      await refreshSession();
    } catch {
      onUnauthenticated?.();
      // Surface the original failure, not the refresh's: the caller asked for
      // the first request.
      throw apiError;
    }

    return api.request(config);
  },
);
