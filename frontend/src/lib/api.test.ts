// src/lib/api.test.ts
import { describe, expect, it, vi } from "vitest";

import { api, endSession } from "@/lib/api";
import { deferred, fakeApi, type FakeReply } from "@/test/fake-api";

const logout = () => api.post("/auth/logout", null, { skipAuthRefresh: true });

/** Refused once with a 401, as an expired access token is, then answered. */
function expiredOnce(): () => FakeReply {
  let calls = 0;
  return () => (calls++ === 0 ? { status: 401 } : { status: 200, data: {} });
}

describe("endSession", () => {
  it("waits for the refresh in flight before sending the logout", async () => {
    const refresh = deferred<FakeReply>();
    const order: string[] = [];
    const server = fakeApi({
      "GET /workspaces": { status: 401 },
      "POST /auth/refresh": async () => {
        const reply = await refresh.promise;
        order.push("refresh answered");
        return reply;
      },
      "POST /auth/logout": () => {
        order.push("logout sent");
        return { status: 204 };
      },
    });

    const waiting = api.get("/workspaces").catch((error: unknown) => error);
    await vi.waitFor(() => expect(server.count("POST /auth/refresh")).toBe(1));

    const ending = endSession(logout);
    refresh.resolve({ status: 204 });
    await ending;
    await waiting;

    // Logging out first would let the refresh put its cookies back after.
    expect(order).toEqual(["refresh answered", "logout sent"]);
  });

  it("starts no refresh while the logout is under way", async () => {
    const logoutReply = deferred<FakeReply>();
    const server = fakeApi({
      "GET /teams": { status: 401 },
      "POST /auth/refresh": { status: 204 },
      "POST /auth/logout": () => logoutReply.promise,
    });

    const ending = endSession(logout);
    await vi.waitFor(() => expect(server.count("POST /auth/logout")).toBe(1));

    const refused = await api.get("/teams").catch((error: unknown) => error);

    expect(server.count("POST /auth/refresh")).toBe(0);
    expect(refused).toMatchObject({ status: 401 });

    logoutReply.resolve({ status: 204 });
    await ending;
  });

  it("does not replay a request once the session it waited for is ended", async () => {
    const refresh = deferred<FakeReply>();
    const server = fakeApi({
      "GET /challenges": { status: 401 },
      "POST /auth/refresh": () => refresh.promise,
      "POST /auth/logout": { status: 204 },
    });

    const waiting = api.get("/challenges").catch((error: unknown) => error);
    await vi.waitFor(() => expect(server.count("POST /auth/refresh")).toBe(1));

    const ending = endSession(logout);
    refresh.resolve({ status: 204 });
    await ending;

    // The refresh did succeed, but for the session that just ended.
    expect(await waiting).toMatchObject({ status: 401 });
    expect(server.count("GET /challenges")).toBe(1);
  });
});

describe("the refresh lock", () => {
  it("rotates inside the cross-tab lock when the browser has one", async () => {
    const order: string[] = [];
    const request = vi.fn(async (_name: string, callback: () => Promise<unknown>) => {
      order.push("lock taken");
      const result = await callback();
      order.push("lock released");
      return result;
    });
    vi.stubGlobal("navigator", Object.create(navigator, { locks: { value: { request } } }));
    fakeApi({
      "GET /notes": expiredOnce(),
      "POST /auth/refresh": () => {
        order.push("refresh sent");
        return { status: 204 };
      },
    });

    await expect(api.get("/notes")).resolves.toMatchObject({ status: 200 });

    expect(request).toHaveBeenCalledWith("pwnforge-auth-refresh", expect.any(Function));
    expect(order).toEqual(["lock taken", "refresh sent", "lock released"]);
  });

  it("falls back to one rotation per tab without it", async () => {
    expect("locks" in navigator).toBe(false);
    const refresh = deferred<FakeReply>();
    const server = fakeApi({
      "GET /notes": expiredOnce(),
      "GET /teams": expiredOnce(),
      "POST /auth/refresh": () => refresh.promise,
    });

    const both = Promise.all([api.get("/notes"), api.get("/teams")]);
    await vi.waitFor(() => expect(server.count("GET /teams")).toBe(1));
    await vi.waitFor(() => expect(server.count("GET /notes")).toBe(1));
    refresh.resolve({ status: 204 });

    await expect(both).resolves.toHaveLength(2);
    // Two 401s at once share one rotation instead of spending the token twice.
    expect(server.count("POST /auth/refresh")).toBe(1);
  });
});
