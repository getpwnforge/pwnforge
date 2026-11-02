const GITHUB_REPO = "https://github.com/getpwnforge/pwnforge";

/** External destinations. These go in <a href>, never in React Router <Link>. */
export const EXTERNAL = {
  docs: import.meta.env.VITE_DOCS_URL ?? "https://docs.pwnforge.app",
  api: import.meta.env.VITE_DOCS_URL
    ? `${import.meta.env.VITE_DOCS_URL}/api`
    : "https://docs.pwnforge.app/api",
  selfhost: "https://docs.pwnforge.app/self-hosting",

  github: GITHUB_REPO,
  contributing: `${GITHUB_REPO}/blob/main/CONTRIBUTING.md`,
  security: `${GITHUB_REPO}/blob/main/SECURITY.md`,
  license: `${GITHUB_REPO}/blob/main/LICENSE`,
  issues: `${GITHUB_REPO}/issues`,

  discord: "https://discord.gg/FJF2vHRy6J",

  status: "https://status.pwnforge.app",
} as const;

export const ROUTES = {
  // Public
  landing: "/",

  legal: "/legal",
  privacy: "/privacy",
  terms: "/terms",
  contact: "/contact",
  changelog: "/changelog",

  login: "/login",
  register: "/register",
  // Matches the link sent by `email_service::send_email_verification` exactly
  // (`{public_url}/auth/email/verify#token=...`): the backend builds that URL
  // itself, so the frontend route has to line up with it, not the other way.
  confirmEmail: "/auth/email/verify",
  forgotPassword: "/forgot-password",
  resetPassword: "/auth/password/reset",

  // First-run wizard. Unreachable once the instance is configured — the setup
  // gate sends it to /login.
  setup: "/setup",

  // The links invitation mails carry, built by the backend's `token_link`
  // (`{public_url}/invitations/{team|workspace}#token=...`): the token rides
  // in the fragment, which never leaves the browser, and `useLinkToken`
  // reads it. No page yet.
  teamInvitation: "/invitations/team",
  workspaceInvitation: "/invitations/workspace",

  // Personal space (sidebar "global" mode)
  workspaces: "/workspaces",
  workspaceCreate: "/workspaces/new",
  teams: "/teams",
  team: (slug: string) => `/teams/${encodeURIComponent(slug)}`,
  teamWorkspaces: (slug: string) => `/teams/${encodeURIComponent(slug)}?tab=workspaces`,
  teamRoles: (slug: string) => `/teams/${encodeURIComponent(slug)}?tab=roles`,
  teamInvitations: (slug: string) => `/teams/${encodeURIComponent(slug)}?tab=invitations`,
  teamSettings: (slug: string) => `/teams/${encodeURIComponent(slug)}?tab=settings`,
  profile: "/me",
  myStats: "/me/stats",
  mySettings: "/me/settings",
  help: "/help",

  // Workspace-scoped (sidebar "workspace" mode), by slug, the key the API
  // names a workspace by. Dashboard is the index route.
  workspace: (slug: string) => `/w/${encodeURIComponent(slug)}`,
  challenges: (slug: string) => `/w/${encodeURIComponent(slug)}/challenges`,
  challenge: (slug: string, challenge: string) => `/w/${encodeURIComponent(slug)}/challenges/${encodeURIComponent(challenge)}`,
  writeups: (slug: string) => `/w/${encodeURIComponent(slug)}/writeups`,
  stats: (slug: string) => `/w/${encodeURIComponent(slug)}/stats`,
  members: (slug: string) => `/w/${encodeURIComponent(slug)}/members`,
  settings: (slug: string) => `/w/${encodeURIComponent(slug)}/settings`,
} as const;

// Pattern for useMatch: reads the current workspace slug from any nested route.
// Kept next to ROUTES.workspace so the two never drift apart.
export const WORKSPACE_ROUTE_MATCH = "/w/:slug/*";
