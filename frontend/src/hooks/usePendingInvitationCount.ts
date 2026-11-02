import { usePendingTeamInvitations, usePendingWorkspaceInvitations } from "@/hooks/useInvitations";

/** Invitations waiting for the current account, both kinds together. */
export function usePendingInvitationCount(): number {
  const teams = usePendingTeamInvitations();
  const workspaces = usePendingWorkspaceInvitations();
  return (teams.data?.length ?? 0) + (workspaces.data?.length ?? 0);
}
