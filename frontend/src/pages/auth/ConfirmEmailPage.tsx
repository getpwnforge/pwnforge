import { useMutation } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { CheckCircle2, MailCheck, XCircle } from "lucide-react";

import { Button } from "@/components/ui/button";
import { FormError } from "@/components/common/FormError";
import { Spinner } from "@/components/ui/spinner";
import { useAuth } from "@/hooks/useAuth";
import { apiErrorKey } from "@/lib/api-error";
import { ROUTES } from "@/lib/routes";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useLinkToken } from "@/hooks/useLinkToken";

export function ConfirmEmailPage() {
  const { t } = useTranslation(["auth", "common"]);
  const { confirmEmail, resendVerification } = useAuth();
  const token = useLinkToken();
  const resendToken = token;
  const resend = useMutation({ mutationFn: resendVerification });
  usePageTitle(t("auth:confirmEmail.title"));

  // Sent on the button, never on load. A link scanner that runs the page's
  // script, as some mail gateways do, would otherwise verify the address
  // without its owner ever choosing to: and an account registered with
  // someone else's address would become a verified one, receiving whatever
  // is sent to that address. A mutation fired from a click has none of the
  // Strict Mode trouble an effect had.
  const confirm = useMutation({ mutationFn: (value: string) => confirmEmail(value) });

  if (!token || confirm.isError) {
    return (
      <div className="flex flex-col items-center gap-4 text-center">
        <div className="flex size-11 items-center justify-center rounded-full bg-danger-soft text-danger-text">
          <XCircle className="size-5" />
        </div>
        <div className="flex flex-col gap-1.5">
          <h1 className="text-page font-semibold tracking-hero">
            {t("auth:confirmEmail.error.title")}
          </h1>
          <p className="text-sm text-muted-foreground">
            {token ? t(apiErrorKey(confirm.error)) : t("auth:confirmEmail.error.missingToken")}
          </p>
          <p className="text-sm text-muted-foreground">
            {t("auth:confirmEmail.error.needNewToken")}
            {resendToken ? (
              <Button
                type="button"
                variant="link"
                className="h-auto p-0 text-sm text-ember-text"
                disabled={resend.isPending}
                onClick={() => resend.mutate({ token: resendToken })}
              >
                {resend.isPending ? <Spinner /> : null}
                {resend.isSuccess
                  ? t("auth:confirmEmail.error.sent")
                  : t("auth:confirmEmail.error.resend")}
              </Button>
            ) : null}
          </p>
          {resend.isError ? <FormError error={resend.error} /> : null}
        </div>
        <Button asChild variant="secondary" size="lg" className="w-full">
          <Link to={ROUTES.login}>{t("auth:confirmEmail.error.backToLogin")}</Link>
        </Button>
      </div>
    );
  }

  if (confirm.isSuccess) {
    return (
      <div className="flex flex-col items-center gap-4 text-center">
        <div className="flex size-11 items-center justify-center rounded-full bg-success-soft text-success-text">
          <CheckCircle2 className="size-5" />
        </div>
        <div className="flex flex-col gap-1.5">
          <h1 className="text-page font-semibold tracking-hero">
            {t("auth:confirmEmail.success.title")}
          </h1>
          <p className="text-sm text-muted-foreground">
            {t("auth:confirmEmail.success.description")}
          </p>
        </div>
        <Button asChild size="lg" className="w-full">
          <Link to={ROUTES.login}>{t("auth:confirmEmail.success.backToLogin")}</Link>
        </Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col items-center gap-4 text-center">
      <div className="flex size-11 items-center justify-center rounded-full bg-muted text-muted-foreground">
        <MailCheck className="size-5" />
      </div>
      <div className="flex flex-col gap-1.5">
        <h1 className="text-page font-semibold tracking-hero">
          {t("auth:confirmEmail.prompt.title")}
        </h1>
        <p className="text-sm text-muted-foreground">{t("auth:confirmEmail.prompt.description")}</p>
      </div>
      <Button
        type="button"
        size="lg"
        className="w-full"
        disabled={confirm.isPending}
        onClick={() => confirm.mutate(token)}
      >
        {confirm.isPending ? <Spinner /> : null}
        {confirm.isPending
          ? t("auth:confirmEmail.loading.title")
          : t("auth:confirmEmail.prompt.confirm")}
      </Button>
    </div>
  );
}
