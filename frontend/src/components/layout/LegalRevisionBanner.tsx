// src/components/layout/LegalRevisionBanner.tsx
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";

import { Button } from "../ui/button";
import { Spinner } from "../ui/spinner";
import { acceptLegal, LEGAL_STATUS_QUERY_KEY, useLegalStatus } from "@/api/legal";
import { useSessionDismissal } from "@/hooks/useSessionDismissal";
import { useAuth } from "@/hooks/useAuth";
import { ROUTES } from "@/lib/routes";

const DISMISSED_KEY = "pwnforge:dismissed-legal-versions";

/**
 * Announces a published revision during its notice period.
 *
 * Only renders in the `pending` state. Past the notice period continued use
 * counts as acceptance under article 23 of the terms, which the backend
 * records on the next login or refresh, so there is nothing left to ask.
 */
export function LegalRevisionBanner() {
  const { t, i18n } = useTranslation();
  const { status: authStatus } = useAuth();
  const queryClient = useQueryClient();
  const { isDismissed, dismiss } = useSessionDismissal(DISMISSED_KEY);

  // Unlike /alerts/active this route is authenticated: querying it while
  // signed out or still loading would answer 401 and trip the refresh
  // interceptor for nothing.
  const { data: legal } = useLegalStatus(authStatus === "authenticated");

  const accept = useMutation({
    mutationFn: acceptLegal,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: LEGAL_STATUS_QUERY_KEY }),
  });

  if (legal?.state !== "pending") return null;

  // Keyed on the version pair rather than a bare flag: a second revision must
  // surface even though the first was closed earlier in this session.
  const versionKey = `${legal.current_terms_version}/${legal.current_privacy_version}`;
  if (isDismissed(versionKey)) return null;

  const effectiveAt = legal.effective_at
    ? new Date(legal.effective_at).toLocaleDateString(i18n.language, {
        year: "numeric",
        month: "long",
        day: "numeric",
      })
    : null;

  // The terms carry the tacit acceptance clause, so they are the document to
  // point at when both changed.
  const target =
    legal.accepted_terms_version === legal.current_terms_version
      ? ROUTES.privacy
      : ROUTES.terms;

  return (
    <output className="flex shrink-0 items-center justify-center gap-3 border-b border-info bg-info-soft px-4 h-(--banner-h) text-[13px] font-medium text-info-text">
      <span className="truncate">
        {effectiveAt
          ? t("legal.revisionPending", { date: effectiveAt })
          : t("legal.revisionPendingNoDate")}
      </span>

      <Link to={target} className="shrink-0 underline underline-offset-2 hover:no-underline">
        {t("legal.read")}
      </Link>

      <Button
        variant="ghost"
        size="sm"
        className="shrink-0"
        disabled={accept.isPending}
        onClick={() =>
          accept.mutate({
            // Echo back what the server just told us is current, so this can
            // never accept a version the user was not shown.
            terms_version: legal.current_terms_version,
            privacy_version: legal.current_privacy_version,
          })
        }
      >
        {accept.isPending ? <Spinner /> : null}
        {t("legal.acceptNow")}
      </Button>

      <Button
        variant="ghost"
        size="icon"
        className="shrink-0 p-1"
        onClick={() => dismiss(versionKey)}
        aria-label={t("legal.dismiss")}
      >
        <X className="h-4 w-4" />
      </Button>
    </output>
  );
}
