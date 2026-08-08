// src/layout/AppShell.tsx
import { Outlet } from "react-router";
import { Sidebar } from "@/components/layout/Sidebar";

export function AppShell() {
  return (
    <div className="flex h-dvh w-screen overflow-hidden">
      <Sidebar />
      <main className="overflow-y-auto w-full">
        <div>
          <Outlet />
        </div>
      </main>
    </div>
  );
}
