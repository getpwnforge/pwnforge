import { useState } from "react";
import { useForm, useWatch } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation } from "@tanstack/react-query";
import { Link } from "react-router";
import { Trans, useTranslation } from "react-i18next";
import { Eye, EyeOff, MailCheck } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field";
import { FormError } from "@/components/common/FormError";
import { PasswordStrength } from "@/components/auth/PasswordStrength";
import { registerSchema, type RegisterValues } from "@/lib/schemas/auth";
import { useAuth } from "@/hooks/useAuth";
import { isApiError } from "@/lib/api-error";
import { ROUTES } from "@/lib/routes";
import { InputGroup, InputGroupButton, InputGroupInput } from "@/components/ui/input-group";
import { useLegalVersions } from "@/api/legal";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useTurnstile } from "@/hooks/useTurnstile";
import { TurnstileWidget } from "@/components/common/TurnstileWidget";

/**
 * Which input a server rejection belongs under.
 *
 * These are the rules the client cannot check for itself: taken and reserved
 * usernames, blocked email domains, passwords found in a breach corpus.
 */
const FIELD_BY_CODE: Record<string, keyof RegisterValues> = {
  username_taken: "username",
  username_reserved: "username",
  email_taken: "email",
  invalid_email: "email",
  email_domain_not_allowed: "email",
  password_compromised: "password",
};

function RegistrationSuccess({ email }: Readonly<{ email: string }>) {
  const { t } = useTranslation(["auth", "common"]);

  return (
    <div className="flex flex-col items-center gap-4 text-center">
      <div className="flex size-11 items-center justify-center rounded-full bg-ember-soft text-ember-text">
        <MailCheck className="size-5" />
      </div>
      <div className="flex flex-col gap-1.5">
        <h1 className="text-page font-semibold tracking-hero">
          {t("auth:register.success.title")}
        </h1>
        <p className="text-sm text-muted-foreground">
          {t("auth:register.success.description", { email })}
        </p>
      </div>
      <Button asChild variant="secondary" size="lg" className="w-full">
        <Link to={ROUTES.login}>{t("auth:register.success.backToLogin")}</Link>
      </Button>
    </div>
  );
}

/**
 * A rejection to place under an input, or `null` for the banner.
 *
 * Two shapes reach here: a semantic code from the table above, and a 422 whose
 * `fields` breakdown names the offending validator. Only the composition rule
 * is worth reading out of the second — every other rule in it is enforced by
 * the schema, so seeing one means client and server have drifted apart.
 */
function fieldForError(
  error: unknown,
): { field: keyof RegisterValues; message: string } | null {
  if (!isApiError(error)) return null;

  const direct = FIELD_BY_CODE[error.code];
  if (direct) return { field: direct, message: `errors:api.${error.code}` };

  if (error.fields?.password?.includes("password_composition")) {
    return { field: "password", message: "auth:validation.passwordComposition" };
  }

  return null;
}

