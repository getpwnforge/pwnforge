import { zodResolver } from "@hookform/resolvers/zod";
import { useRef, type ChangeEvent, type ReactNode } from "react";
import { Controller, useForm, useWatch } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { toast } from "sonner";
import { WsIcon } from "@/components/brand/WsIcon";
import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { FormError } from "@/components/common/FormError";
import { TransferCaptaincyDialog } from "@/components/teams/TransferCaptaincyDialog";
import { Button } from "@/components/ui/button";
import { ColorPicker } from "@/components/ui/color-picker";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { useAuth } from "@/hooks/useAuth";
import { useDeleteTeam, useDeleteTeamAvatar, useRemoveTeamMember, useUpdateTeam, useUploadTeamAvatar } from "@/hooks/useTeams";
import { apiErrorKey, fieldErrorKey, isApiError } from "@/lib/api-error";
import type { TeamPatch, TeamResponse } from "@/lib/api/types";
import { avatarUrl } from "@/lib/avatar";
import { canDeleteTeam, canEditTeam, canLeaveTeam, canTransferCaptaincy } from "@/lib/permissions";
import { ROUTES } from "@/lib/routes";
import { teamSettingsSchema, type TeamSettingsValues } from "@/lib/schemas/team";

const NAME_CODES = new Set(["team_name_not_sluggable", "team_name_reserved"]);

/** Image types the avatar picker offers; the server checks the file again. */
const AVATAR_TYPES = "image/png,image/jpeg,image/webp";

function nameErrorOf(error: unknown): string | null {
  if (!isApiError(error)) return null;
  if (NAME_CODES.has(error.code)) return apiErrorKey(error);
  const listed = error.fields?.name?.[0];
  return listed ? fieldErrorKey(listed) : null;
}

export function TeamSettingsTab({ team }: Readonly<{ team: TeamResponse }>) {
  return (
    <div className="flex max-w-3xl flex-col gap-7">
      {canEditTeam(team.my_role) ? <IdentitySection team={team} /> : null}
      <DangerSection team={team} />
    </div>
  );
}

/** A titled block, as on the mockup: the heading underlined, then its rows. */
function Section({ id, title, children }: Readonly<{ id: string; title: string; children: ReactNode }>) {
  return (
    <section aria-labelledby={id}>
      <h2 id={id} className="mb-3.5 border-b border-border pb-1.5 text-sm font-semibold text-text">
        {title}
      </h2>
      {children}
    </section>
  );
}

/**
 * Avatar, name, color, description and who may create workspaces, as the Admins and the Captain
 * may change them. The form sends only what changed; the avatar is sent as soon as it is picked.
 */
function IdentitySection({ team }: Readonly<{ team: TeamResponse }>) {
  const { t } = useTranslation(["teams", "errors"]);
  const update = useUpdateTeam();
  // `values` follows the team: after a save, or a change from elsewhere, the form shows it.
  const form = useForm<TeamSettingsValues>({
    resolver: zodResolver(teamSettingsSchema),
    values: {
      name: team.name,
      color: team.color,
      description: team.description ?? "",
      members_can_create_workspaces: team.members_can_create_workspaces,
    },
  });
  const [name, color] = useWatch({ control: form.control, name: ["name", "color"] });
  const { dirtyFields, errors, isDirty } = form.formState;

  const submit = (values: TeamSettingsValues) => {
    const patch: TeamPatch = {};
    if (dirtyFields.name) patch.name = values.name.trim();
    if (dirtyFields.color) patch.color = values.color;
    // An emptied description is sent as null, which clears it.
    if (dirtyFields.description) patch.description = values.description.trim() || null;
    if (dirtyFields.members_can_create_workspaces) patch.members_can_create_workspaces = values.members_can_create_workspaces;
    update.mutate(
      { slug: team.slug, patch },
      {
        onSuccess: () => toast.success(t("teams:settings.saved")),
        onError: (error) => {
          const message = nameErrorOf(error);
          if (message) form.setError("name", { message }, { shouldFocus: true });
        },
      },
    );
  };
  const unplaced = update.error && !nameErrorOf(update.error) ? update.error : null;

  return (
    <Section id="team-identity" title={t("teams:settings.identity")}>
      <form noValidate className="flex flex-col gap-4" onSubmit={(event) => void form.handleSubmit(submit)(event)}>
        {unplaced ? <FormError error={unplaced} /> : null}

        <div className="flex flex-wrap items-start gap-4">
          <WsIcon name={name || team.name} color={color} size="lg" src={avatarUrl("teams", team.id, team.avatar_version, 256)} />
          <div className="flex flex-col gap-2">
            <Controller
              control={form.control}
              name="color"
              render={({ field }) => (
                <ColorPicker name="team-color" label={t("teams:settings.color")} value={field.value} onChange={field.onChange} />
              )}
            />
            <p className="text-xs text-text-muted">{t("teams:settings.colorHint")}</p>
            <AvatarButtons team={team} />
          </div>
        </div>

        <div className="grid gap-3 sm:grid-cols-2">
          <Field data-invalid={!!errors.name}>
            <FieldLabel htmlFor="team-settings-name">{t("teams:settings.name")}</FieldLabel>
            <Input id="team-settings-name" autoComplete="off" aria-invalid={!!errors.name} {...form.register("name")} />
            <FieldError errors={errors.name ? [{ message: t(errors.name.message ?? "errors:field.invalid") }] : undefined} />
          </Field>
          <Field>
            <FieldLabel htmlFor="team-settings-slug">{t("teams:settings.slug")}</FieldLabel>
            <Input id="team-settings-slug" disabled value={team.slug} className="font-mono text-text-muted" />
            <FieldDescription>{t("teams:settings.slugHint")}</FieldDescription>
          </Field>
        </div>

        <Field data-invalid={!!errors.description}>
          <FieldLabel htmlFor="team-settings-description">{t("teams:settings.description")}</FieldLabel>
          <Textarea id="team-settings-description" rows={2} aria-invalid={!!errors.description} {...form.register("description")} />
          <FieldError
            errors={errors.description ? [{ message: t(errors.description.message ?? "errors:field.invalid") }] : undefined}
          />
        </Field>

        <Controller
          control={form.control}
          name="members_can_create_workspaces"
          render={({ field }) => (
            <div className="flex items-center gap-4 border-t border-border pt-4">
              <div className="min-w-0 flex-1">
                <label htmlFor="team-members-create" className="text-[13px] font-medium text-text">
                  {t("teams:settings.membersCreate")}
                </label>
                <p className="mt-0.5 text-xs text-text-muted">{t("teams:settings.membersCreateHint")}</p>
              </div>
              <Switch id="team-members-create" checked={field.value} onCheckedChange={field.onChange} />
            </div>
          )}
        />

        <div className="flex justify-end gap-2 border-t border-border pt-4">
          <Button type="button" variant="ghost" disabled={!isDirty} onClick={() => form.reset()}>
            {t("teams:settings.cancel")}
          </Button>
          <Button type="submit" variant="primary" disabled={!isDirty || update.isPending}>
            {t("teams:settings.save")}
          </Button>
        </div>
      </form>
    </Section>
  );
}

