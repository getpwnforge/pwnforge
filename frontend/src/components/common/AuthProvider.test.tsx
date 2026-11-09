// src/components/common/AuthProvider.test.tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { AUTH_ME_QUERY_KEY } from "@/api/auth";
import { AuthProvider } from "@/components/common/AuthProvider";
import { useAuth } from "@/hooks/useAuth";
import { api } from "@/lib/api";
import type { AuthContextValue } from "@/lib/auth";
import { fakeApi, type FakeReply } from "@/test/fake-api";

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

  /** Renders the provider and hands back whatever it last exposed. */
  function mount() {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const seen: { current: AuthContextValue | null } = { current: null };
    function Probe() {
      seen.current = useAuth();
      return <output data-testid="status">{seen.current.status}</output>;
    }
    render(
      <QueryClientProvider client={client}>
        <AuthProvider>
          <Probe />
        </AuthProvider>
      </QueryClientProvider>,
    );
    const status = () => screen.getByTestId("status").textContent;
    return { client, status, auth: () => seen.current as AuthContextValue };
  }

  it("reads a visitor with no session as anonymous, not as loading for good", async () => {
    fakeApi({
      "GET /auth/me": { status: 401 },
      "POST /auth/refresh": { status: 401 },
    });
    const { status } = mount();

    await waitFor(() => expect(status()).toBe("anonymous"));
  });

  it("turns anonymous when the session is lost under a signed-in user", async () => {
    fakeApi({
      "GET /auth/me": { status: 200, data: { id: "u1", username: "alice" } },
      "GET /workspaces": { status: 401 },
      "POST /auth/refresh": { status: 401 },
    });
    const { status } = mount();
    await waitFor(() => expect(status()).toBe("authenticated"));

    await api.get("/workspaces").catch(() => undefined);

    await waitFor(() => expect(status()).toBe("anonymous"));
  });

  it("lets no /auth/me answering after the logout sign the account back in", async () => {
    let answerLate: ((reply: FakeReply) => void) | undefined;
    let calls = 0;
    fakeApi({
      "GET /auth/me": () => {
        calls += 1;
        if (calls === 1) return { status: 200, data: { id: "u1", username: "alice" } };
        return new Promise<FakeReply>((resolve) => {
          answerLate = resolve;
        });
      },
      "POST /auth/logout": { status: 204 },
    });
    const { client, status, auth } = mount();
    await waitFor(() => expect(status()).toBe("authenticated"));

    // A refetch on the wire, then the logout, then its answer.
    void auth().reload();
    await waitFor(() => expect(calls).toBe(2));
    await act(() => auth().logout());
    answerLate?.({ status: 200, data: { id: "u1", username: "alice" } });
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(status()).toBe("anonymous");
    expect(client.getQueryData(AUTH_ME_QUERY_KEY)).toBeNull();
  });
});
