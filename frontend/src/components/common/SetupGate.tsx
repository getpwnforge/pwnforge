// src/components/common/SetupGate.tsx
import { Navigate, Outlet, useLocation } from "react-router";
import { useQuery } from "@tanstack/react-query";

import { PageLoader } from "@/components/common/PageLoader";
import { SETUP_STATUS_QUERY_KEY, fetchSetupStatus } from "@/api/setup";
import { ROUTES } from "@/lib/routes";

/**
 * Layout route that keeps a fresh instance on the wizard.
 *
 * Asked once at boot and cached for the session: the answer only changes when
 * the wizard itself completes, and it updates the cache directly when it does.
 */
export function SetupGate() {
  const location = useLocation();

  const { data, isPending, isError } = useQuery({
    queryKey: SETUP_STATUS_QUERY_KEY,
    queryFn: fetchSetupStatus,
    retry: 1,
    staleTime: Infinity,
    refetchOnWindowFocus: false,
  });

  if (isPending) {
    return <PageLoader />;
  }

  // Backend unreachable: let the app through. Trapping every route behind a
  // wizard we cannot confirm is needed would lock out a working instance over
  // a transient network error.
  const completed = isError ? true : data.completed;

  const onSetupRoute = location.pathname === ROUTES.setup;

  if (!completed && !onSetupRoute) {
    return <Navigate to={ROUTES.setup} replace />;
  }

  // Nothing left to configure — the wizard's own routes answer 404 from here on.
  if (completed && onSetupRoute) {
    return <Navigate to={ROUTES.login} replace />;
  }

  return <Outlet />;
}
