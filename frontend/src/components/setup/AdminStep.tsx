// src/components/setup/AdminStep.tsx
import { useForm, useWatch } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field";
import { FormError } from "@/components/common/FormError";
import { PasswordStrength } from "@/components/auth/PasswordStrength";
import { loadEstimator } from "@/lib/password-strength";
import { validateAdmin } from "@/api/setup";
import { adminSchema, type AdminValues } from "@/lib/schemas/setup";
import { isApiError } from "@/lib/api-error";

/**
 * Which input a server rejection belongs under.
 *
 * The three rules the client cannot check itself — reserved username, blocked
 * email domain, breached password — each map to exactly one field.
 */
const FIELD_BY_CODE: Record<string, keyof AdminValues> = {
  username_taken: "admin_username",
  username_reserved: "admin_username",
  email_taken: "admin_email",
  invalid_email: "admin_email",
  email_domain_not_allowed: "admin_email",
  password_compromised: "admin_password",
};

/**
 * A rejection to place under an input, or `null` for the banner.
 *
 * Two shapes reach here: a semantic code from the table above, and a 422 whose
 * `fields` breakdown names the offending validator. Only the composition rule
 * is worth reading out of the second — every other rule in it is enforced by
 * the schema, so seeing one means client and server have drifted apart.
 */
function fieldForError(error: unknown): { field: keyof AdminValues; message: string } | null {
  if (!isApiError(error)) return null;

  const direct = FIELD_BY_CODE[error.code];
  if (direct) return { field: direct, message: `errors:api.${error.code}` };

  if (error.fields?.admin_password?.includes("password_composition")) {
    return { field: "admin_password", message: "auth:validation.passwordComposition" };
  }

  return null;
}

export function AdminStep({
  token,
  defaultValues,
  onBack,
  onNext,
}: Readonly<{
  token: string;
  defaultValues: AdminValues | null;
  onBack: () => void;
  onNext: (values: AdminValues) => void;
}>) {
  const { t } = useTranslation(["setup", "auth", "common"]);

  const {
    register,
    handleSubmit,
    setError,
    control,
    formState: { errors },
  } = useForm<AdminValues>({
    resolver: zodResolver(adminSchema),
    defaultValues: defaultValues ?? {
      admin_username: "",
      admin_email: "",
      admin_password: "",
    },
  });

  // `useWatch`, not the `watch()` from `useForm`: that one is a function the
  // React Compiler refuses to memoize around.
  const password = useWatch({ control, name: "admin_password" });

  const check = useMutation({
    mutationFn: (values: AdminValues) => validateAdmin(token, values),
    onSuccess: (_result, values) => onNext(values),
    onError: (error) => {
      const placed = fieldForError(error);
      if (placed) {
        setError(placed.field, { message: placed.message }, { shouldFocus: true });
      }
    },
  });

  // Anything that did not land under an input — rate limit, backend down —
  // still has to be visible somewhere.
  const bannerError = check.error && !fieldForError(check.error) ? check.error : null;

  return (
    <form onSubmit={handleSubmit((values) => check.mutate(values))} className="flex flex-col gap-5">
      <FormError error={bannerError} />

      <FieldGroup>
        <Field data-invalid={Boolean(errors.admin_username)}>
          <FieldLabel htmlFor="admin_username">{t("common:fields.handle")}</FieldLabel>
          <Input
            id="admin_username"
            type="text"
            autoComplete="username"
            autoFocus
            aria-invalid={Boolean(errors.admin_username)}
            placeholder={t("auth:fields.handlePlaceholder")}
            disabled={check.isPending}
            {...register("admin_username")}
          />
          {errors.admin_username?.message ? (
            <FieldError>{t(errors.admin_username.message)}</FieldError>
          ) : null}
        </Field>

        <Field data-invalid={Boolean(errors.admin_email)}>
          <FieldLabel htmlFor="admin_email">{t("common:fields.email")}</FieldLabel>
          <Input
            id="admin_email"
            type="email"
            autoComplete="email"
            aria-invalid={Boolean(errors.admin_email)}
            placeholder={t("auth:fields.emailPlaceholder")}
            disabled={check.isPending}
            {...register("admin_email")}
          />
          <FieldDescription>{t("setup:admin.emailHint")}</FieldDescription>
          {errors.admin_email?.message ? (
            <FieldError>{t(errors.admin_email.message)}</FieldError>
          ) : null}
        </Field>

        <Field data-invalid={Boolean(errors.admin_password)}>
          <FieldLabel htmlFor="admin_password">{t("common:fields.password")}</FieldLabel>
          <Input
            id="admin_password"
            type="password"
            autoComplete="new-password"
            aria-invalid={Boolean(errors.admin_password)}
            placeholder={t("auth:fields.passwordPlaceholder")}
            disabled={check.isPending}
            onFocus={() => void loadEstimator()}
            {...register("admin_password")}
          />
          <PasswordStrength password={password} />
          <FieldDescription>{t("auth:password.hint")}</FieldDescription>
          {errors.admin_password?.message ? (
            <FieldError>{t(errors.admin_password.message)}</FieldError>
          ) : null}
        </Field>
      </FieldGroup>

      <div className="flex gap-2">
        <Button
          type="button"
          variant="secondary"
          size="lg"
          onClick={onBack}
          disabled={check.isPending}
        >
          {t("setup:actions.back")}
        </Button>
        <Button type="submit" size="lg" className="flex-1" disabled={check.isPending}>
          {check.isPending ? <Spinner /> : null}
          {t("common:actions.next")}
        </Button>
      </div>
    </form>
  );
}
