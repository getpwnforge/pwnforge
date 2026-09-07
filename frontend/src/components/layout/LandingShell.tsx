import { useEffect } from "react";
import { Outlet, useLocation } from "react-router";

import { LandingHeader } from "@/components/layout/LandingHeader";
import { SiteFooter } from "@/components/layout/SiteFooter";

function useHashScroll() {
  const { hash } = useLocation();

  useEffect(() => {
    if (!hash) return;
    // Wait for the target section to exist in the DOM
    const el = document.getElementById(hash.slice(1));
    if (el) el.scrollIntoView({ behavior: "smooth" });
  }, [hash]);
}

export function LandingShell() {
  useHashScroll();

  return (
    <div className="flex min-h-dvh flex-col">
      {/* Landing Header */}
      <LandingHeader />

      {/* Landing content */}
      <main className="flex flex-col items-center justify-center gap-18 px-6 py-10">
        <Outlet />
      </main>

      {/* Landing Footer */}
      <SiteFooter />
    </div>
  );
}
