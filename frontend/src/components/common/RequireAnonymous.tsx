// src/components/common/RequireAnonymous.tsx
import { Navigate, Outlet, useSearchParams } from "react-router";

import { PageLoader } from "@/components/common/PageLoader";
import { useAuth } from "@/hooks/useAuth";
import { safeRedirect } from "@/lib/safe-redirect";

/**
 * Layout route for the pages only a signed-out visitor has a use for: signing
 * in and creating an account. A browser that still holds a session skips them
 * and goes on to `?from=`, or to the app. That is also how the login page
 * leaves once the sign-in succeeds: the status turns `authenticated`, and this
 * guard is the one place that decides where the user lands.
 *
 * Waits for the session the way `RequireAuth` does: deciding on `loading`
 * would show the form to a signed-in user for as long as `/auth/me` takes to
 * answer, then pull it from under them.
 *
 * Not for the pages a mailed link opens (email verification, password reset):
 * those have to work in a browser that is signed in, and their token rides in
 * the fragment, which a redirect would drop.
 */
export function RequireAnonymous() {
  const { status } = useAuth();
  const [searchParams] = useSearchParams();

  if (status === "loading") {
    return <PageLoader />;
  }

  if (status === "authenticated") {
    // Through safeRedirect, as after a sign-in: `?from=` is whatever the link
    // that brought the visitor here said, and an open redirect needs nothing
    // more than a signed-in victim following it.
    return <Navigate to={safeRedirect(searchParams.get("from"))} replace />;
  }

  return <Outlet />;
}
