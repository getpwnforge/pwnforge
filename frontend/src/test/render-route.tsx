import { render } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter, Routes, useLocation } from "react-router";
import { vi } from "vitest";

import type { UserResponse } from "@/lib/api/types";
import { AuthContext, type AuthContextValue } from "@/lib/auth";
import { userResponse } from "@/test/api-fixtures";
import { testQueryClient, withClient } from "@/test/query";

/** Prints where the router ended up, so a test can assert a navigation. */
function LocationProbe() {
  const { pathname, search } = useLocation();
  return <output data-testid="location">{pathname + search}</output>;
}

/** What jsdom lacks and Radix menus, selects and dialogs call. */
function stubBrowserGaps() {
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  const element = Element.prototype as unknown as Record<string, unknown>;
  element.hasPointerCapture ??= () => false;
  element.releasePointerCapture ??= () => {};
  element.scrollIntoView ??= () => {};
}

type Options = { user?: UserResponse | null };

/**
 * Renders `routes` at `url` with a fresh query client and an auth context.
 * `user: null` renders as an anonymous visitor.
 */
export function renderRoute(url: string, routes: ReactNode, { user = userResponse() }: Options = {}) {
  stubBrowserGaps();
  const auth: AuthContextValue = {
    user: user as AuthContextValue["user"],
    status: user ? "authenticated" : "anonymous",
    login: vi.fn(),
    register: vi.fn(),
    logout: vi.fn(),
    confirmEmail: vi.fn(),
    resendVerification: vi.fn(),
    forgotPassword: vi.fn(),
    resetPassword: vi.fn(),
    reload: vi.fn(),
  };
  const client = testQueryClient();
  const view = render(
    <AuthContext value={auth}>
      <MemoryRouter initialEntries={[url]}>
        <Routes>{routes}</Routes>
        <LocationProbe />
      </MemoryRouter>
    </AuthContext>,
    { wrapper: withClient(client) },
  );
  return { ...view, client };
}
