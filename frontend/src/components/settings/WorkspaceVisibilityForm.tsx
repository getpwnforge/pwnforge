import { Controller, useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { toast } from "sonner";

import { FormError } from "@/components/common/FormError";
import { SettingsFormBar } from "@/components/settings/SettingsFormBar";
import { SettingRow, SettingsSection } from "@/components/settings/settings-layout";
import { UnsavedChangesGuard, type SaveThen } from "@/components/settings/UnsavedChangesGuard";
import { Button } from "@/components/ui/button";
import { Tabs, TabsList, TabsTab } from "@/components/ui/tabs";
import { useTeams } from "@/hooks/useTeams";
import { useUpdateWorkspace } from "@/hooks/useWorkspaces";
import type { WorkspaceResponse } from "@/lib/api/types";
import { canEditWorkspace } from "@/lib/permissions";
import { ROUTES } from "@/lib/routes";
import { WRITEUP_VISIBILITIES } from "@/lib/schemas/workspace";
import { ChevronRight } from "lucide-react";

type Visibility = (typeof WRITEUP_VISIBILITIES)[number];
type Values = { default_writeup_visibility: Visibility };

const isVisibility = (value: unknown): value is Visibility => WRITEUP_VISIBILITIES.includes(value as Visibility);
const valuesOf = (workspace: WorkspaceResponse): Values => ({
  default_writeup_visibility: workspace.default_writeup_visibility,
});

/** Who reaches the workspace, and how its writeups start out. Saved with the bar, like every form. */
export function WorkspaceVisibilityForm({ workspace }: Readonly<{ workspace: WorkspaceResponse }>) {
  const { t } = useTranslation(["workspaces"]);
  const update = useUpdateWorkspace();
  const canEdit = canEditWorkspace(workspace, workspace.my_role);
  const form = useForm<Values>({ values: valuesOf(workspace), resetOptions: { keepDirtyValues: true } });
  const { isDirty } = form.formState;

  const submit = (values: Values, outcome?: Parameters<SaveThen>[0]) =>
    update.mutate(
      { slug: workspace.slug, patch: values },
      {
        onSuccess: () => {
          form.reset(values);
          toast.success(t("workspaces:settings.saved"));
          outcome?.onSaved();
        },
        onError: () => outcome?.onFailed(),
      },
    );
  const saveThen: SaveThen = (outcome) => void form.handleSubmit((values) => submit(values, outcome), outcome.onFailed)();

  return (
    <form noValidate className="flex flex-col gap-7" onSubmit={(event) => void form.handleSubmit((values) => submit(values))(event)}>
      {update.error ? <FormError error={update.error} /> : null}
      <TeamRoles workspace={workspace} />

      <SettingsSection id="ws-visibility" title={t("workspaces:settings.writeupVisibility.title")}>
        <SettingRow label={t("workspaces:settings.writeupVisibility.defaultLabel")} hint={t("workspaces:settings.writeupVisibility.defaultHint")}>
          <Controller
            control={form.control}
            name="default_writeup_visibility"
            render={({ field }) => (
              <Tabs
                value={field.value}
                onValueChange={(value) => {
                  if (isVisibility(value)) field.onChange(value);
                }}
              >
                <TabsList aria-label={t("workspaces:settings.writeupVisibility.defaultLabel")}>
                  {WRITEUP_VISIBILITIES.map((value) => (
                    <TabsTab key={value} value={value} disabled={!canEdit}>
                      {t(`workspaces:settings.writeupVisibility.${value}`)}
                    </TabsTab>
                  ))}
                </TabsList>
              </Tabs>
            )}
          />
        </SettingRow>
      </SettingsSection>

      {canEdit ? (
        <>
          <SettingsFormBar dirty={isDirty} saving={update.isPending} onCancel={() => form.reset(valuesOf(workspace))} />
          <UnsavedChangesGuard dirty={isDirty} saving={update.isPending} onSave={saveThen} />
        </>
      ) : null}
    </form>
  );
}

/**
 * Roles come from the team: point there. Only for a team workspace, and only to whoever is on
 * that team (from my teams); a guest would land on a team they cannot open.
 */
function TeamRoles({ workspace }: Readonly<{ workspace: WorkspaceResponse }>) {
  const { t } = useTranslation(["workspaces"]);
  const teams = useTeams();
  const team = workspace.team_id ? teams.data?.find((candidate) => candidate.id === workspace.team_id) : undefined;
  if (!team) return null;

  return (
    <SettingsSection id="ws-access" title={t("workspaces:settings.roles.title")}>
      <SettingRow label={t("workspaces:settings.roles.label")} hint={t("workspaces:settings.roles.hint", { team: team.name })}>
        <Button asChild variant="link" size="sm">
          <Link to={{ pathname: ROUTES.team(team.slug), search: "?tab=roles" }}>{t("workspaces:settings.roles.manage")}<ChevronRight /></Link>
        </Button>
      </SettingRow>
    </SettingsSection>
  );
}
