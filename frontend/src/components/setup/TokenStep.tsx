// src/components/setup/TokenStep.tsx
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { AlertCircle, Terminal } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { fetchEmailConfig } from "@/api/setup";
import type { EmailConfig } from "@/types/api";
import { tokenSchema, type TokenValues } from "@/lib/schemas/setup";
import { apiErrorKey, isApiError } from "@/lib/api-error";

export function TokenStep({
  onValidated,
}: Readonly<{
  onValidated: (token: string, config: EmailConfig) => void;
}>) {
  const { t } = useTranslation(["setup", "common"]);

  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<TokenValues>({
    resolver: zodResolver(tokenSchema),
    defaultValues: { token: "" },
  });

  // There is no dedicated "check this token" route: fetching the email config
  // is the cheapest authenticated call, and its 404 is the answer.
  const { mutate, isPending, error } = useMutation({
    mutationFn: (values: TokenValues) =>
      fetchEmailConfig(values.token).then((config) => ({ token: values.token, config })),
    onSuccess: ({ token, config }) => onValidated(token, config),
  });

  // The backend answers 404 on a bad token — deliberately, so the setup
  // surface stays unadvertised. Translated as "wrong token" here because on
  // this screen that is the only thing it can mean.
  let errorMessage: string | null = null;

  if (error) {
    if (isApiError(error) && error.status === 404) {
      errorMessage = t("setup:token.invalid");
    } else {
      errorMessage = t(apiErrorKey(error));
    }
  }

  return (
    <form onSubmit={handleSubmit((values) => mutate(values))} className="flex flex-col gap-5">
      {errorMessage ? (
        <div
          role="alert"
          className="flex items-start gap-2 rounded-md border border-danger-soft bg-danger-soft px-3 py-2 text-sm text-danger-text"
        >
          <AlertCircle className="mt-px size-4 shrink-0" />
          <span>{errorMessage}</span>
        </div>
      ) : null}

      <div className="flex gap-3 rounded-md bg-surface-2 px-3 py-2.5 text-xs text-muted-foreground">
        <Terminal className="mt-px size-4 shrink-0" />
        <div className="flex flex-col gap-1">
          <span>{t("setup:token.whereFrom")}</span>
          <code className="font-mono text-text-subtle">{t("setup:token.logLine")}</code>
          <span>{t("setup:token.envAlternative")}</span>
        </div>
      </div>

      <Field data-invalid={Boolean(errors.token)}>
        <FieldLabel htmlFor="token">{t("setup:token.label")}</FieldLabel>
        <Input
          id="token"
          type="text"
          autoComplete="off"
          spellCheck={false}
          autoFocus
          className="font-mono"
          aria-invalid={Boolean(errors.token)}
          disabled={isPending}
          {...register("token")}
        />
        <FieldDescription>{t("setup:token.hint")}</FieldDescription>
        {errors.token?.message ? <FieldError>{t(errors.token.message)}</FieldError> : null}
      </Field>

      <Button type="submit" size="lg" className="w-full" disabled={isPending}>
        {isPending ? <Spinner /> : null}
        {t("common:actions.next")}
      </Button>
    </form>
  );
}
