import { useMutation, useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Link, useSearchParams } from "react-router";
import { CheckCircle2, XCircle } from "lucide-react";

import { Button } from "@/components/ui/button";
import { FormError } from "@/components/common/FormError";
import { Spinner } from "@/components/ui/spinner";
import { useAuth } from "@/hooks/useAuth";
import { apiErrorKey } from "@/lib/api-error";
import { ROUTES } from "@/lib/routes";
import { usePageTitle } from "@/hooks/usePageTitle";

export function ConfirmEmailPage() {
  const { t } = useTranslation(["auth", "common"]);
  const { confirmEmail, resendVerification } = useAuth();
  const [searchParams] = useSearchParams();
  const token = searchParams.get("token");
  const resendToken = token;
  const resend = useMutation({ mutationFn: resendVerification });
  usePageTitle(t("auth:confirmEmail.title"));

  // A query, not a mutation, even though it POSTs: mail clients and link
  // scanners only ever GET, so the SPA has to turn the user's click into the
  // POST the backend requires. Doing that from a mutation fired inside a
  // manual `useEffect` never settles reliably — React's dev-mode double-effect
  // pass tears down and rebuilds the mutation observer's subscription, and the
  // eventual result lands on the stale one. Keying a query on the token gets
  // fetch-once-per-token semantics, and Strict Mode safety, for free.
  const confirm = useQuery({
    queryKey: ["email-verify", token],
    // `confirmEmail` resolves to `void`, but a query function must not resolve
    // to `undefined` — TanStack Query treats that as its own internal error
    // ("Query data cannot be undefined"), masking a successful verification
    // behind the generic error screen.
    queryFn: () => confirmEmail(token as string).then(() => true as const),
    enabled: token !== null,
    retry: false,
    staleTime: Infinity,
  });

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
      <Spinner className="size-6 text-muted-foreground" />
      <div className="flex flex-col gap-1.5">
        <h1 className="text-page font-semibold tracking-hero">
          {t("auth:confirmEmail.loading.title")}
        </h1>
        <p className="text-sm text-muted-foreground">{t("auth:confirmEmail.loading.description")}</p>
      </div>
    </div>
  );
}
