import { useTranslation } from "react-i18next";
import { formatRelativeTime } from "@/lib/format";
import { useRelativeTimeRefresh } from "@/hooks/useRelativeTimeRefresh";

type RelativeTimeProps = {
  timestamp: string;
  className?: string;
};

export function RelativeTime({ timestamp, className }: Readonly<RelativeTimeProps>) {
  const { i18n } = useTranslation();
  useRelativeTimeRefresh(timestamp);

  return (
    <time
      dateTime={timestamp}
      title={new Intl.DateTimeFormat(i18n.language, {
        dateStyle: "long",
        timeStyle: "short",
      }).format(new Date(timestamp))}
      className={className}
    >
      {formatRelativeTime(timestamp, i18n.language)}
    </time>
  );
}