/** Add, change or remove the team's image. Each click is sent at once: an image is not a form field. */
function AvatarButtons({ team }: Readonly<{ team: TeamResponse }>) {
  const { t } = useTranslation(["teams"]);
  const upload = useUploadTeamAvatar();
  const remove = useDeleteTeamAvatar();
  const fileInput = useRef<HTMLInputElement>(null);
  const onError = (error: Error) => toast.error(t(apiErrorKey(error)));

  const pickFile = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    // Cleared so the same file can be picked again after a refusal.
    event.target.value = "";
    if (file) upload.mutate({ teamId: team.id, slug: team.slug, image: file }, { onError });
  };

  return (
    <div className="flex gap-2">
      <input ref={fileInput} type="file" accept={AVATAR_TYPES} className="sr-only" tabIndex={-1} aria-hidden onChange={pickFile} />
      <Button type="button" variant="secondary" size="sm" disabled={upload.isPending} onClick={() => fileInput.current?.click()}>
        {t(team.avatar_version ? "teams:settings.avatarChange" : "teams:settings.avatarAdd")}
      </Button>
      {team.avatar_version ? (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          disabled={remove.isPending}
          onClick={() => remove.mutate({ teamId: team.id, slug: team.slug }, { onError })}
        >
          {t("teams:settings.avatarRemove")}
        </Button>
      ) : null}
    </div>
  );
}

/** Leave, transfer and delete, each shown only to whom it is allowed. */
function DangerSection({ team }: Readonly<{ team: TeamResponse }>) {
  const { t } = useTranslation(["teams"]);
  const navigate = useNavigate();
  const { user } = useAuth();
  const leave = useRemoveTeamMember();
  const del = useDeleteTeam();
  const onError = (error: Error) => toast.error(t(apiErrorKey(error)));
  // Leaving or deleting empties the team from the cache: go before its 404 shows.
  const gone = { onSuccess: () => void navigate(ROUTES.teams), onError };

  return (
    <Section id="team-danger" title={t("teams:settings.danger")}>
      <div className="divide-y divide-danger/30 rounded-lg border border-danger/40 bg-danger/5 px-4">
        {canLeaveTeam(team.my_role) && user ? (
          <SettingRow label={t("teams:settings.leave")} hint={t("teams:settings.leaveHint")}>
            <ConfirmDialog
              trigger={<Button variant="secondary" size="sm">{t("teams:settings.leave")}</Button>}
              title={t("teams:settings.leaveTitle", { team: team.name })}
              description={t("teams:settings.leaveText")}
              confirmLabel={t("teams:settings.leave")}
              danger
              pending={leave.isPending}
              onConfirm={() => leave.mutate({ slug: team.slug, userId: user.id }, gone)}
            />
          </SettingRow>
        ) : null}
        {canTransferCaptaincy(team.my_role) ? (
          <SettingRow label={t("teams:settings.transfer")} hint={t("teams:settings.transferHint")}>
            <TransferCaptaincyDialog team={team} />
          </SettingRow>
        ) : null}
        {canDeleteTeam(team.my_role) ? (
          <SettingRow label={t("teams:settings.delete")} hint={t("teams:settings.deleteHint")}>
            <ConfirmDialog
              trigger={<Button variant="danger" size="sm">{t("teams:settings.delete")}</Button>}
              title={t("teams:settings.deleteTitle", { team: team.name })}
              description={t("teams:settings.deleteText", { team: team.name })}
              confirmLabel={t("teams:settings.delete")}
              danger
              confirmText={team.name}
              pending={del.isPending}
              onConfirm={() => del.mutate(team.slug, gone)}
            />
          </SettingRow>
        ) : null}
      </div>
    </Section>
  );
}

/** One setting: label and hint on the left, its control on the right. */
function SettingRow({ label, hint, children }: Readonly<{ label: string; hint: string; children: ReactNode }>) {
  return (
    <div className="flex flex-wrap items-center gap-4 py-2.5">
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-medium text-text">{label}</div>
        <div className="mt-0.5 text-xs text-text-muted">{hint}</div>
      </div>
      {children}
    </div>
  );
}
