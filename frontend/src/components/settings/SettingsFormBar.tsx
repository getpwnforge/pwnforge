import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";

type Props = Readonly<{ dirty: boolean; saving: boolean; onCancel: () => void }>;

/**
 * The bottom of every settings form: always there, its buttons idle until something changed.
 * Sticky, so a long page can be saved without scrolling back down. Must sit inside the <form>;
 * its negative bottom margin eats the page's bottom padding so it rests on the edge.
 */
export function SettingsFormBar({ dirty, saving, onCancel }: Props) {
  const { t } = useTranslation(["workspaces"]);
  return (
    <div className="sticky bottom-0 z-10 -mx-6 mt-2 -mb-6 flex items-center gap-2 border-t border-border bg-bg px-6 py-3 md:-mx-8 md:px-8">
      <p aria-live="polite" className="flex-1 text-xs text-danger">
        {dirty ? t("workspaces:settings.unsaved") : null}
      </p>
      <Button type="button" variant="ghost" disabled={!dirty || saving} onClick={onCancel}>
        {t("workspaces:settings.cancel")}
      </Button>
      <Button type="submit" variant="primary" disabled={!dirty || saving}>
        {t("workspaces:settings.save")}
      </Button>
    </div>
  );
}
