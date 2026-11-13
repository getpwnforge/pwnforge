// src/lib/query-client.ts
import { QueryClient } from "@tanstack/react-query";
import { isApiError } from "@/lib/api-error";

/**
 * Shared query client.
 *
 * Exported as a module singleton rather than built inside `App`: the auth layer
 * needs to drop cached data when a session ends, and that happens outside the
 * React tree (see `setUnauthenticatedHandler`).
 */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      // A 401 is answered by the axios interceptor, not by retrying; anything
      // else gets one second chance.
      retry: 1,
    },
  },
});

/**
 * `retry` for a query naming one resource: a 404 is an answer, that it does
 * not exist or that the caller has no access to it, and asking again would
 * not change it. Anything else gets the default's one second chance.
 */
export function retryUnlessNotFound(failureCount: number, error: unknown): boolean {
  return !(isApiError(error) && error.status === 404) && failureCount < 1;
}
