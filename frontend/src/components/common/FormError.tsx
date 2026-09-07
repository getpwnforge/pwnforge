// src/components/common/FormError.tsx
import { useTranslation } from "react-i18next";
import type { ReactNode } from "react";
import { AlertCircle } from "lucide-react";
import { apiErrorKey } from "@/lib/api-error";

/**
 * Renders a failed submission above the form.
 *
 * The message always comes from the backend's error code translated locally —
 * the API never returns a sentence to display, and the codes it does return are
 * deliberately vague on the auth routes (anti-enumeration).
 */
export function FormError({
  error,
  action,
}: Readonly<{ error: unknown; action?: ReactNode }>) {
  const { t } = useTranslation();

  if (!error) return null;

  return (
    <div
      role="alert"
      className="flex items-start gap-2 rounded-md border border-danger-soft bg-danger-soft px-3 py-2 text-sm text-danger-text"
    >
      <AlertCircle className="mt-px size-4 shrink-0" />
      <span>
        {t(apiErrorKey(error))}
        {action ? <span className="ml-1">{action}</span> : null}
      </span>
    </div>
  );
}
