import { useTranslation } from "react-i18next";

import { CreateTeamLinkDialog } from "@/components/invitations/CreateTeamLinkDialog";
import { InviteToTeamForm } from "@/components/invitations/InviteToTeamForm";
import { TeamInvitationsTable } from "@/components/invitations/TeamInvitationsTable";
import { Badge } from "@/components/ui/badge";
import type { TeamResponse } from "@/lib/api/types";

const CARD = "rounded-lg border border-border bg-bg-elev p-4";

/** Captain and Admins only: TeamPage does not mount this tab for anyone else. */
export function TeamInvitationsTab({ team }: Readonly<{ team: TeamResponse }>) {
  const { t } = useTranslation(["teams"]);

  return (
    <div className="flex flex-col gap-4">
      <div className="grid items-stretch gap-3.5 lg:grid-cols-[1.35fr_1fr]">
        <section aria-labelledby="invite-person-title" className={CARD}>
          <h2 id="invite-person-title" className="text-sm font-semibold text-text">
            {t("teams:invitations.invite.title")}
          </h2>
          <p className="mt-1 mb-3.5 text-xs text-text-muted">{t("teams:invitations.invite.text")}</p>
          <InviteToTeamForm team={team} />
        </section>
        <LinkCard team={team} />
      </div>
      <TeamInvitationsTable team={team} />
    </div>
  );
}

/** What a link is, and the button that makes one. Links in use are rows of the table below. */
function LinkCard({ team }: Readonly<{ team: TeamResponse }>) {
  const { t } = useTranslation(["teams"]);
  return (
    <section aria-labelledby="invite-link-title" className={`${CARD} flex flex-col`}>
      <h2 id="invite-link-title" className="text-sm font-semibold text-text">
        {t("teams:invitations.link.title")}
      </h2>
      <p className="mt-1 mb-3.5 text-xs text-text-muted">{t("teams:invitations.link.text")}</p>
      <div className="mb-3.5 flex flex-wrap gap-1.5">
        <Badge variant="default">{t("teams:invitations.link.factExpiry")}</Badge>
        <Badge variant="default">{t("teams:invitations.link.factUses")}</Badge>
      </div>
      <div className="mt-auto flex flex-wrap items-center gap-2.5">
        <CreateTeamLinkDialog team={team} />
        <span className="text-xs text-text-muted">{t("teams:invitations.link.shownOnce")}</span>
      </div>
    </section>
  );
}
