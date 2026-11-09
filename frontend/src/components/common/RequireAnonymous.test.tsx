// src/components/common/RequireAnonymous.test.tsx
import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {
  MemoryRouter,
  Route,
  RouterProvider,
  Routes,
  createMemoryRouter,
  useLocation,
  useNavigate,
} from "react-router";
import { describe, expect, it, vi } from "vitest";

import { AuthProvider } from "@/components/common/AuthProvider";
import { RequireAnonymous } from "@/components/common/RequireAnonymous";
import { RequireAuth } from "@/components/common/RequireAuth";
import { useAuth } from "@/hooks/useAuth";
import { AuthContext, type AuthContextValue, type AuthStatus } from "@/lib/auth";
import { ROUTES } from "@/lib/routes";
import { fakeApi } from "@/test/fake-api";
import { testQueryClient } from "@/test/query";
import type { User } from "@/types/api";

/** Where the router lands once the guard has let a signed-in visitor through. */
function Landed() {
  const location = useLocation();
  return <output data-testid="landed">{`${location.pathname}${location.search}`}</output>;
}

function auth(status: AuthStatus): AuthContextValue {
  return {
    user: status === "authenticated" ? ({ id: "u1", username: "alice" } as User) : null,
    status,
    login: vi.fn(),
    register: vi.fn(),
    logout: vi.fn(),
    confirmEmail: vi.fn(),
    resendVerification: vi.fn(),
    forgotPassword: vi.fn(),
    resetPassword: vi.fn(),
    reload: vi.fn(),
  };
}

function visit(url: string, status: AuthStatus) {
  render(
    <AuthContext value={auth(status)}>
      <MemoryRouter initialEntries={[url]}>
        <Routes>
          <Route element={<RequireAnonymous />}>
            <Route path={ROUTES.login} element={<p>sign-in form</p>} />
          </Route>
          <Route path="*" element={<Landed />} />
        </Routes>
      </MemoryRouter>
    </AuthContext>,
  );
}

function landed(): string {
  return screen.getByTestId("landed").textContent ?? "";
}

describe("RequireAnonymous", () => {
  it("shows the page to a signed-out visitor", () => {
    visit(ROUTES.login, "anonymous");

    expect(screen.getByText("sign-in form")).toBeInTheDocument();
  });

  it("sends a signed-in visitor to the app", () => {
    visit(ROUTES.login, "authenticated");

    expect(landed()).toBe(ROUTES.workspaces);
    expect(screen.queryByText("sign-in form")).not.toBeInTheDocument();
  });

  it("sends a signed-in visitor where ?from= says, as a sign-in would", () => {
    const from = "/w/ctf-2026-cd34/challenges?tag=web";
    visit(`${ROUTES.login}?from=${encodeURIComponent(from)}`, "authenticated");

    expect(landed()).toBe(from);
  });

  it("never follows ?from= off-site", () => {
    visit(`${ROUTES.login}?from=${encodeURIComponent("//evil.tld/phish")}`, "authenticated");

    expect(landed()).toBe(ROUTES.workspaces);
  });

  it("decides nothing while the session is still being read", () => {
    visit(ROUTES.login, "loading");

    expect(screen.getByRole("status")).toBeInTheDocument();
    expect(screen.queryByText("sign-in form")).not.toBeInTheDocument();
    expect(screen.queryByTestId("landed")).not.toBeInTheDocument();
  });
});

/** Signs out, then goes to /login the way the sidebar's menu does. */
function SignOut() {
  const { logout } = useAuth();
  const navigate = useNavigate();
  return (
    <button
      type="button"
      onClick={async () => {
        await logout();
        await navigate(ROUTES.login, { replace: true });
      }}
    >
      sign out
    </button>
  );
}

/** Signs in and leaves the rest to the guard, as the login page does. */
function SignIn() {
  const { login } = useAuth();
  return (
    <button
      type="button"
      onClick={() => login({ username_or_email: "alice", password: "pw", turnstile_token: "" })}
    >
      sign in
    </button>
  );
}

// Against the real provider and a data router, as in the app: the session
// changes and the navigation that follows land close together, and either
// guard reading the old answer would bounce the visitor back.
describe("RequireAnonymous with RequireAuth", () => {
  function app(url: string) {
    const router = createMemoryRouter(
      [
        {
          element: <RequireAnonymous />,
          children: [{ path: ROUTES.login, element: <SignIn /> }],
        },
        {
          element: <RequireAuth />,
          children: [{ path: "*", element: <SignOut /> }],
        },
      ],
      { initialEntries: [url] },
    );
    render(
      <QueryClientProvider client={testQueryClient()}>
        <AuthProvider>
          <RouterProvider router={router} />
        </AuthProvider>
      </QueryClientProvider>,
    );
    return router;
  }

  it("lands a signed-out user on the sign-in form, not back in the app", async () => {
    fakeApi({
      "GET /auth/me": { status: 200, data: { id: "u1", username: "alice" } },
      "POST /auth/logout": { status: 204 },
    });
    const router = app(ROUTES.teams);

    await userEvent.click(await screen.findByRole("button", { name: "sign out" }));

    await screen.findByRole("button", { name: "sign in" });
    expect(router.state.location.pathname).toBe(ROUTES.login);
    expect(router.state.location.search).toBe("");
  });

  it("lands a user who just signed in where ?from= says, not back on the form", async () => {
    fakeApi({
      "GET /auth/me": { status: 401 },
      "POST /auth/refresh": { status: 401 },
      "POST /auth/login": { status: 200, data: { id: "u1", username: "alice" } },
    });
    const router = app(`${ROUTES.login}?from=${encodeURIComponent("/teams/red")}`);

    await userEvent.click(await screen.findByRole("button", { name: "sign in" }));

    await screen.findByRole("button", { name: "sign out" });
    expect(router.state.location.pathname).toBe("/teams/red");
  });
});
