import { useForm } from "react-hook-form";
import { useState } from "react";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation } from "@tanstack/react-query";
import { Link, useNavigate, useSearchParams } from "react-router";
import { useTranslation } from "react-i18next";
import { EyeOff, Eye } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field";
import { FormError } from "@/components/common/FormError";
import { loginSchema, type LoginValues } from "@/lib/schemas/auth";
import { useAuth } from "@/hooks/useAuth";
import { ROUTES } from "@/lib/routes";
import { InputGroup, InputGroupButton, InputGroupInput } from "@/components/ui/input-group";
import { isApiError } from "@/lib/api-error";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useTurnstile } from "@/hooks/useTurnstile";
import { TurnstileWidget } from "@/components/common/TurnstileWidget";

/**
 * Where to land after signing in.
 *
 * Only same-origin paths are accepted: `//evil.tld` and `https://evil.tld` are
 * both values a browser will happily follow off-site, and this one comes
 * straight from the query string.
 */
function safeRedirect(target: string | null): string {
  if (!target || !target.startsWith("/") || target.startsWith("//")) {
    return ROUTES.workspaces;
  }

  return target;
}

export function LoginPage() {
  const { t } = useTranslation(["auth", "common"]);
  const { login, resendVerification } = useAuth();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const [showPassword, setShowPassword] = useState(false);
  const resend = useMutation({ mutationFn: resendVerification });
  const turnstile = useTurnstile();
  usePageTitle(t("auth:login.tabTitle"));

  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<LoginValues>({
    resolver: zodResolver(loginSchema),
    defaultValues: { username_or_email: "", password: "" },
  });

  const submit = useMutation({
    mutationFn: (values: LoginValues) => login({ ...values, turnstile_token: turnstile.token ?? "" }),
    onSuccess: () => navigate(safeRedirect(searchParams.get("from")), { replace: true }),
    onError: () => turnstile.reset(),
  });
  const unverifiedUserId =
    isApiError(submit.error) && submit.error.code === "email_not_verified"
      ? submit.error.resendToken
      : null;
  let resendLabel = t("auth:login.resendVerification");
  if (resend.isPending) resendLabel = t("auth:login.sendingVerification");
  if (resend.isSuccess) resendLabel = t("auth:login.verificationSent");

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-col gap-1.5">
        <span className="font-mono text-xs uppercase tracking-wider text-text-subtle">
          {t("auth:login.eyebrow")}
        </span>
        <h1 className="text-page font-semibold tracking-hero">{t("auth:login.title")}</h1>
        <p className="text-sm text-muted-foreground">{t("auth:login.subtitle")}</p>
      </header>

      <form onSubmit={handleSubmit((values) => submit.mutate(values))} className="flex flex-col gap-5">
        {/* Every login failure is deliberately vague server-side — wrong
            password and unknown account answer the same code — so there is
            nothing to place under a specific field. */}
        <FormError
          error={submit.error}
          action={
            unverifiedUserId ? (
              <button
                type="button"
                className="font-medium text-ember-text underline-offset-4 hover:underline disabled:opacity-60"
                disabled={resend.isPending}
                onClick={() => resend.mutate({ token: unverifiedUserId })}
              >
                {resendLabel}
              </button>
            ) : null
          }
        />
        {resend.isError ? <FormError error={resend.error} /> : null}

        <FieldGroup>
          <Field data-invalid={Boolean(errors.username_or_email)}>
            <FieldLabel htmlFor="username_or_email">{t("auth:login.identifier")}</FieldLabel>
            <Input
              id="username_or_email"
              type="text"
              autoComplete="username"
              autoFocus
              aria-invalid={Boolean(errors.username_or_email)}
              placeholder={t("auth:login.identifierPlaceholder")}
              disabled={submit.isPending}
              {...register("username_or_email")}
            />
            {errors.username_or_email?.message ? (
              <FieldError>{t(errors.username_or_email.message)}</FieldError>
            ) : null}
          </Field>

          <Field data-invalid={Boolean(errors.password)}>
            <div className="flex items-baseline justify-between gap-2">
              <FieldLabel htmlFor="password">{t("common:fields.password")}</FieldLabel>
              <Link
                to={ROUTES.forgotPassword}
                className="text-xs text-ember-text hover:underline"
              >
                {t("auth:login.forgotPassword")}
              </Link>
            </div>
            <InputGroup>
              <InputGroupInput
                id="password"
                type={showPassword ? "text" : "password"}
                autoComplete="current-password"
                aria-invalid={Boolean(errors.password)}
                placeholder={t("auth:fields.passwordPlaceholder")}
                disabled={submit.isPending}
                {...register("password")}
              />
              <InputGroupButton
                type="button"
                aria-label={
                  showPassword ? t("common:actions.hidePassword") : t("common:actions.showPassword")
                }
                onClick={() => setShowPassword((current) => !current)}
                disabled={submit.isPending}
              >
                {showPassword ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
              </InputGroupButton>
            </InputGroup>
            {errors.password?.message ? (
              <FieldError>{t(errors.password.message)}</FieldError>
            ) : null}
          </Field>
        </FieldGroup>

        <TurnstileWidget siteKey={turnstile.siteKey} widgetRef={turnstile.ref} onSuccess={turnstile.setToken} />

        <Button type="submit" size="lg" className="w-full" disabled={submit.isPending || !turnstile.isReady}>
          {submit.isPending ? <Spinner /> : null}
          {t("auth:login.submit")}
        </Button>
      </form>

      <p className="text-center text-sm text-muted-foreground">
        {t("auth:login.noAccount")}{" "}
        <Link to={ROUTES.register} className="font-medium text-ember-text hover:underline">
          {t("auth:login.createAccount")}
        </Link>
      </p>
    </div>
  );
}
