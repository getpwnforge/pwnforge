// src/components/layout/InstanceAlertBanner.tsx
import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "../ui/button";
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

const KIND_STYLES: Record<AlertKind, string> = {
  info: "bg-info-soft text-info-text border-info",
  warning: "bg-warning-soft text-warning-text border-warning",
  danger: "bg-danger-soft text-danger-text border-danger",
  maintenance: "bg-info-soft text-info-text border-info",
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
    <output
      className={`flex shrink-0 items-center justify-center gap-3 border-b px-4 h-(--banner-h) text-[13px] font-medium ${KIND_STYLES[alert.kind]}`}
    >
      <span className="truncate">{alert.message}</span>

      {alert.link_url && (
        <a
          href={alert.link_url}
          className="shrink-0 underline underline-offset-2 hover:no-underline"
        >
          {t("alerts.readMore")}
        </a>
      )}

      <Button
        variant="ghost"
        size="icon"
        className="shrink-0 p-1"
        onClick={() => dismiss(alert.id)}
        aria-label={t("alerts.dismiss")}
      >
        <X className="h-4 w-4" />
      </Button>
    </output>
  );
}
