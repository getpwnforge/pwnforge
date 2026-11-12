import { zodResolver } from "@hookform/resolvers/zod";
import { useRef, type ChangeEvent } from "react";
import { Controller, useForm, useWatch } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { WsIcon } from "@/components/brand/WsIcon";
import { FormError } from "@/components/common/FormError";
import { SettingsFormBar } from "@/components/settings/SettingsFormBar";
import { TeamName, WorkspaceGeneralSummary } from "@/components/settings/settings-summary";
import { SettingRow, SettingsSection } from "@/components/settings/settings-layout";
import { UnsavedChangesGuard, type SaveThen } from "@/components/settings/UnsavedChangesGuard";
import { Button } from "@/components/ui/button";
import { ColorPicker } from "@/components/ui/color-picker";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { useDeleteWorkspaceAvatar, useUpdateWorkspace, useUploadWorkspaceAvatar } from "@/hooks/useWorkspaces";
import { apiErrorKey, fieldErrorKey, isApiError } from "@/lib/api-error";
import type { WorkspaceResponse } from "@/lib/api/types";
import { avatarUrl } from "@/lib/avatar";
import { canEditWorkspace } from "@/lib/permissions";
import {
  PLATFORMS,
  toLocalInput,
  toWorkspacePatch,
  updateWorkspaceSchema,
  type UpdateWorkspaceValues,
} from "@/lib/schemas/workspace";

/** The API codes that belong under one field rather than above the form. */
const FIELD_OF_CODE: Record<string, keyof UpdateWorkspaceValues> = {
  workspace_name_reserved: "name",
  workspace_name_not_sluggable: "name",
  invalid_event_url: "event_url",
  platform_required: "platform",
};
/** Fields whose validator codes the API can list in `fields`. */
const LISTED_FIELDS = ["name", "event_url"] as const;
/** Image types the avatar picker offers; the server checks the file again. */
const AVATAR_TYPES = "image/png,image/jpeg,image/webp";
/** Radix Select has no empty item: this stands for "no platform" on an event. */
const NO_PLATFORM = "none";

/** Where a refused save shows: under one field, or (null) above the form. */
function placedErrorOf(error: unknown): { field: keyof UpdateWorkspaceValues; message: string } | null {
  if (!isApiError(error)) return null;
  const field = FIELD_OF_CODE[error.code];
  if (field) return { field, message: apiErrorKey(error) };
  for (const listed of LISTED_FIELDS) {
    const code = error.fields?.[listed]?.[0];
    if (code) return { field: listed, message: fieldErrorKey(code) };
  }
  return null;
}

function valuesOf(workspace: WorkspaceResponse): UpdateWorkspaceValues {
  return {
    name: workspace.name,
    description: workspace.description ?? "",
    color: workspace.color,
    platform: workspace.platform ?? null,
    event_url: workspace.event_url ?? "",
    starts_at: toLocalInput(workspace.starts_at),
    ends_at: toLocalInput(workspace.ends_at),
    lock_after_end: workspace.lock_after_end,
    default_writeup_visibility: workspace.default_writeup_visibility,
  };
}

/**
 * Identity, type and source, mode and team, and the event's dates, saved together with the bar and
 * only for what changed. Whoever cannot edit, and everyone while the workspace is archived, gets
 * the values instead (`WorkspaceGeneralSummary`): no control that would do nothing.
 */
