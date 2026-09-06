import { useState } from "react";
import { useNavigate } from "react-router";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Check } from "lucide-react";
import { toast } from "sonner";

import { Logo } from "@/components/brand/Logo";
import { cn } from "@/lib/utils";
import { ROUTES } from "@/lib/routes";
import { SETUP_STATUS_QUERY_KEY, completeSetup } from "@/api/setup";
import type { EmailConfig } from "@/types/api";
import { AdminStep } from "@/components/setup/AdminStep";
import { EmailStep } from "@/components/setup/EmailStep";
import { SettingsStep } from "@/components/setup/SettingsStep";
import { TokenStep } from "@/components/setup/TokenStep";
import type { AdminValues, SettingsValues } from "@/lib/schemas/setup";

const STEPS = ["token", "admin", "settings", "email"] as const;

type StepId = (typeof STEPS)[number];

function Stepper({ current }: Readonly<{ current: StepId }>) {
  const { t } = useTranslation("setup");
  const currentIndex = STEPS.indexOf(current);

  return (
    <ol className="flex items-center gap-2">
      {STEPS.map((step, index) => {
        const done = index < currentIndex;
        const active = index === currentIndex;

        return (
          <li key={step} className="flex flex-1 flex-col gap-1.5">
            <span
              aria-hidden
              className={cn(
                "h-0.5 rounded-full transition-colors",
                done || active ? "bg-ember" : "bg-surface-2",
              )}
            />
            <span
              className={cn(
                "flex items-center gap-1 text-xs",
                active ? "font-medium text-foreground" : "text-text-subtle",
              )}
              aria-current={active ? "step" : undefined}
            >
              {done ? <Check className="size-3 text-ember-text" /> : null}
              {t(`steps.${step}.label`)}
            </span>
          </li>
        );
      })}
    </ol>
  );
}

export function SetupPage() {
  const { t } = useTranslation("setup");
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  const [step, setStep] = useState<StepId>("token");
  const [token, setToken] = useState<string | null>(null);
  const [emailConfig, setEmailConfig] = useState<EmailConfig | null>(null);
  const [admin, setAdmin] = useState<AdminValues | null>(null);
  const [settings, setSettings] = useState<SettingsValues | null>(null);

  const complete = useMutation({
    mutationFn: (input: { token: string; admin: AdminValues; settings: SettingsValues }) =>
      completeSetup(input.token, { ...input.admin, ...input.settings }),
    onSuccess: () => {
      // The gate reads this key; updating it here is what reopens the rest of
      // the app without a reload.
      queryClient.setQueryData(SETUP_STATUS_QUERY_KEY, { completed: true });

      // The redirect below drops the operator on a sign-in form with nothing
      // to say the wizard succeeded. The toast is the only confirmation the
      // administrator was created.
      toast.success(t("completed.title"), { description: t("completed.description") });

      // No session is opened on purpose: the administrator signs in with the
      // credentials they just chose, which also proves they typed what they meant.
      navigate(ROUTES.login, { replace: true });
    },
  });

  function handleFinish() {
    if (!token || !admin || !settings) return;
    complete.mutate({ token, admin, settings });
  }

  return (
    <div className="flex min-h-dvh items-center justify-center px-6 py-10">
      <div className="flex w-full max-w-settings flex-col gap-8">
        <div className="flex flex-col items-center gap-4 text-center">
          <Logo size="lg" type="noBg" />
          <div className="flex flex-col gap-1.5">
            <h1 className="text-page font-semibold tracking-hero">{t("title")}</h1>
            <p className="text-sm text-muted-foreground">{t("subtitle")}</p>
          </div>
        </div>

        <Stepper current={step} />

        <div className="flex flex-col gap-5 rounded-lg bg-bg-elev p-6 ring-1 ring-foreground/10">
          <div className="flex flex-col gap-1">
            <h2 className="text-h2 font-semibold">{t(`steps.${step}.title`)}</h2>
            <p className="text-sm text-muted-foreground">{t(`steps.${step}.description`)}</p>
          </div>

          {step === "token" ? (
            <TokenStep
              onValidated={(validToken, config) => {
                setToken(validToken);
                setEmailConfig(config);
                setStep("admin");
              }}
            />
          ) : null}

          {step === "admin" && token ? (
            <AdminStep
              token={token}
              defaultValues={admin}
              onBack={() => setStep("token")}
              onNext={(values) => {
                setAdmin(values);
                setStep("settings");
              }}
            />
          ) : null}

          {step === "settings" ? (
            <SettingsStep
              defaultValues={settings}
              onBack={() => setStep("admin")}
              onNext={(values) => {
                setSettings(values);
                setStep("email");
              }}
            />
          ) : null}

          {step === "email" && token && emailConfig ? (
            <EmailStep
              token={token}
              config={emailConfig}
              submitting={complete.isPending}
              submitError={complete.error}
              onBack={() => setStep("settings")}
              onFinish={handleFinish}
            />
          ) : null}
        </div>
      </div>
    </div>
  );
}
