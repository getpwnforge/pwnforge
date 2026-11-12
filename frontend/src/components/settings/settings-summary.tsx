import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";

import { WsIcon } from "@/components/brand/WsIcon";
import { SettingRow, SettingsSection } from "@/components/settings/settings-layout";
import { Button } from "@/components/ui/button";
import { useTeams } from "@/hooks/useTeams";
import type { WorkspaceResponse } from "@/lib/api/types";
import { avatarUrl } from "@/lib/avatar";
import { ROUTES } from "@/lib/routes";

/*
 * The settings as values, for whoever may not change them: nothing here looks like a control,
 * so nothing invites a click that would do nothing. The forms render these instead of their
 * fields when the viewer cannot edit, or while the workspace is archived.
 */

/** Why nothing can be changed. Archived says so in its own banner, above the page. */
export function ReadOnlyNote({ workspace }: Readonly<{ workspace: WorkspaceResponse }>) {
  const { t } = useTranslation(["workspaces"]);
  if (workspace.archived) return null;
  return <p className="text-sm text-danger-text">{t("workspaces:settings.readOnly")}</p>;
}

/** A value, or a dash when there is none: an empty cell reads as a missing render. */
function Value({ children, muted }: Readonly<{ children?: ReactNode; muted?: boolean }>) {
  if (children === null || children === undefined || children === "") {
    return <span className="text-sm text-text-muted">—</span>;
  }
  return <span className={muted ? "text-sm text-text-muted" : "text-sm text-text"}>{children}</span>;
}

/** The team the workspace belongs to, from my teams; a guest off the team reads "Team". */
export function TeamName({ teamId }: Readonly<{ teamId: string }>) {
  const { t } = useTranslation(["workspaces"]);
  const teams = useTeams();
  const team = teams.data?.find((candidate) => candidate.id === teamId);
  if (!team) return <span className="text-sm text-text-muted">{t("workspaces:dashboard.team")}</span>;
  return (
    <Link to={ROUTES.team(team.slug)} className="flex items-center gap-2 text-sm font-medium text-text hover:underline">
      <WsIcon name={team.name} color={team.color} size="sm" src={avatarUrl("teams", team.id, team.avatar_version, 64)} />
      {team.name}
    </Link>
  );
}

/**
 * Roles come from the team: point there. Only for a team workspace, and only to whoever is on
 * that team (from my teams); a guest would land on a team they cannot open.
 */
export function TeamRoles({ workspace }: Readonly<{ workspace: WorkspaceResponse }>) {
  const { t } = useTranslation(["workspaces"]);
  const teams = useTeams();
  const team = workspace.team_id ? teams.data?.find((candidate) => candidate.id === workspace.team_id) : undefined;
  if (!team) return null;

  return (
    <SettingsSection id="ws-access" title={t("workspaces:settings.roles.title")}>
      <SettingRow label={t("workspaces:settings.roles.label")} hint={t("workspaces:settings.roles.hint", { team: team.name })}>
        <Button asChild variant="ghost" size="sm">
          <Link to={{ pathname: ROUTES.team(team.slug), search: "?tab=roles" }}>{t("workspaces:settings.roles.manage")}</Link>
        </Button>
      </SettingRow>
    </SettingsSection>
  );
}

/** General, read: identity, mode and team, type and source, and the event. */
export function WorkspaceGeneralSummary({ workspace }: Readonly<{ workspace: WorkspaceResponse }>) {
  const { t, i18n } = useTranslation(["workspaces", "nav"]);
  const when = new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium", timeStyle: "short" });
  const date = (instant: string | null | undefined) => (instant ? when.format(new Date(instant)) : null);

  return (
    <div className="flex flex-col gap-7">
      <ReadOnlyNote workspace={workspace} />

      <SettingsSection id="ws-identity" title={t("workspaces:settings.identity.title")}>
        <div className="flex items-center gap-3 border-b border-border py-2.5">
          <WsIcon
            name={workspace.name}
            color={workspace.color}
            size="lg"
            src={avatarUrl("workspaces", workspace.id, workspace.avatar_version, 256)}
          />
          <div className="min-w-0">
            <div className="truncate text-sm font-medium text-text">{workspace.name}</div>
            <div className="truncate font-mono text-xs text-text-muted">{workspace.slug}</div>
          </div>
        </div>
        <SettingRow label={t("workspaces:settings.identity.description")}>
          <Value>{workspace.description}</Value>
        </SettingRow>
      </SettingsSection>

      <SettingsSection id="ws-mode" title={t("workspaces:settings.modeTeam.title")}>
        <SettingRow label={t("workspaces:settings.modeTeam.workspaceType")}>
          <Value>{t(`nav:workspaceModes.${workspace.mode}`)}</Value>
        </SettingRow>
        {workspace.team_id ? (
          <SettingRow label={t("workspaces:settings.modeTeam.associatedTeam")}>
            <TeamName teamId={workspace.team_id} />
          </SettingRow>
        ) : null}
      </SettingsSection>

      <SettingsSection id="ws-source" title={t("workspaces:settings.source.title")}>
        <SettingRow label={t("workspaces:settings.source.type")}>
          <Value>{t(`workspaces:create.type.${workspace.type}`)}</Value>
        </SettingRow>
        <SettingRow label={t("workspaces:settings.source.platform")}>
          <Value>{workspace.platform ? t(`nav:platforms.${workspace.platform}`) : null}</Value>
        </SettingRow>
        <SettingRow label={t("workspaces:settings.source.eventUrl")}>
          {workspace.event_url ? (
            <a href={workspace.event_url} target="_blank" rel="noopener noreferrer" className="max-w-72 truncate text-sm text-text hover:underline">
              {workspace.event_url}
            </a>
          ) : (
            <Value />
          )}
        </SettingRow>
      </SettingsSection>

      {workspace.type === "event" ? (
        <SettingsSection id="ws-event" title={t("workspaces:settings.event.title")}>
          <SettingRow label={t("workspaces:settings.event.start")}>
            <Value>{date(workspace.starts_at)}</Value>
          </SettingRow>
          <SettingRow label={t("workspaces:settings.event.end")}>
            <Value>{date(workspace.ends_at)}</Value>
          </SettingRow>
          <SettingRow label={t("workspaces:settings.event.lockAfterEnd")} hint={t("workspaces:settings.event.lockAfterEndHint")}>
            <Value>{t(workspace.lock_after_end ? "workspaces:settings.enabled" : "workspaces:settings.disabled")}</Value>
          </SettingRow>
          <SettingRow label={t("workspaces:settings.event.autoArchive")}>
            <Value muted>{t("workspaces:settings.event.autoArchiveNever")}</Value>
          </SettingRow>
        </SettingsSection>
      ) : null}
    </div>
  );
}

/** Visibility, read: where the roles come from, and how writeups start out. */
export function WorkspaceVisibilitySummary({ workspace }: Readonly<{ workspace: WorkspaceResponse }>) {
  const { t } = useTranslation(["workspaces"]);
  return (
    <div className="flex flex-col gap-7">
      <ReadOnlyNote workspace={workspace} />
      <TeamRoles workspace={workspace} />
      <SettingsSection id="ws-visibility" title={t("workspaces:settings.writeupVisibility.title")}>
        <SettingRow label={t("workspaces:settings.writeupVisibility.defaultLabel")} hint={t("workspaces:settings.writeupVisibility.defaultHint")}>
          <Value>{t(`workspaces:settings.writeupVisibility.${workspace.default_writeup_visibility}`)}</Value>
        </SettingRow>
      </SettingsSection>
    </div>
  );
}
