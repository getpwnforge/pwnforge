import { Turnstile, type TurnstileInstance } from "@marsidev/react-turnstile";
import type { RefObject } from "react";
import { useTranslation } from "react-i18next";
import { AlertCircle } from "lucide-react";
import type { TurnstileAction } from "@/hooks/useTurnstile";

interface TurnstileWidgetProps {
  siteKey: string | undefined;
  action: TurnstileAction;
  configError: boolean;
  widgetRef: RefObject<TurnstileInstance | null>;
  onSuccess: (token: string) => void;
}

export function TurnstileWidget({
  siteKey,
  action,
  configError,
  widgetRef,
  onSuccess,
}: Readonly<TurnstileWidgetProps>) {
  const { t } = useTranslation("errors");

  if (configError) {
    return (
      <div
        role="alert"
        className="flex items-start gap-2 rounded-md border border-danger-soft bg-danger-soft px-3 py-2 text-sm text-danger-text"
      >
        <AlertCircle className="mt-px size-4 shrink-0" />
        <span>{t("generic.config_unavailable")}</span>
      </div>
    );
  }

  if (!siteKey) return null;

  return (
    <Turnstile ref={widgetRef} siteKey={siteKey} options={{ action }} onSuccess={onSuccess} />
  );
}
