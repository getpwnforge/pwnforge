import { Controller, useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { FormError } from "@/components/common/FormError";
import { SettingsFormBar } from "@/components/settings/SettingsFormBar";
import { SettingRow, SettingsSection } from "@/components/settings/settings-layout";
import { TeamRoles, WorkspaceVisibilitySummary } from "@/components/settings/settings-summary";
import { UnsavedChangesGuard, type SaveThen } from "@/components/settings/UnsavedChangesGuard";
import { Tabs, TabsList, TabsTab } from "@/components/ui/tabs";
import { useUpdateWorkspace } from "@/hooks/useWorkspaces";
import type { WorkspaceResponse } from "@/lib/api/types";
import { canEditWorkspace } from "@/lib/permissions";
import { WRITEUP_VISIBILITIES } from "@/lib/schemas/workspace";

type Visibility = (typeof WRITEUP_VISIBILITIES)[number];
type Values = { default_writeup_visibility: Visibility };

const isVisibility = (value: unknown): value is Visibility => WRITEUP_VISIBILITIES.includes(value as Visibility);
const valuesOf = (workspace: WorkspaceResponse): Values => ({
  default_writeup_visibility: workspace.default_writeup_visibility,
});

/**
 * Who reaches the workspace, and how its writeups start out. Saved with the bar, like every form;
 * whoever cannot edit gets the values instead (`WorkspaceVisibilitySummary`).
 */
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

  if (!canEdit) return <WorkspaceVisibilitySummary workspace={workspace} />;

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
                    <TabsTab key={value} value={value}>
                      {t(`workspaces:settings.writeupVisibility.${value}`)}
                    </TabsTab>
                  ))}
                </TabsList>
              </Tabs>
            )}
          />
        </SettingRow>
      </SettingsSection>

      <SettingsFormBar dirty={isDirty} saving={update.isPending} onCancel={() => form.reset(valuesOf(workspace))} />
      <UnsavedChangesGuard dirty={isDirty} saving={update.isPending} onSave={saveThen} />
    </form>
  );
}
