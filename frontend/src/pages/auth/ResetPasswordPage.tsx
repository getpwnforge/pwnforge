import { useState } from "react";
import { useForm, useWatch } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation } from "@tanstack/react-query";
import { Link, useSearchParams } from "react-router";
import { useTranslation } from "react-i18next";
import { CheckCircle2, XCircle, Eye, EyeOff } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field";
import { FormError } from "@/components/common/FormError";
import { PasswordStrength } from "@/components/auth/PasswordStrength";
import { resetPasswordSchema, type ResetPasswordValues } from "@/lib/schemas/auth";
import { useAuth } from "@/hooks/useAuth";
import { ROUTES } from "@/lib/routes";
import { InputGroup, InputGroupButton, InputGroupInput } from "@/components/ui/input-group";
import { usePageTitle } from "@/hooks/usePageTitle";

export function ResetPasswordPage() {
	const { t } = useTranslation(["auth", "common"]);
	const { resetPassword } = useAuth();
	const [searchParams] = useSearchParams();
	const token = searchParams.get("token");
	const [isReset, setIsReset] = useState(false);
  const [showPassword, setShowPassword] = useState(false);
  usePageTitle(t("auth:resetPassword.tabTitle"));

	const {
		register,
		handleSubmit,
		control,
		formState: { errors },
	} = useForm<ResetPasswordValues>({
		resolver: zodResolver(resetPasswordSchema),
		defaultValues: { password: "", confirmPassword: "" },
	});
	const password = useWatch({ control, name: "password" });

	const submit = useMutation({
		mutationFn: (values: ResetPasswordValues) =>
			resetPassword({ token: token as string, new_password: values.password }),
		onSuccess: () => setIsReset(true),
	});

	if (!token) {
		return (
			<div className="flex flex-col items-center gap-4 text-center">
				<div className="flex size-11 items-center justify-center rounded-full bg-danger-soft text-danger-text">
					<XCircle className="size-5" />
				</div>
				<div className="flex flex-col gap-1.5">
					<h1 className="text-page font-semibold tracking-hero">{t("auth:resetPassword.error.title")}</h1>
					<p className="text-sm text-muted-foreground">{t("auth:resetPassword.error.missingToken")}</p>
				</div>
				<Button asChild variant="secondary" size="lg" className="w-full">
					<Link to={ROUTES.login}>{t("auth:resetPassword.error.backToLogin")}</Link>
				</Button>
			</div>
		);
	}

	if (isReset) {
		return (
			<div className="flex flex-col items-center gap-4 text-center">
				<div className="flex size-11 items-center justify-center rounded-full bg-success-soft text-success-text">
					<CheckCircle2 className="size-5" />
				</div>
				<div className="flex flex-col gap-1.5">
					<h1 className="text-page font-semibold tracking-hero">{t("auth:resetPassword.success.title")}</h1>
					<p className="text-sm text-muted-foreground">{t("auth:resetPassword.success.description")}</p>
				</div>
				<Button asChild size="lg" className="w-full">
					<Link to={ROUTES.login}>{t("auth:resetPassword.success.backToLogin")}</Link>
				</Button>
			</div>
		);
	}

	return (
		<div className="flex flex-col gap-6">
			<header className="flex flex-col gap-1.5">
				<h1 className="text-page font-semibold tracking-hero">{t("auth:resetPassword.title")}</h1>
				<p className="text-sm text-muted-foreground">{t("auth:resetPassword.subtitle")}</p>
			</header>

			<form onSubmit={handleSubmit((values) => submit.mutate(values))} className="flex flex-col gap-5">
				<FormError error={submit.error} />

				<FieldGroup>
					<Field data-invalid={Boolean(errors.password)}>
						<FieldLabel htmlFor="password">{t("auth:resetPassword.newPassword")}</FieldLabel>
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
						<PasswordStrength password={password} />
						<FieldDescription>{t("auth:password.hint")}</FieldDescription>
						{errors.password?.message ? <FieldError>{t(errors.password.message)}</FieldError> : null}
					</Field>

					<Field data-invalid={Boolean(errors.confirmPassword)}>
						<FieldLabel htmlFor="confirmPassword">{t("auth:resetPassword.confirmPassword")}</FieldLabel>
						<Input
							id="confirmPassword"
							type="password"
							autoComplete="new-password"
							aria-invalid={Boolean(errors.confirmPassword)}
							placeholder={t("auth:fields.passwordPlaceholder")}
							disabled={submit.isPending}
							{...register("confirmPassword")}
						/>
						{errors.confirmPassword?.message ? (
							<FieldError>{t(errors.confirmPassword.message)}</FieldError>
						) : null}
					</Field>
				</FieldGroup>

				<Button type="submit" size="lg" className="w-full" disabled={submit.isPending}>
					{submit.isPending ? <Spinner /> : null}
					{t("auth:resetPassword.submit")}
				</Button>
			</form>
		</div>
	);
}
