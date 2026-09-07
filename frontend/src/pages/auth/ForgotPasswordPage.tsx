import { useState } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation } from "@tanstack/react-query";
import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import { CheckCircle2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Field, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field";
import { FormError } from "@/components/common/FormError";
import { forgotPasswordSchema, type ForgotPasswordValues } from "@/lib/schemas/auth";
import { useAuth } from "@/hooks/useAuth";
import { ROUTES } from "@/lib/routes";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useTurnstile } from "@/hooks/useTurnstile";
import { TurnstileWidget } from "@/components/common/TurnstileWidget";

export function ForgotPasswordPage() {
	const { t } = useTranslation(["auth", "common"]);
	const { forgotPassword } = useAuth();
	const [requestedEmail, setRequestedEmail] = useState<string | null>(null);
  const turnstile = useTurnstile();
  usePageTitle(t("auth:forgotPassword.tabTitle"));

	const {
		register,
		handleSubmit,
		formState: { errors },
	} = useForm<ForgotPasswordValues>({
		resolver: zodResolver(forgotPasswordSchema),
		defaultValues: { email: "" },
	});

	const submit = useMutation({
		mutationFn: (values: ForgotPasswordValues) => forgotPassword({ ...values, turnstile_token: turnstile.token ?? "" }),
		onSuccess: (_data, values) => setRequestedEmail(values.email),
    onError: () => turnstile.reset(),
	});

	if (requestedEmail) {
		return (
			<div className="flex flex-col items-center gap-4 text-center">
				<div className="flex size-11 items-center justify-center rounded-full bg-success-soft text-success-text">
					<CheckCircle2 className="size-5" />
				</div>
				<div className="flex flex-col gap-1.5">
					<h1 className="text-page font-semibold tracking-hero">
						{t("auth:forgotPassword.success.title")}
					</h1>
					<p className="text-sm text-muted-foreground">
						{t("auth:forgotPassword.success.description", { email: requestedEmail })}
					</p>
				</div>
				<Button asChild size="lg" className="w-full">
					<Link to={ROUTES.login}>{t("auth:forgotPassword.success.backToLogin")}</Link>
				</Button>
			</div>
		);
	}

	return (
		<div className="flex flex-col gap-6">
			<header className="flex flex-col gap-1.5">
				<h1 className="text-page font-semibold tracking-hero">{t("auth:forgotPassword.title")}</h1>
				<p className="text-sm text-muted-foreground">{t("auth:forgotPassword.subtitle")}</p>
			</header>

			<form onSubmit={handleSubmit((values) => submit.mutate(values))} className="flex flex-col gap-5">
				<FormError error={submit.error} />

				<FieldGroup>
					<Field data-invalid={Boolean(errors.email)}>
						<FieldLabel htmlFor="email">{t("common:fields.email")}</FieldLabel>
						<Input
							id="email"
							type="email"
							autoComplete="email"
							autoFocus
							aria-invalid={Boolean(errors.email)}
							placeholder={t("auth:fields.emailPlaceholder")}
							disabled={submit.isPending}
							{...register("email")}
						/>
						{errors.email?.message ? <FieldError>{t(errors.email.message)}</FieldError> : null}
					</Field>
				</FieldGroup>

        <TurnstileWidget siteKey={turnstile.siteKey} widgetRef={turnstile.ref} onSuccess={turnstile.setToken} />

				<Button type="submit" size="lg" className="w-full" disabled={submit.isPending || !turnstile.isReady}>
					{submit.isPending ? <Spinner /> : null}
					{t("auth:forgotPassword.submit")}
				</Button>
			</form>

			<p className="text-center text-sm text-muted-foreground">
				<Link to={ROUTES.login} className="font-medium text-ember-text hover:underline">
					{t("auth:forgotPassword.backToLogin")}
				</Link>
			</p>
		</div>
	);
}
