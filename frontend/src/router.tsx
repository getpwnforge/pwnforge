// src/router.tsx
import { createBrowserRouter } from "react-router";
import { ROUTES } from "@/lib/routes";

// Public routes
import { LandingShell } from "@/components/layout/LandingShell";
import { LandingPage } from "@/pages/public/LandingPage";

// Auth routes
import { AuthShell } from "@/components/layout/AuthShell";
import { LoginPage } from "@/pages/auth/LoginPage";
import { RegisterPage } from "@/pages/auth/RegisterPage";
// import { ForgotPasswordPage } from "@/pages/auth/ForgotPasswordPage";
// import { ResetPasswordPage } from "@/pages/auth/ResetPasswordPage";

// Authenticated app routes
import { AppShell } from "@/components/layout/AppShell";
import { ActivityItem } from "@/components/activity/ActivityItem";


// Dev (temporary) pages for testing and prototyping. Remove these once the real pages exist.
import { ComponentsPage } from "@/pages/dev/ComponentsPage";
import { MarkdownPreview } from "./components/writeups/MarkdownPreview";
import { MARKDOWN_SAMPLE } from "./pages/dev/markdown-sample";


// Temporary stand-in until the real pages exist (Phase 2+).
// const Stub = ({ name }: { name: string }) => (
//   <div style={{ padding: 32 }}>{name}</div>
// );

const activityExample = {
  user: { name: "Alice" },
  action: "blocked",
  element: "Challenge 1",
  tag: "+100 pts",
  tagType: "success" as const,
  timestamp: "2025-08-07 22:34",
};

export const router = createBrowserRouter([

  { path: "/dev/components", element: <ComponentsPage /> },
  { path: "/dev/md", element: <MarkdownPreview content={MARKDOWN_SAMPLE}  /> },

  // Public routes (wrap in LandingShell / AuthShell later)
  {
    element: <LandingShell />,
    children: [
      { path: ROUTES.landing, element: <LandingPage /> },
      // { path: ROUTES.privacy, element: <Stub name="Privacy policy" /> },
      // { path: ROUTES.terms, element: <Stub name="Terms of service" /> },
      // { path: ROUTES.contact, element: <Stub name="Contact" /> },
      // { path: ROUTES.changelog, element: <Stub name="Changelog" /> }
    ],
  },
  {
    element: <AuthShell />,
    children: [
      { path: ROUTES.login, element: <LoginPage /> },
      { path: ROUTES.register, element: <RegisterPage /> },
      // { path: ROUTES.forgotPassword, element: <ForgotPasswordPage /> },
      // { path: ROUTES.resetPassword(":token"), element: <ResetPasswordPage /> },
    ],
  },

  // Authenticated app: AppShell is the layout, pages render in its <Outlet />
  {
    element: <AppShell />,
    children: [
      // Personal space (sidebar in "global" mode)
      { path: ROUTES.workspaces, element: <ActivityItem activity={activityExample} /> },
      // { path: ROUTES.workspaceCreate, element: <Stub name="Create workspace" /> },
      // { path: ROUTES.teams, element: <Stub name="Teams" /> },
      // { path: ROUTES.team(":teamId"), element: <Stub name="Team detail" /> },
      // { path: ROUTES.profile, element: <Stub name="My profile" /> },
      // { path: ROUTES.help, element: <Stub name="Help" /> },
      // { path: ROUTES.myStats, element: <Stub name="My stats" /> },
      // { path: ROUTES.mySettings, element: <Stub name="My settings" /> },

      // Inside a workspace (sidebar in "workspace" mode, driven by :workspaceSlug)
      // { path: ROUTES.workspace(":workspaceId"), element: <Stub name="Dashboard" /> },
      // { path: ROUTES.challenges(":workspaceId"), element: <Stub name="Challenges" /> },
      // { path: ROUTES.challenge(":workspaceId", ":challengeId"), element: <Stub name="Challenge detail" /> },
      // { path: ROUTES.writeups(":workspaceId"), element: <Stub name="Writeups" /> },
      // { path: ROUTES.stats(":workspaceId"), element: <Stub name="Workspace stats" /> },
      // { path: ROUTES.members(":workspaceId"), element: <Stub name="Members" /> },
      // { path: ROUTES.settings(":workspaceId"), element: <Stub name="Settings" /> },
    ],
  },
]);