export function WorkspaceGeneralForm({ workspace }: Readonly<{ workspace: WorkspaceResponse }>) {
  const { t } = useTranslation(["workspaces", "nav", "errors"]);
  const update = useUpdateWorkspace();
  const canEdit = canEditWorkspace(workspace, workspace.my_role);
  // `values` follows the workspace after a save; edits not yet saved survive a refresh from elsewhere.
  const form = useForm<UpdateWorkspaceValues>({
    resolver: zodResolver(updateWorkspaceSchema),
    values: valuesOf(workspace),
    resetOptions: { keepDirtyValues: true },
  });
  const [name, color] = useWatch({ control: form.control, name: ["name", "color"] });
  const { dirtyFields, errors, isDirty } = form.formState;
  const fieldError = (message: string | undefined) => (message ? [{ message: t(message) }] : undefined);

  const submit = (values: UpdateWorkspaceValues, outcome?: Parameters<SaveThen>[0]) =>
    update.mutate(
      { slug: workspace.slug, patch: toWorkspacePatch(values, dirtyFields) },
      {
        onSuccess: () => {
          // Clean at once: the refreshed workspace follows, and a pending navigation can go on.
          form.reset(values);
          toast.success(t("workspaces:settings.saved"));
          outcome?.onSaved();
        },
        onError: (error) => {
          const placed = placedErrorOf(error);
          if (placed) form.setError(placed.field, { message: placed.message }, { shouldFocus: true });
          outcome?.onFailed();
        },
      },
    );
  const saveThen: SaveThen = (outcome) => void form.handleSubmit((values) => submit(values, outcome), outcome.onFailed)();
  const unplaced = update.error && !placedErrorOf(update.error) ? update.error : null;

  if (!canEdit) return <WorkspaceGeneralSummary workspace={workspace} />;

  return (
    <form noValidate className="flex flex-col gap-7" onSubmit={(event) => void form.handleSubmit((values) => submit(values))(event)}>
      {unplaced ? <FormError error={unplaced} /> : null}

      <div className="flex min-w-0 flex-col gap-7">
        <SettingsSection id="ws-identity" title={t("workspaces:settings.identity.title")}>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field data-invalid={!!errors.name}>
              <FieldLabel htmlFor="ws-settings-name">{t("workspaces:settings.identity.workspaceName")}</FieldLabel>
              <Input id="ws-settings-name" autoComplete="off" aria-invalid={!!errors.name} {...form.register("name")} />
              <FieldError errors={fieldError(errors.name?.message)} />
            </Field>
            <Field>
              <FieldLabel htmlFor="ws-settings-slug">{t("workspaces:settings.identity.slug")}</FieldLabel>
              <Input id="ws-settings-slug" disabled value={workspace.slug} className="font-mono text-text-muted" />
              <FieldDescription>{t("workspaces:settings.identity.slugHint")}</FieldDescription>
            </Field>
          </div>

          <Field data-invalid={!!errors.description} className="mt-3">
            <FieldLabel htmlFor="ws-settings-description">{t("workspaces:settings.identity.description")}</FieldLabel>
            <Textarea id="ws-settings-description" rows={2} aria-invalid={!!errors.description} {...form.register("description")} />
            <FieldError errors={fieldError(errors.description?.message)} />
          </Field>

          <div className="mt-3 flex flex-wrap items-start gap-4">
            <WsIcon
              name={name || workspace.name}
              color={color}
              size="lg"
              src={avatarUrl("workspaces", workspace.id, workspace.avatar_version, 256)}
            />
            <div className="flex flex-col gap-2">
              <Controller
                control={form.control}
                name="color"
                render={({ field }) => (
                  <ColorPicker name="ws-color" label={t("workspaces:settings.identity.color")} value={field.value} onChange={field.onChange} />
                )}
              />
              <AvatarButtons workspace={workspace} />
            </div>
          </div>
        </SettingsSection>

        <SettingsSection id="ws-mode" title={t("workspaces:settings.modeTeam.title")}>
          <SettingRow label={t("workspaces:settings.modeTeam.workspaceType")} hint={t("workspaces:settings.modeTeam.workspaceTypeHint")}>
            <span className="text-sm text-text">{t(`nav:workspaceModes.${workspace.mode}`)}</span>
          </SettingRow>
          {workspace.team_id ? (
            <SettingRow label={t("workspaces:settings.modeTeam.associatedTeam")} hint={t("workspaces:settings.modeTeam.associatedTeamHint")}>
              <TeamName teamId={workspace.team_id} />
            </SettingRow>
          ) : null}
        </SettingsSection>

        <SettingsSection id="ws-source" title={t("workspaces:settings.source.title")}>
          <SettingRow label={t("workspaces:settings.source.type")} hint={t("workspaces:settings.source.typeHint")}>
            <span className="text-sm text-text">{t(`workspaces:create.type.${workspace.type}`)}</span>
          </SettingRow>
          <div className="mt-3 grid gap-3 sm:grid-cols-2">
            <Controller
              control={form.control}
              name="platform"
              render={({ field, fieldState }) => (
                <Field data-invalid={!!fieldState.error}>
                  <FieldLabel id="ws-settings-platform-label" htmlFor="ws-settings-platform">
                    {t("workspaces:settings.source.platform")}
                  </FieldLabel>
                  <Select
                    value={field.value ?? (workspace.type === "event" ? NO_PLATFORM : "")}
                    onValueChange={(value) => field.onChange(value === NO_PLATFORM ? null : value)}
                  >
                    <SelectTrigger
                      ref={field.ref}
                      id="ws-settings-platform"
                      aria-labelledby="ws-settings-platform-label"
                      aria-invalid={!!fieldState.error}
                      className="w-full"
                    >
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {/* A platform workspace always keeps one; only an event may have none. */}
                      {workspace.type === "event" ? (
                        <SelectItem value={NO_PLATFORM}>{t("workspaces:settings.source.noPlatform")}</SelectItem>
                      ) : null}
                      {PLATFORMS.map((platform) => (
                        <SelectItem key={platform} value={platform}>
                          {t(`nav:platforms.${platform}`)}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <FieldError errors={fieldError(fieldState.error?.message)} />
                </Field>
              )}
            />
            <Field data-invalid={!!errors.event_url}>
              <FieldLabel htmlFor="ws-settings-event-url">{t("workspaces:settings.source.eventUrl")}</FieldLabel>
              <Input
                id="ws-settings-event-url"
                type="url"
                inputMode="url"
                autoComplete="off"
                spellCheck={false}
                placeholder="https://"
                aria-invalid={!!errors.event_url}
                {...form.register("event_url")}
              />
              <FieldDescription>{t("workspaces:settings.source.eventUrlHint")}</FieldDescription>
              <FieldError errors={fieldError(errors.event_url?.message)} />
            </Field>
          </div>
        </SettingsSection>

        {workspace.type === "event" ? (
          <SettingsSection id="ws-event" title={t("workspaces:settings.event.title")}>
            <div className="grid gap-3 sm:grid-cols-2">
              <Controller
                control={form.control}
                name="starts_at"
                render={({ field }) => (
                  <Field>
                    <FieldLabel htmlFor="ws-settings-starts">{t("workspaces:settings.event.start")}</FieldLabel>
                    <Input
                      id="ws-settings-starts"
                      type="datetime-local"
                      value={field.value ?? ""}
                      onChange={(event) => field.onChange(event.target.value || null)}
                      onBlur={field.onBlur}
                    />
                  </Field>
                )}
              />
              <Controller
                control={form.control}
                name="ends_at"
                render={({ field }) => (
                  <Field data-invalid={!!errors.ends_at}>
                    <FieldLabel htmlFor="ws-settings-ends">{t("workspaces:settings.event.end")}</FieldLabel>
                    <Input
                      id="ws-settings-ends"
                      type="datetime-local"
                      aria-invalid={!!errors.ends_at}
                      value={field.value ?? ""}
                      onChange={(event) => field.onChange(event.target.value || null)}
                      onBlur={field.onBlur}
                    />
                    <FieldError errors={fieldError(errors.ends_at?.message)} />
                  </Field>
                )}
              />
            </div>
            <Controller
              control={form.control}
              name="lock_after_end"
              render={({ field }) => (
                <div className="mt-3 flex items-center gap-4 border-b border-border py-2.5">
                  <div className="min-w-0 flex-1">
                    <label htmlFor="ws-settings-lock" className="text-[13px] font-medium text-text">
                      {t("workspaces:settings.event.lockAfterEnd")}
                    </label>
                    <p className="mt-0.5 text-xs text-text-muted">{t("workspaces:settings.event.lockAfterEndHint")}</p>
                  </div>
                  <Switch id="ws-settings-lock" checked={field.value} onCheckedChange={field.onChange} />
                </div>
              )}
            />
            {/* Listed ahead of its job (end of phase 5): shown, never stored. */}
            <SettingRow label={t("workspaces:settings.event.autoArchive")} hint={t("workspaces:settings.event.autoArchiveHint")}>
              <span aria-disabled="true" className="flex items-center gap-2 text-sm text-text-subtle">
                {t("workspaces:settings.event.autoArchiveNever")}
                <span className="text-[10.5px]">{t("workspaces:settings.soon")}</span>
              </span>
            </SettingRow>
          </SettingsSection>
        ) : null}
      </div>

      <SettingsFormBar dirty={isDirty} saving={update.isPending} onCancel={() => form.reset(valuesOf(workspace))} />
      <UnsavedChangesGuard dirty={isDirty} saving={update.isPending} onSave={saveThen} />
    </form>
  );
}

/** Add, change or remove the image. Each click is sent at once: an image is not a form field. */
function AvatarButtons({ workspace }: Readonly<{ workspace: WorkspaceResponse }>) {
  const { t } = useTranslation(["workspaces", "errors"]);
  const upload = useUploadWorkspaceAvatar();
  const remove = useDeleteWorkspaceAvatar();
  const fileInput = useRef<HTMLInputElement>(null);
  const onError = (error: Error) => toast.error(t(apiErrorKey(error)));
  const target = { workspaceId: workspace.id, slug: workspace.slug };

  const pickFile = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    // Cleared so the same file can be picked again after a refusal.
    event.target.value = "";
    if (file) upload.mutate({ ...target, image: file }, { onError });
  };

  return (
    <div className="flex gap-2">
      <input ref={fileInput} type="file" accept={AVATAR_TYPES} className="sr-only" tabIndex={-1} aria-hidden onChange={pickFile} />
      <Button type="button" variant="secondary" size="sm" disabled={upload.isPending} onClick={() => fileInput.current?.click()}>
        {t(workspace.avatar_version ? "workspaces:settings.identity.avatarChange" : "workspaces:settings.identity.avatarAdd")}
      </Button>
      {workspace.avatar_version ? (
        <Button type="button" variant="ghost" size="sm" disabled={remove.isPending} onClick={() => remove.mutate(target, { onError })}>
          {t("workspaces:settings.identity.avatarRemove")}
        </Button>
      ) : null}
    </div>
  );
}
