// src/lib/invitation-link.ts
import type { InvitationKind } from "@/api/invitations";
import { ROUTES } from "@/lib/routes";

/**
 * The link an invitation token opens, in the format the backend's mails use
 * (`token_link`: `{public_url}/invitations/{team|workspace}#token=...`), for
 * a generic team link to copy. The token rides in the fragment, which never
 * leaves the browser, so no server or proxy on the way logs it.
 */
export function invitationLink(
  kind: InvitationKind,
  token: string,
  origin: string = window.location.origin,
): string {
  const path = kind === "team" ? ROUTES.teamInvitation : ROUTES.workspaceInvitation;
  return `${origin}${path}#token=${encodeURIComponent(token)}`;
}
