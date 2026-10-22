// src/lib/safe-redirect.test.ts
import { describe, expect, it } from "vitest";

import { ROUTES } from "@/lib/routes";
import { safeRedirect } from "@/lib/safe-redirect";

describe("safeRedirect", () => {
  // Each one a value a browser would follow off this origin, or into a
  // script, if it were used as given.
  it.each([
    ["a backslash read as a slash", "/\\evil.example"],
    ["a tab the URL parser drops", "/\t/evil.example"],
    ["a newline the URL parser drops", "/\n/evil.example"],
    ["a CRLF the URL parser drops", "/\r\n/evil.example"],
    ["a scheme-relative URL", "//evil.example"],
    ["an absolute URL elsewhere", "https://evil.example"],
    ["a javascript: URL", "javascript:alert(1)"],
    ["a data: URL", "data:text/html,<script>alert(1)</script>"],
  ])("refuses %s", (_name, target) => {
    expect(safeRedirect(target)).toBe(ROUTES.workspaces);
  });

  it("keeps a path on this origin, with its query, and drops the fragment", () => {
    expect(safeRedirect("/teams/ctf-team?tab=members#roster")).toBe("/teams/ctf-team?tab=members");
  });

  it("never brings a link's token back from `from`", () => {
    expect(safeRedirect("/invitations/team#token=abc")).toBe("/invitations/team");
  });

  it("returns the path as the browser resolves it", () => {
    expect(safeRedirect("/teams/./x/../ctf-team")).toBe("/teams/ctf-team");
  });

  // Not hostile, but not a path: only paths are taken.
  it("refuses an absolute URL even on this origin", () => {
    expect(safeRedirect(`${window.location.origin}/teams`)).toBe(ROUTES.workspaces);
  });

  // The origin check alone would let it through as /teams/ctf-team: a
  // backslash is refused outright, wherever it is.
  it("refuses any backslash, even one that resolves on this origin", () => {
    expect(safeRedirect("/teams\\ctf-team")).toBe(ROUTES.workspaces);
  });

  it.each([
    ["an empty value", ""],
    ["no value", null],
  ])("falls back to the default on %s", (_name, target) => {
    expect(safeRedirect(target)).toBe(ROUTES.workspaces);
  });
});
