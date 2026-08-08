import { UserAvatar } from "@/components/ui/user-avatar";
import { StatusBadge } from "@/components/ui/status-badge";
import type { StatusVariant } from "@/components/ui/status-badge-variants";
import { useTranslation } from "react-i18next";
import { formatRelativeTime } from "@/lib/format";

type ActivityItemProps = {
  activity: {
    user: {
      name: string;
    };
    action: string;
    element: string;
    tag: string;
    tagType: StatusVariant;
    timestamp: string;
  }
}

export function ActivityItem({ activity }: Readonly<ActivityItemProps>) {
  const { t, i18n } = useTranslation("activity");

  return (
    <div className="flex flex-col gap-3 p-4 rounded-md hover:bg-surface-2 w-full">
      <div className="flex flex-row items-center gap-2">
        <UserAvatar name={activity.user.name} />
        <div className="flex flex-col">
          <div className="flex flex-row gap-1 items-center">
            <span className="text-sm font-bold">{activity.user.name}</span>
            <span className="text-sm text-muted-foreground">{t(`events.${activity.action}`)}</span>
            <span className="text-sm font-bold">{activity.element}</span>
            <StatusBadge variant={activity.tagType}>
              {activity.tag}
            </StatusBadge>
          </div>
          <span className="text-xs text-muted-foreground">
            <time
              dateTime={activity.timestamp}
              title={new Intl.DateTimeFormat(i18n.language, {
                dateStyle: "long",
                timeStyle: "short",
              }).format(new Date(activity.timestamp))}
              className="text-xs text-text-subtle"
            >
              {formatRelativeTime(activity.timestamp, i18n.language)}
            </time>
          </span>
        </div>
      </div>
    </div>
  );
}
