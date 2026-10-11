import { Turnstile, type TurnstileInstance } from "@marsidev/react-turnstile";
import { useMemo, type RefObject } from "react";
import { useTranslation } from "react-i18next";
import { AlertCircle } from "lucide-react";
import type { TurnstileAction, TurnstileIssue } from "@/hooks/useTurnstile";

interface TurnstileWidgetProps {
  siteKey: string | undefined;
  action: TurnstileAction;
  configError: boolean;
  issue: TurnstileIssue | null;
  widgetRef: RefObject<TurnstileInstance | null>;
  onSuccess: (token: string) => void;
  onError: () => void;
  onExpire: () => void;
}

function Notice({ message }: Readonly<{ message: string }>) {
  return (
    <div
      role="alert"
      className="flex items-start gap-2 rounded-md border border-danger-soft bg-danger-soft px-3 py-2 text-sm text-danger-text"
    >
      <AlertCircle className="mt-px size-4 shrink-0" />
      <span>{message}</span>
    </div>
  );
}

export function TurnstileWidget({
  siteKey,
  action,
  configError,
  issue,
  widgetRef,
  onSuccess,
  onError,
  onExpire,
}: Readonly<TurnstileWidgetProps>) {
  const { t } = useTranslation("errors");
  // A script that cannot load (blocked domain, CSP, outage) never renders a
  // widget, so its own error callback is the only signal.
  const scriptOptions = useMemo(() => ({ onError }), [onError]);

  if (configError) {
    return <Notice message={t("generic.config_unavailable")} />;
  }

  if (!siteKey) return null;

  return (
    <>
      <Turnstile
        ref={widgetRef}
        siteKey={siteKey}
        options={{ action }}
        scriptOptions={scriptOptions}
        onSuccess={onSuccess}
        onError={onError}
        onExpire={onExpire}
      />
      {issue === "error" && <Notice message={t("turnstile.widget_error")} />}
      {issue === "expired" && <Notice message={t("turnstile.widget_expired")} />}
    </>
  );
}
