// src/components/setup/EmailStep.tsx
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { AlertTriangle, CheckCircle2, XCircle } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { FormError } from "@/components/common/FormError";
import { sendTestEmail } from "@/api/setup";
import type { EmailConfig } from "@/types/api";
import { testEmailSchema, type TestEmailValues } from "@/lib/schemas/setup";
import { isApiError } from "@/lib/api-error";

function ConfigRow({ label, value }: Readonly<{ label: string; value: string }>) {
  return (
    <div className="flex items-baseline justify-between gap-4 py-1 text-sm">
      <span className="text-muted-foreground">{label}</span>
      <span className="truncate font-mono text-text-subtle">{value}</span>
    </div>
  );
}

/** What the backend read from its environment, secrets excluded. */
function ConfigSummary({ config }: Readonly<{ config: EmailConfig }>) {
  const { t } = useTranslation(["setup", "common"]);

  // Built as data so the markup stays a single loop: which rows apply depends
  // on the active backend, and SMTP fields are absent on the others. Declared
  // in one expression rather than appended to, so the shape of the list is
  // readable at a glance.
  const rows: { label: string; value: string }[] = [
    { label: t("setup:email.backend"), value: config.backend },
    { label: t("setup:email.from"), value: config.from },
    { label: t("setup:email.publicUrl"), value: config.public_url },
    ...(config.smtp_host
      ? [
          {
            label: t("setup:email.smtpHost"),
            value: `${config.smtp_host}:${config.smtp_port ?? "?"}`,
          },
          {
            label: t("setup:email.smtpAuth"),
            value: config.smtp_auth ? t("common:state.active") : t("common:state.none"),
          },
        ]
      : []),
    ...(config.smtp_tls ? [{ label: t("setup:email.smtpTls"), value: config.smtp_tls }] : []),
    ...(config.resend_key_hint
      ? [{ label: t("setup:email.resendKey"), value: config.resend_key_hint }]
      : []),
  ];

  return (
    <div className="rounded-md bg-surface-2 px-3 py-2">
      {rows.map((row) => (
        <ConfigRow key={row.label} label={row.label} value={row.value} />
      ))}
    </div>
  );
}

function ConsoleWarning() {
  const { t } = useTranslation("setup");

  return (
    <div className="flex items-start gap-2 rounded-md border border-warning-soft bg-warning-soft px-3 py-2 text-sm text-warning-text">
      <AlertTriangle className="mt-px size-4 shrink-0" />
      <span>{t("email.consoleWarning")}</span>
    </div>
  );
}

function TestSuccess() {
  const { t } = useTranslation("setup");

  return (
    <div className="flex items-start gap-2 rounded-md border border-success-soft bg-success-soft px-3 py-2 text-sm text-success-text">
      <CheckCircle2 className="mt-px size-4 shrink-0" />
      <span>{t("email.testSuccess")}</span>
    </div>
  );
}

function TestFailure({ error }: Readonly<{ error: unknown }>) {
  const { t } = useTranslation("setup");
  // The backend forwards what the mail server actually said; it is the only
  // part of this screen an operator can debug with.
  const detail = isApiError(error) ? error.detail : null;

  return (
    <div className="flex items-start gap-2 rounded-md border border-danger-soft bg-danger-soft px-3 py-2 text-sm text-danger-text">
      <XCircle className="mt-px size-4 shrink-0" />
      <div className="flex min-w-0 flex-col gap-1">
        <span>{t("email.testFailure")}</span>
        {detail ? <code className="break-all font-mono text-xs opacity-80">{detail}</code> : null}
      </div>
    </div>
  );
}

function TestEmailForm({ token }: Readonly<{ token: string }>) {
  const { t } = useTranslation(["setup", "auth"]);

  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<TestEmailValues>({
    resolver: zodResolver(testEmailSchema),
    defaultValues: { to: "" },
  });

  const test = useMutation({
    mutationFn: (values: TestEmailValues) => sendTestEmail(token, values.to),
  });

  return (
    <form onSubmit={handleSubmit((values) => test.mutate(values))} className="flex flex-col gap-3">
      <Field data-invalid={Boolean(errors.to)}>
        <FieldLabel htmlFor="to">{t("setup:email.testLabel")}</FieldLabel>
        <div className="flex gap-2">
          <Input
            id="to"
            type="email"
            autoComplete="email"
            aria-invalid={Boolean(errors.to)}
            placeholder={t("auth:fields.emailPlaceholder")}
            disabled={test.isPending}
            {...register("to")}
          />
          <Button type="submit" variant="secondary" disabled={test.isPending}>
            {test.isPending ? <Spinner /> : null}
            {t("setup:email.testButton")}
          </Button>
        </div>
        {errors.to?.message ? <FieldError>{t(errors.to.message)}</FieldError> : null}
      </Field>

      {test.isSuccess ? <TestSuccess /> : null}
      {test.error ? <TestFailure error={test.error} /> : null}
    </form>
  );
}

export function EmailStep({
  token,
  config,
  submitting,
  submitError,
  onBack,
  onFinish,
}: Readonly<{
  token: string;
  config: EmailConfig;
  submitting: boolean;
  submitError: unknown;
  onBack: () => void;
  onFinish: () => void;
}>) {
  const { t } = useTranslation("setup");

  // No mail leaves the process on this backend, so there is nothing to test —
  // a "sent" here would be a lie the operator only discovers at first signup.
  const isConsole = config.backend === "console";

  return (
    <div className="flex flex-col gap-5">
      <ConfigSummary config={config} />

      {isConsole ? <ConsoleWarning /> : <TestEmailForm token={token} />}

      <p className="text-xs text-muted-foreground">{t("email.continueAnywayHint")}</p>

      <FormError error={submitError} />

      <div className="flex gap-2">
        <Button type="button" variant="secondary" size="lg" onClick={onBack} disabled={submitting}>
          {t("actions.back")}
        </Button>
        <Button type="button" size="lg" className="flex-1" onClick={onFinish} disabled={submitting}>
          {submitting ? <Spinner /> : null}
          {t("actions.finish")}
        </Button>
      </div>
    </div>
  );
}
