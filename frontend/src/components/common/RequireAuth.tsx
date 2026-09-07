// src/components/common/RequireAuth.tsx
import { Navigate, Outlet, useLocation } from "react-router";

import { PageLoader } from "@/components/common/PageLoader";
import { useAuth } from "@/hooks/useAuth";
import { ROUTES } from "@/lib/routes";

/**
 * Layout route that keeps signed-out visitors out of the application.
 *
 * The three-state `status` earns its keep here: redirecting while the session
 * is still being read would bounce an authenticated user to the sign-in page on
 * every page refresh, since `/auth/me` has not answered yet at first render.
 *
 * Nothing to do for an expiring session either — when the refresh is refused,
 * the axios interceptor drops the cached user, `status` turns `anonymous`, and
 * this component redirects on the next render.
 */
export function RequireAuth() {
  const { status } = useAuth();
  const location = useLocation();

  if (status === "loading") {
    return <PageLoader />;
  }

  if (status === "anonymous") {
    // Search and hash included: a filtered challenge list has to come back
    // filtered once the user has signed in.
    const from = `${location.pathname}${location.search}${location.hash}`;

    return <Navigate to={`${ROUTES.login}?from=${encodeURIComponent(from)}`} replace />;
  }

  return <Outlet />;
}
