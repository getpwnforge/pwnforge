import { Outlet, Navigate } from "react-router";
import { ROUTES } from "@/lib/routes";
import { useInstanceConfig } from "@/hooks/useInstanceConfig";

// Gates the marketing landing page behind hide_landing_page. Blocks
// rendering until the config resolves, same principle as RequireAuth: no
// flash of content that shouldn't be seen, even briefly.
export function LandingGate() {
  const { data, isPending } = useInstanceConfig();

  if (isPending) {
    return null;
  }

  if (data?.hide_landing_page) {
    return <Navigate to={ROUTES.login} replace />;
  }

  return <Outlet />;
}
