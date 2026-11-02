import { CheckIcon, MinusIcon } from "lucide-react";
import { Fragment } from "react";
import { useTranslation } from "react-i18next";
import { RoleBadge } from "@/components/members/RoleBadge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { useTeamMembers } from "@/hooks/useTeams";
import type { TeamResponse } from "@/lib/api/types";
import { countByRole, PERMISSION_GROUPS, TEAM_ROLES } from "@/lib/team-roles";

/** What each role may do. Read-only: the four roles are fixed, there are no custom roles. */
export function TeamRolesTab({ team }: Readonly<{ team: TeamResponse }>) {
  const { t } = useTranslation(["teams", "common"]);
  const members = useTeamMembers(team.slug);
  const counts = countByRole(members.data ?? []);

  return (
    <div className="flex flex-col gap-4">
      <ul className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {TEAM_ROLES.map((role) => (
          <li key={role} className="rounded-lg border border-border bg-bg-elev p-3.5">
            <div className="mb-2 flex items-center justify-between">
              <RoleBadge role={role} />
              <span className="text-xs tabular-nums text-text-muted">{t("teams:card.members", { count: counts[role] })}</span>
            </div>
            <p className="text-xs leading-relaxed text-text-muted">{t(`teams:roles.long.${role}`)}</p>
          </li>
        ))}
      </ul>

      <section aria-labelledby="matrix-title" className="rounded-lg border border-border bg-bg-elev">
        <h2 id="matrix-title" className="border-b border-border px-4 py-3 text-sm font-semibold text-text">
          {t("teams:roles.matrix")}
        </h2>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-2/5">{t("teams:roles.permission")}</TableHead>
              {TEAM_ROLES.map((role) => (
                <TableHead key={role} className="text-center">
                  {t(`common:roles.${role}`)}
                </TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            {PERMISSION_GROUPS.map((group) => (
              <Fragment key={group.key}>
                <TableRow className="hover:bg-transparent">
                  <TableCell colSpan={5} className="bg-surface py-1.5 text-nav font-semibold uppercase tracking-wider text-text-muted">
                    {t(`teams:roles.groups.${group.key}`)}
                  </TableCell>
                </TableRow>
                {group.rows.map((row) => (
                  <TableRow key={row.key}>
                    <TableCell className="text-sm">{t(`teams:roles.rows.${row.key}`)}</TableCell>
                    {TEAM_ROLES.map((role) => (
                      <TableCell key={role} className="text-center">
                        {row.allowed(role, team) ? (
                          <CheckIcon role="img" className="inline size-4 text-primary" aria-label={t("teams:roles.allowed")} />
                        ) : (
                          <MinusIcon role="img" className="inline size-4 text-text-subtle" aria-label={t("teams:roles.denied")} />
                        )}
                      </TableCell>
                    ))}
                  </TableRow>
                ))}
              </Fragment>
            ))}
          </TableBody>
        </Table>
        <div className="flex flex-wrap items-center gap-4 border-t border-border px-4 py-2.5 text-xs text-text-muted">
          <span className="inline-flex items-center gap-1.5">
            <CheckIcon aria-hidden className="size-4 text-primary" />
            {t("teams:roles.allowed")}
          </span>
          <span className="inline-flex items-center gap-1.5">
            <MinusIcon aria-hidden className="size-4 text-text-subtle" />
            {t("teams:roles.denied")}
          </span>
          <span className="flex-1" />
          <span>{t("teams:roles.footnote")}</span>
        </div>
      </section>
    </div>
  );
}
