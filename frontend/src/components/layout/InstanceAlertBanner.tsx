// src/components/layout/InstanceAlertBanner.tsx
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";

import { Banner } from "@/components/ui/banner";
import type { BannerVariant } from "@/components/ui/banner-variants";
import { useSessionDismissal } from "@/hooks/useSessionDismissal";

type AlertKind = "info" | "warning" | "danger" | "maintenance";

interface InstanceAlert {
  id: string;
  kind: AlertKind;
  message: string;
  link_url: string | null;
  starts_at: string; // ISO 8601
  ends_at: string | null;
}

const DISMISSED_KEY = "pwnforge:dismissed-alert-ids";

/** Maintenance reads as information: it is announced, not an incident. */
const KIND_VARIANT: Record<AlertKind, BannerVariant> = {
  info: "info",
  warning: "warning",
  danger: "danger",
  maintenance: "info",
};

/** Public, unauthenticated route. An empty array means no alert is active. */
async function fetchActiveAlerts(): Promise<InstanceAlert[]> {
  const res = await fetch("/api/v1/alerts/active", { credentials: "include" });

  if (!res.ok) throw new Error(`failed to fetch active alerts: ${res.status}`);

  return res.json();
}

export function InstanceAlertBanner() {
  const { t } = useTranslation();
  const { isDismissed, dismiss } = useSessionDismissal(DISMISSED_KEY);

  const { data: alerts } = useQuery({
    queryKey: ["instance-alert", "active"],
    queryFn: fetchActiveAlerts,
    refetchInterval: 5 * 60 * 1000, // 5min polling
    staleTime: 60 * 1000,
    retry: false,
  });

  // The backend returns every active alert, highest priority first. Show the
  // first one still standing: dismissing the top alert must reveal the next,
  // not bury the rest until it expires.
  const alert = alerts?.find((candidate) => !isDismissed(candidate.id));

  if (!alert) return null;

  return (
    <Banner variant={KIND_VARIANT[alert.kind]} onDismiss={() => dismiss(alert.id)} dismissLabel={t("alerts.dismiss")}>
      <span className="truncate">{alert.message}</span>
      {alert.link_url ? (
        <a href={alert.link_url} className="shrink-0 underline underline-offset-2 hover:no-underline">
          {t("alerts.readMore")}
        </a>
      ) : null}
    </Banner>
  );
}
