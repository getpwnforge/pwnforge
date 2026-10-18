// src/components/common/AuthProvider.test.tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { AUTH_ME_QUERY_KEY } from "@/api/auth";
import { AuthProvider } from "@/components/common/AuthProvider";
import { api } from "@/lib/api";
import { fakeApi } from "@/test/fake-api";

describe("AuthProvider", () => {
  it("empties the whole query cache when the session is lost", async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    fakeApi({
      "GET /auth/me": { status: 200, data: { id: "u1", username: "alice" } },
      "GET /workspaces": { status: 401 },
      // The refresh is refused: the session is gone for good.
      "POST /auth/refresh": { status: 401 },
    });
    client.setQueryData(["workspaces"], [{ slug: "ctf-2026" }]);
    client.setQueryData(["legal", "status"], { state: "ok" });

    render(
      <QueryClientProvider client={client}>
        <AuthProvider>
          <div />
        </AuthProvider>
      </QueryClientProvider>,
    );
    await waitFor(() => expect(client.getQueryData(AUTH_ME_QUERY_KEY)).toMatchObject({ username: "alice" }));

    await api.get("/workspaces").catch(() => undefined);

    // Only the answer "nobody is signed in" is left for the next account.
    await waitFor(() =>
      expect(client.getQueryCache().getAll().map((query) => query.queryKey)).toEqual([
        AUTH_ME_QUERY_KEY,
      ]),
    );
    expect(client.getQueryData(AUTH_ME_QUERY_KEY)).toBeNull();
  });
});
