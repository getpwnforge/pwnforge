import { Outlet } from "react-router";
import { AppSidebar } from "@/components/layout/Sidebar";
import { InstanceAlertBanner } from "@/components/layout/InstanceAlertBanner";
import { LegalRevisionBanner } from "@/components/layout/LegalRevisionBanner";
import { SidebarProvider, SidebarInset, SidebarTrigger } from "@/components/ui/sidebar";

export function AppShell() {
  return (
    <div className="flex flex-col h-dvh w-screen overflow-hidden">
      {/* Independent notices, stacked rather than competing: an imminent
          maintenance window and a terms revision are unrelated, and hiding one
          because the other is up would repeat the bug this replaces. Instance
          alerts come first, being the more urgent of the two. The column is
          already flexible, so each banner keeps its own height and the shell
          absorbs the rest. */}
      <InstanceAlertBanner />
      <LegalRevisionBanner />
      <SidebarProvider className="flex-1 min-h-0">
        <AppSidebar />
        <SidebarInset className="overflow-y-auto">
          <header className="flex h-12 items-center gap-2 border-b border-border px-3 md:hidden">
            <SidebarTrigger />
          </header>
          <Outlet />
        </SidebarInset>
      </SidebarProvider>
    </div>
  );
}
