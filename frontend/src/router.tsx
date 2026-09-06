// src/router.tsx
import { createBrowserRouter } from "react-router";
import { ROUTES } from "@/lib/routes";

// First-run wizard
import { SetupGate } from "@/components/common/SetupGate";
import { SetupPage } from "@/pages/public/SetupPage";

// Public routes
import { LandingGate } from "@/components/common/LandingGate";
import { LandingShell } from "@/components/layout/LandingShell";
import { DocumentShell } from "@/components/layout/DocumentShell";
import { LandingPage } from "@/pages/public/LandingPage";

// Auth routes
import { AuthShell } from "@/components/layout/AuthShell";
import { LoginPage } from "@/pages/auth/LoginPage";
import { RegisterPage } from "@/pages/auth/RegisterPage";
import { ConfirmEmailPage } from "@/pages/auth/ConfirmEmailPage";
import { ForgotPasswordPage } from "@/pages/auth/ForgotPasswordPage";
import { ResetPasswordPage } from "@/pages/auth/ResetPasswordPage";

// Authenticated app routes
import { RequireAuth } from "@/components/common/RequireAuth";
import { AppShell } from "@/components/layout/AppShell";
import { ActivityItem } from "@/components/activity/ActivityItem";
import { LegalPage } from "@/pages/public/LegalPage";
import { ContactPage } from "./pages/public/ContactPage";


const activityExample = {
  user: { name: "Alice" },
  action: "blocked",
  element: "Challenge 1",
  tag: "+100 pts",
  tagType: "success" as const,
  timestamp: "2026-09-05 19:23",
};

export const router = createBrowserRouter([
  // Everything sits under the setup gate: on a fresh instance every route
  // answers with the wizard, and once it has run /setup stops existing.
  {
    element: <SetupGate />,
    children: [
      { path: ROUTES.setup, element: <SetupPage /> },

      // Public routes (wrap in LandingShell / AuthShell later)
      {
        element: <LandingGate />,
          children: [
            {
              element: <LandingShell />,
              children: [
                { path: ROUTES.landing, element: <LandingPage /> },
              ],
            },
          ],
      },
      {
        element: <DocumentShell />,
        children: [
          { path: ROUTES.legal, element: <LegalPage slug="legal" /> },
          { path: ROUTES.privacy, element: <LegalPage slug="privacy" /> },
          { path: ROUTES.terms, element: <LegalPage slug="terms" /> },
          { path: ROUTES.contact, element: <ContactPage /> },
          // { path: ROUTES.changelog, element: <LegalPage slug="changelog" /> }
        ],
      },
      {
        element: <AuthShell />,
        children: [
          { path: ROUTES.login, element: <LoginPage /> },
          { path: ROUTES.register, element: <RegisterPage /> },
          { path: ROUTES.confirmEmail, element: <ConfirmEmailPage /> },
          { path: ROUTES.forgotPassword, element: <ForgotPasswordPage /> },
          { path: ROUTES.resetPassword, element: <ResetPasswordPage /> },
        ],
      },

      // Authenticated app: RequireAuth answers "is anyone signed in" before
      // AppShell renders, so no protected page ever flashes to a visitor.
      {
        element: <RequireAuth />,
        children: [
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
        ],
      },
    ],
  },
]);
