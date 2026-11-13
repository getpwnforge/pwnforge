import { Controller, useFormContext, useWatch, type FieldError as FormFieldError } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { ColorPicker } from "@/components/ui/color-picker";
import { Field, FieldContent, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { PLATFORMS, type CreateWorkspaceValues } from "@/lib/schemas/workspace";

/** Radix Select has no empty item: this stands for "no platform" on an event. */
const NO_PLATFORM = "none";

/** 2 · Details: identity, platform, and the dates of an event. */
export function DetailsSection() {
  const { t } = useTranslation(["workspaces", "nav", "errors"]);
  const { register, control, formState: { errors } } = useFormContext<CreateWorkspaceValues>();
  const type = useWatch({ control, name: "type" });
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;

  // Messages are i18n keys, from the schema or from the API.
  const shown = (error?: FormFieldError) => (error ? [{ message: t(error.message ?? "errors:field.invalid") }] : undefined);

  return (
    <section aria-labelledby="ws-details-title" className="flex flex-col gap-4">
      <h3 id="ws-details-title" className="text-sm font-semibold text-text">
        {t("workspaces:create.steps.details")}
      </h3>

      <div className="grid gap-4 sm:grid-cols-2">
        <Field data-invalid={!!errors.name}>
          <FieldLabel htmlFor="ws-name">{t("workspaces:create.details.name")}</FieldLabel>
          <Input id="ws-name" autoComplete="off" aria-invalid={!!errors.name} {...register("name")} />
          <FieldError errors={shown(errors.name)} />
        </Field>

        <Controller
          control={control}
          name="platform"
          render={({ field, fieldState }) => (
            <Field data-invalid={!!fieldState.error}>
              <FieldLabel id="ws-platform-label" htmlFor="ws-platform">
                {t("workspaces:create.details.platform")}
              </FieldLabel>
              <Select value={field.value ?? ""} onValueChange={(value) => field.onChange(value === NO_PLATFORM ? null : value)}>
                <SelectTrigger
                  ref={field.ref}
                  id="ws-platform"
                  aria-labelledby="ws-platform-label"
                  aria-invalid={!!fieldState.error}
                  className="w-full"
                >
                  <SelectValue placeholder={t("workspaces:create.details.platformPlaceholder")} />
                </SelectTrigger>
                <SelectContent>
                  {type === "event" ? <SelectItem value={NO_PLATFORM}>{t("workspaces:create.details.noPlatform")}</SelectItem> : null}
                  {PLATFORMS.map((platform) => (
                    <SelectItem key={platform} value={platform}>
                      {t(`nav:platforms.${platform}`)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <FieldError errors={shown(fieldState.error)} />
            </Field>
          )}
        />
      </div>

      <Field>
        <FieldLabel htmlFor="ws-description">{t("workspaces:create.details.description")}</FieldLabel>
        <Textarea id="ws-description" rows={2} {...register("description")} />
      </Field>

      <div className="flex flex-col gap-2">
        {/* The picker names itself for screen readers; this is the visible label. */}
        <span aria-hidden className="text-sm font-medium text-text">
          {t("workspaces:create.details.color")}
        </span>
        <Controller
          control={control}
          name="color"
          render={({ field }) => (
            <ColorPicker name="color" label={t("workspaces:create.details.color")} value={field.value} onChange={field.onChange} />
          )}
        />
      </div>

      {type === "event" ? (
        <>
          <div className="grid gap-4 sm:grid-cols-2">
            <Field>
              <FieldLabel htmlFor="ws-starts">{t("workspaces:create.details.startsAt")}</FieldLabel>
              <Input id="ws-starts" type="datetime-local" {...register("starts_at")} />
            </Field>
            <Field data-invalid={!!errors.ends_at}>
              <FieldLabel htmlFor="ws-ends">{t("workspaces:create.details.endsAt")}</FieldLabel>
              <Input id="ws-ends" type="datetime-local" aria-invalid={!!errors.ends_at} {...register("ends_at")} />
              <FieldError errors={shown(errors.ends_at)} />
            </Field>
          </div>
          <p className="-mt-2 text-xs text-text-muted">{t("workspaces:create.details.timezone", { zone })}</p>

          <Controller
            control={control}
            name="lock_after_end"
            render={({ field }) => (
              <Field orientation="horizontal">
                <FieldContent>
                  <FieldLabel htmlFor="ws-lock">{t("workspaces:create.details.lockAfterEnd")}</FieldLabel>
                  <FieldDescription>{t("workspaces:create.details.lockAfterEndHelp")}</FieldDescription>
                </FieldContent>
                <Switch id="ws-lock" checked={field.value} onCheckedChange={field.onChange} />
              </Field>
            )}
          />
        </>
      ) : null}
    </section>
  );
}
