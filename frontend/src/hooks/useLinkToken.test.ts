// src/hooks/useLinkToken.test.ts
import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { useLinkToken } from "@/hooks/useLinkToken";

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
