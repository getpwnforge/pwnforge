// src/hooks/useLinkToken.test.ts
import { renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { forgetLinkToken, useLinkToken } from "@/hooks/useLinkToken";

describe("useLinkToken", () => {
  it("reads the token from the fragment, then drops the fragment from the address bar", () => {
    window.history.replaceState({ idx: 3 }, "", "/auth/email/verify?lang=fr#token=abc123");
    const replaceState = vi.spyOn(window.history, "replaceState");

    const { result } = renderHook(() => useLinkToken());

    expect(result.current).toBe("abc123");
    // The router's own state rides along, the query string stays.
    expect(replaceState).toHaveBeenCalledWith({ idx: 3 }, "", "/auth/email/verify?lang=fr");
    expect(window.location.hash).toBe("");
    expect(window.location.search).toBe("?lang=fr");
  });

  it("keeps the token after the fragment is gone", () => {
    window.history.replaceState(null, "", "/auth/password/reset#token=abc123");

    const { result, rerender } = renderHook(() => useLinkToken());
    rerender();

    expect(window.location.hash).toBe("");
    expect(result.current).toBe("abc123");
  });

  it("ignores a token in the query string, and leaves the address alone", () => {
    window.history.replaceState(null, "", "/auth/password/reset?token=abc123");
    const replaceState = vi.spyOn(window.history, "replaceState");

    const { result } = renderHook(() => useLinkToken());

    expect(result.current).toBeNull();
    expect(replaceState).not.toHaveBeenCalled();
  });

  it("returns null without a token", () => {
    window.history.replaceState(null, "", "/auth/email/verify");

    const { result } = renderHook(() => useLinkToken());

    expect(result.current).toBeNull();
  });
});

describe("useLinkToken across a sign-in", () => {
  const open = (url: string) => window.history.replaceState(null, "", url);

  afterEach(() => {
    window.sessionStorage.clear();
    open("/");
  });

  it("keeps an invitation's token for the way back, out of the address bar", () => {
    open("/invitations/team#token=abc");
    expect(renderHook(() => useLinkToken("invitation:team")).result.current).toBe("abc");
    expect(window.location.hash).toBe("");

    // Back from /login, with no fragment.
    open("/invitations/team");
    expect(renderHook(() => useLinkToken("invitation:team")).result.current).toBe("abc");
  });

  it("has nothing once the token is forgotten", () => {
    open("/invitations/team#token=abc");
    renderHook(() => useLinkToken("invitation:team"));
    forgetLinkToken("invitation:team");

    open("/invitations/team");
    expect(renderHook(() => useLinkToken("invitation:team")).result.current).toBeNull();
  });

  it("keeps nothing for a link that asks for no stash", () => {
    open("/auth/password/reset#token=secret");
    renderHook(() => useLinkToken());

    expect(window.sessionStorage).toHaveLength(0);
  });
});
