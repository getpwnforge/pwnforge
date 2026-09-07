// src/components/setup/SettingsStep.tsx
import { useMemo } from "react";
import { Controller, useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field";
import { settingsSchema, type SettingsValues } from "@/lib/schemas/setup";
import { listTimeZones, resolvedTimeZone } from "@/lib/timezones";

/** Kept in step with `supportedLngs` in `lib/i18n.ts`. */
const LOCALES = ["en", "fr"] as const;

function localeLabel(locale: string, displayIn: string): string {
  try {
    return new Intl.DisplayNames([displayIn], { type: "language" }).of(locale) ?? locale;
  } catch {
    return locale;
  }
}

export function SettingsStep({
  defaultValues,
  onBack,
  onNext,
}: Readonly<{
  defaultValues: SettingsValues | null;
  onBack: () => void;
  onNext: (values: SettingsValues) => void;
}>) {
  const { t, i18n } = useTranslation(["setup", "common"]);

  // ~400 entries, each needing an Intl format call: build once.
  const timeZones = useMemo(() => listTimeZones(), []);

  const {
    control,
    handleSubmit,
    formState: { errors },
  } = useForm<SettingsValues>({
    resolver: zodResolver(settingsSchema),
    defaultValues: defaultValues ?? {
      default_locale: LOCALES.includes(i18n.resolvedLanguage as (typeof LOCALES)[number])
        ? (i18n.resolvedLanguage as string)
        : "en",
      // Preselected from the browser, which is the operator's own zone in
      // practice — they can still pick any other.
      default_timezone: resolvedTimeZone(),
      allow_public_signup: true,
      hide_landing_page: false,
    },
  });

  return (
    <form onSubmit={handleSubmit(onNext)} className="flex flex-col gap-5">
      <FieldGroup>
        <Field data-invalid={Boolean(errors.default_locale)}>
          <FieldLabel htmlFor="default_locale">{t("setup:settings.locale")}</FieldLabel>
          <Controller
            control={control}
            name="default_locale"
            render={({ field }) => (
              <Select value={field.value} onValueChange={field.onChange}>
                <SelectTrigger id="default_locale" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {LOCALES.map((locale) => (
                    <SelectItem key={locale} value={locale}>
                      {localeLabel(locale, i18n.language)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          />
          <FieldDescription>{t("setup:settings.localeHint")}</FieldDescription>
          {errors.default_locale?.message ? (
            <FieldError>{t(errors.default_locale.message)}</FieldError>
          ) : null}
        </Field>

        <Field data-invalid={Boolean(errors.default_timezone)}>
          <FieldLabel htmlFor="default_timezone">{t("setup:settings.timezone")}</FieldLabel>
          <Controller
            control={control}
            name="default_timezone"
            render={({ field }) => (
              <Select value={field.value} onValueChange={field.onChange}>
                <SelectTrigger id="default_timezone" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent className="max-h-72">
                  {timeZones.map((zone) => (
                    <SelectItem key={zone.id} value={zone.id}>
                      {zone.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          />
          <FieldDescription>{t("setup:settings.timezoneHint")}</FieldDescription>
          {errors.default_timezone?.message ? (
            <FieldError>{t(errors.default_timezone.message)}</FieldError>
          ) : null}
        </Field>

        <Field orientation="horizontal">
          <Controller
            control={control}
            name="allow_public_signup"
            render={({ field }) => (
              <Switch
                id="allow_public_signup"
                checked={field.value}
                onCheckedChange={field.onChange}
              />
            )}
          />
          <div className="flex flex-col gap-0.5">
            <FieldLabel htmlFor="allow_public_signup">
              {t("setup:settings.publicSignup")}
            </FieldLabel>
            <FieldDescription>{t("setup:settings.publicSignupHint")}</FieldDescription>
          </div>
        </Field>

        <Field orientation="horizontal">
          <Controller
            control={control}
            name="hide_landing_page"
            render={({ field }) => (
              <Switch
                id="hide_landing_page"
                checked={field.value}
                onCheckedChange={field.onChange}
              />
            )}
          />
          <div className="flex flex-col gap-0.5">
            <FieldLabel htmlFor="hide_landing_page">
              {t("setup:settings.hideLandingPage")}
            </FieldLabel>
            <FieldDescription>{t("setup:settings.hideLandingPageHint")}</FieldDescription>
          </div>
        </Field>
      </FieldGroup>

      <div className="flex gap-2">
        <Button type="button" variant="secondary" size="lg" onClick={onBack}>
          {t("setup:actions.back")}
        </Button>
        <Button type="submit" size="lg" className="flex-1">
          {t("common:actions.next")}
        </Button>
      </div>
    </form>
  );
}
