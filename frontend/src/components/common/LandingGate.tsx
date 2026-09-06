import { Outlet, Navigate } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { ROUTES } from "@/lib/routes";
import { getPublicInstanceConfig, INSTANCE_CONFIG_QUERY_KEY } from "@/api/instance";

// Gates the marketing landing page behind hide_landing_page. Blocks
// rendering until the config resolves, same principle as RequireAuth: no
// flash of content that shouldn't be seen, even briefly.
export function LandingGate() {
  const { data, isPending } = useQuery({
    queryKey: INSTANCE_CONFIG_QUERY_KEY,
    queryFn: getPublicInstanceConfig,
    staleTime: Infinity,
    retry: false,
  });

  if (isPending) {
    return null;
  }

  if (data?.hide_landing_page) {
    return <Navigate to={ROUTES.login} replace />;
  }

  return <Outlet />;
}
