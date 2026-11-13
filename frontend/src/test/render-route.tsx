import { render } from "@testing-library/react";
import type { ReactNode } from "react";
import { createMemoryRouter, createRoutesFromElements, Outlet, Route, RouterProvider, useLocation } from "react-router";
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

/** Every test route renders under this, so the probe sits inside the router. */
function ProbeLayout() {
  return (
    <>
      <Outlet />
      <LocationProbe />
    </>
  );
}

/** jsdom lays nothing out: there is never a size to report. */
class ResizeObserverStub {
  observe() {
    // Nothing to watch without layout.
  }
  unobserve() {
    // Nothing was watched.
  }
  disconnect() {
    // Nothing to let go of.
  }
}

/** What jsdom lacks and Radix menus, selects and dialogs call. */
function stubBrowserGaps() {
  vi.stubGlobal("ResizeObserver", ResizeObserverStub);
  const element = Element.prototype as unknown as Record<string, unknown>;
  element.hasPointerCapture ??= () => false;
  element.releasePointerCapture ??= () => {};
  element.scrollIntoView ??= () => {};
}

type Options = { user?: UserResponse | null };

/**
 * Renders `routes` at `url` with a fresh query client and an auth context.
 * `user: null` renders as an anonymous visitor. A data router, as in the app, so that
 * useBlocker and useMatches work.
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
  const router = createMemoryRouter(createRoutesFromElements(<Route element={<ProbeLayout />}>{routes}</Route>), {
    initialEntries: [url],
  });
  const view = render(
    <AuthContext value={auth}>
      <RouterProvider router={router} />
    </AuthContext>,
    { wrapper: withClient(client) },
  );
  return { ...view, client, router };
}
