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
import { RequireAnonymous } from "@/components/common/RequireAnonymous";
import { LoginPage } from "@/pages/auth/LoginPage";
import { RegisterPage } from "@/pages/auth/RegisterPage";
import { ConfirmEmailPage } from "@/pages/auth/ConfirmEmailPage";
import { ForgotPasswordPage } from "@/pages/auth/ForgotPasswordPage";
import { ResetPasswordPage } from "@/pages/auth/ResetPasswordPage";

// Authenticated app routes
import { RequireAuth } from "@/components/common/RequireAuth";
import { AppShell } from "@/components/layout/AppShell";
import { LegalPage } from "@/pages/public/LegalPage";
import { ContactPage } from "./pages/public/ContactPage";
import { NotFoundPage } from "./pages/app/NotFoundPage";
import { WorkspacesPage } from "./pages/app/workspaces/WorkspacesPage";
import { TeamsPage } from "./pages/app/teams/TeamsPage";
import { TeamPage } from "./pages/app/teams/TeamPage";
import { InvitationLinkPage } from "./pages/invitations/InvitationLinkPage";

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
      // Signed-out only: a browser that still holds a session goes straight to
      // the app. Above AuthShell rather than inside it, so the auth layout
      // never shows on the way.
      {
        element: <RequireAnonymous />,
        children: [
          {
            element: <AuthShell />,
            children: [
              { path: ROUTES.login, element: <LoginPage /> },
              { path: ROUTES.register, element: <RegisterPage /> },
            ],
          },
        ],
      },
      // Reachable signed in or not. The two mailed links have to work in a
      // browser that holds a session, and their token is in the fragment, which
      // a redirect would drop. Forgot-password stays open to the user who has
      // a session but no longer knows the password behind it.
      {
        element: <AuthShell />,
        children: [
          { path: ROUTES.confirmEmail, element: <ConfirmEmailPage /> },
          { path: ROUTES.forgotPassword, element: <ForgotPasswordPage /> },
          { path: ROUTES.resetPassword, element: <ResetPasswordPage /> },
          { path: ROUTES.teamInvitation, element: <InvitationLinkPage kind="team" /> },
          { path: ROUTES.workspaceInvitation, element: <InvitationLinkPage kind="workspace" /> },
        ],
      },

      // The links invitation mails carry. Outside RequireAuth on purpose: it
      // keeps only the path and the query string in `?from=`, never the
      // fragment, so a signed-out visitor sent to /login would arrive back
      // without the token. The page reads it with useLinkToken first, then
      // asks for a session itself.
      // { path: ROUTES.teamInvitation, element: <Stub name="Team invitation" /> },
      // { path: ROUTES.workspaceInvitation, element: <Stub name="Workspace invitation" /> },

      // Authenticated app: RequireAuth answers "is anyone signed in" before
      // AppShell renders, so no protected page ever flashes to a visitor.
      {
        element: <RequireAuth />,
        children: [
          {
            element: <AppShell />,
            children: [
              // Personal space (sidebar in "global" mode)
              { path: ROUTES.workspaces, element: <WorkspacesPage /> },
              { path: ROUTES.teams, element: <TeamsPage /> },
              { path: "teams/:slug", element: <TeamPage /> },
              // { path: ROUTES.profile, element: <Stub name="My profile" /> },
              // { path: ROUTES.help, element: <Stub name="Help" /> },
              // { path: ROUTES.myStats, element: <Stub name="My stats" /> },
              // { path: ROUTES.mySettings, element: <Stub name="My settings" /> },

              // Inside a workspace (sidebar in "workspace" mode, driven by :slug)
              // { path: ROUTES.workspace(":slug"), element: <Stub name="Dashboard" /> },
              // { path: ROUTES.challenges(":slug"), element: <Stub name="Challenges" /> },
              // { path: ROUTES.challenge(":slug", ":challengeId"), element: <Stub name="Challenge detail" /> },
              // { path: ROUTES.writeups(":slug"), element: <Stub name="Writeups" /> },
              // { path: ROUTES.stats(":slug"), element: <Stub name="Workspace stats" /> },
              // { path: ROUTES.members(":slug"), element: <Stub name="Members" /> },
              // { path: ROUTES.settings(":slug"), element: <Stub name="Settings" /> },
              { path: "*", element: <NotFoundPage /> }
            ],
          },
        ],
      },
    ],
  },
]);
