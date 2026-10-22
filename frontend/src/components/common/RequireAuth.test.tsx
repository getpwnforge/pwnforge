// src/components/common/RequireAuth.test.tsx
import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { describe, expect, it, vi } from "vitest";

import { RequireAuth } from "@/components/common/RequireAuth";
import { AuthContext, type AuthContextValue } from "@/lib/auth";
import { ROUTES } from "@/lib/routes";
import { safeRedirect } from "@/lib/safe-redirect";

/** The whole URL the router lands on: path, query string and fragment. */
function Landed() {
  const location = useLocation();
  return <output data-testid="landed">{`${location.pathname}${location.search}${location.hash}`}</output>;
}

const signedOut: AuthContextValue = {
  user: null,
  status: "anonymous",
  login: vi.fn(),
  register: vi.fn(),
  logout: vi.fn(),
  confirmEmail: vi.fn(),
  resendVerification: vi.fn(),
  forgotPassword: vi.fn(),
  resetPassword: vi.fn(),
  reload: vi.fn(),
};

function visit(url: string): string {
  render(
    <AuthContext value={signedOut}>
      <MemoryRouter initialEntries={[url]}>
        <Routes>
          <Route element={<RequireAuth />}>
            <Route path="*" element={<p>protected</p>} />
          </Route>
          <Route path={ROUTES.login} element={<Landed />} />
        </Routes>
      </MemoryRouter>
    </AuthContext>,
  );
  return screen.getByTestId("landed").textContent ?? "";
}

describe("RequireAuth", () => {
  it("sends a signed-out visitor to /login with the path and the query string", () => {
    const landed = visit("/w/ctf-2026-cd34/challenges?tag=web");

    expect(landed).toBe(`/login?from=${encodeURIComponent("/w/ctf-2026-cd34/challenges?tag=web")}`);
  });

  it("never carries a link's fragment into the login URL", () => {
    const landed = visit("/w/ctf-2026-cd34?tab=members#token=abc");

    expect(landed).toBe(`/login?from=${encodeURIComponent("/w/ctf-2026-cd34?tab=members")}`);
    expect(landed).not.toContain("token");
    expect(decodeURIComponent(landed)).not.toContain("token");
  });

  it("leaves nothing of the token for the way back after signing in", () => {
    const landed = visit("/invitations/team#token=abc");
    const from = new URLSearchParams(landed.split("?")[1]).get("from");

    expect(safeRedirect(from)).toBe("/invitations/team");
  });
});
