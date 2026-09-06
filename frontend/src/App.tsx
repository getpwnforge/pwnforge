import { RouterProvider } from "react-router";
import { QueryClientProvider } from "@tanstack/react-query";
import { queryClient } from "@/lib/query-client";
import { AuthProvider } from "@/components/common/AuthProvider";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { router } from "@/router";

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      {/* Above the router: the session is read once, not per route. */}
      <AuthProvider>
        {/* Above the router too: any component in any route can trigger a
            tooltip (sidebar nav, future buttons/badges elsewhere), so the
            provider needs to sit above all of them at once rather than being
            duplicated per route. */}
        <TooltipProvider>
          <RouterProvider router={router} />
        </TooltipProvider>
        {/* Outside the router on purpose: a toast has to outlive the navigation
            that usually follows the event it reports. The session-expired one
            fires exactly as RequireAuth redirects to /login — mounted inside a
            route element, it would unmount before anyone could read it. */}
        <Toaster />
      </AuthProvider>
    </QueryClientProvider>
  );
}
