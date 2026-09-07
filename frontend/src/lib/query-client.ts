// src/lib/query-client.ts
import { QueryClient } from "@tanstack/react-query";

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