export function RegisterPage() {
  const { t } = useTranslation(["auth", "common"]);
  const { register: createAccount } = useAuth();

  const [termsAccepted, setTermsAccepted] = useState(false);
  // Set once the account exists; also the address the verification mail went to.
  const [registeredEmail, setRegisteredEmail] = useState<string | null>(null);
  const [showPassword, setShowPassword] = useState(false);
  const turnstile = useTurnstile();
  usePageTitle(t("auth:register.tabTitle"));

  const {
    register,
    handleSubmit,
    setError,
    control,
    formState: { errors },
  } = useForm<RegisterValues>({
    resolver: zodResolver(registerSchema),
    defaultValues: { username: "", email: "", password: "" },
  });

  // `useWatch` rather than the `watch()` returned by `useForm`: the latter is a
  // function the React Compiler cannot memoize, and it bails out of optimising
  // the whole component when its result is passed down.
  const password = useWatch({ control, name: "password" });

  const { data: legalVersions, isPending: versionsPending, refetch: refetchVersions } =
    useLegalVersions();

  const submit = useMutation({
    mutationFn: (values: RegisterValues) =>
      createAccount({ ...values, legal: legalVersions!, turnstile_token: turnstile.token ?? "" }),
    onSuccess: (_user, values) => setRegisteredEmail(values.email),
    onError: (error) => {
      turnstile.reset();

      // Documents revised while the form was open. Never retry with the new
      // versions: that would accept, on the user's behalf, a text they never
      // saw. Clear the box and make them read it again.
      if (isApiError(error) && error.code === "legal_version_stale") {
        setTermsAccepted(false);
        void refetchVersions();
        return;
      }

      const placed = fieldForError(error);
      if (placed) {
        setError(placed.field, { message: placed.message }, { shouldFocus: true });
      }
    },
  });

  // Anything that did not land under an input — rate limit, backend down —
  // still has to be visible somewhere.
  const bannerError = submit.error && !fieldForError(submit.error) ? submit.error : null;

  // Registering does not open a session — the account is unusable until the
  // address is verified, so the only next step is the inbox.
  if (registeredEmail) {
    return <RegistrationSuccess email={registeredEmail} />;
  }

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-col gap-1.5">
        <span className="font-mono text-xs uppercase tracking-wider text-text-subtle">
          {t("auth:register.eyebrow")}
        </span>
        <h1 className="text-page font-semibold tracking-hero">{t("auth:register.title")}</h1>
        <p className="text-sm text-muted-foreground">{t("auth:register.subtitle")}</p>
      </header>

      <form onSubmit={handleSubmit((values) => submit.mutate(values))} className="flex flex-col gap-5">
        <FormError error={bannerError} />

        <FieldGroup>
          <Field data-invalid={Boolean(errors.username)}>
            <FieldLabel htmlFor="username">{t("common:fields.handle")}</FieldLabel>
            <Input
              id="username"
              type="text"
              autoComplete="username"
              autoFocus
              aria-invalid={Boolean(errors.username)}
              placeholder={t("auth:fields.handlePlaceholder")}
              disabled={submit.isPending}
              {...register("username")}
            />
            <FieldDescription>{t("auth:handleVisibilityHint")}</FieldDescription>
            {errors.username?.message ? (
              <FieldError>{t(errors.username.message)}</FieldError>
            ) : null}
          </Field>

          <Field data-invalid={Boolean(errors.email)}>
            <FieldLabel htmlFor="email">{t("common:fields.email")}</FieldLabel>
            <Input
              id="email"
              type="email"
              autoComplete="email"
              aria-invalid={Boolean(errors.email)}
              placeholder={t("auth:fields.emailPlaceholder")}
              disabled={submit.isPending}
              {...register("email")}
            />
            {errors.email?.message ? <FieldError>{t(errors.email.message)}</FieldError> : null}
          </Field>

          <Field data-invalid={Boolean(errors.password)}>
            <FieldLabel htmlFor="password">{t("common:fields.password")}</FieldLabel>
            <InputGroup>
              <InputGroupInput
                id="password"
                type={showPassword ? "text" : "password"}
                autoComplete="new-password"
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
            <PasswordStrength password={password} />
            <FieldDescription>{t("auth:password.hint")}</FieldDescription>
            {errors.password?.message ? (
              <FieldError>{t(errors.password.message)}</FieldError>
            ) : null}
          </Field>
        </FieldGroup>

        {/* Not a <label>: the sentence carries links, and wrapping them would
            make every click on "terms of service" toggle the checkbox. */}
        <div className="flex items-start gap-2.5 text-xs text-muted-foreground">
          <Checkbox
            id="terms"
            aria-labelledby="terms-label"
            checked={termsAccepted}
            onCheckedChange={(checked) => setTermsAccepted(checked === true)}
            disabled={submit.isPending}
            className="mt-px"
          />
          <span id="terms-label">
            <Trans
              t={t}
              i18nKey="auth:register.termsAgree"
              components={{
                terms: <Link to={ROUTES.terms} className="text-ember-text hover:underline" />,
                privacy: <Link to={ROUTES.privacy} className="text-ember-text hover:underline" />,
              }}
            />
          </span>
        </div>

        <TurnstileWidget siteKey={turnstile.siteKey} widgetRef={turnstile.ref} onSuccess={turnstile.setToken} />

        <Button
          type="submit"
          size="lg"
          className="w-full"
          disabled={submit.isPending || !termsAccepted || versionsPending || !legalVersions || !turnstile.isReady}
        >
          {submit.isPending ? <Spinner /> : null}
          {t("auth:register.submit")}
        </Button>
      </form>

      <p className="text-center text-sm text-muted-foreground">
        {t("auth:register.alreadyRegistered")}{" "}
        <Link to={ROUTES.login} className="font-medium text-ember-text hover:underline">
          {t("auth:register.signIn")}
        </Link>
      </p>
    </div>
  );
}
