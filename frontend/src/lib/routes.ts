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
  // (`{public_url}/auth/email/verify?token=...`) — the backend builds that URL
  // itself, so the frontend route has to line up with it, not the other way.
  confirmEmail: "/auth/email/verify",
  forgotPassword: "/forgot-password",
  resetPassword: "/auth/password/reset",

  // First-run wizard. Unreachable once the instance is configured — the setup
  // gate sends it to /login.
  setup: "/setup",


  // Personal space (sidebar "global" mode)
  workspaces: "/workspaces",
  workspaceCreate: "/workspaces/new",
  teams: "/teams",
  team: (slug: string) => `/teams/${slug}`,
  profile: "/me",
  myStats: "/me/stats",
  mySettings: "/me/settings",
  help: "/help",

  // Workspace-scoped (sidebar "workspace" mode). Dashboard is the index route.
  workspace: (id: string) => `/w/${id}`,
  challenges: (id: string) => `/w/${id}/challenges`,
  challenge: (id: string, challenge: string) => `/w/${id}/challenges/${challenge}`,
  writeups: (id: string) => `/w/${id}/writeups`,
  stats: (id: string) => `/w/${id}/stats`,
  members: (id: string) => `/w/${id}/members`,
  settings: (id: string) => `/w/${id}/settings`,
} as const;

// Pattern for useMatch: reads the current workspace id from any nested route.
// Kept next to ROUTES.workspace so the two never drift apart.
export const WORKSPACE_ROUTE_MATCH = "/w/:workspaceId/*";
