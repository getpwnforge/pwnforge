import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Link, useSearchParams } from "react-router";

import { PageChrome } from "@/components/layout/PageChrome";
import { WorkspaceDangerZone } from "@/components/settings/WorkspaceDangerZone";
import { WorkspaceGeneralForm } from "@/components/settings/WorkspaceGeneralForm";
import { WorkspaceVisibilityForm } from "@/components/settings/WorkspaceVisibilityForm";
import { useCurrentWorkspace } from "@/hooks/useCurrentWorkspace";
import type { WorkspaceResponse } from "@/lib/api/types";
import { canArchiveWorkspace, canDeleteWorkspace, canLeaveWorkspace } from "@/lib/permissions";
import { ROUTES } from "@/lib/routes";
import { cn } from "@/lib/utils";
import { Separator } from "@/components/ui/separator";

const SECTIONS = ["general", "visibility", "danger"] as const;
type Section = (typeof SECTIONS)[number];
const isSection = (value: unknown): value is Section => SECTIONS.includes(value as Section);

/** A section that opens, or one listed ahead of its phase (the key under settings.nav). */
type Entry = { section: Section } | { soon: string };

/** The nav as on the mockup: grouped, the danger zone apart at the bottom. */
const GROUPS: readonly { key: string; entries: readonly Entry[] }[] = [
  { key: "workspace", entries: [{ section: "general" }, { section: "visibility" }, { soon: "tagsCategories" }] },
  { key: "connections", entries: [{ soon: "integrations" }, { soon: "notifications" }, { soon: "webhooks" }] },
  { key: "data", entries: [{ soon: "importExport" }] },
];

/** The key under settings.nav, and the heading of the open section. */
const NAV_KEY: Record<Section, string> = { general: "general", visibility: "visibility", danger: "dangerZone" };
const HEADING_KEY: Record<Section, string> = { general: "pageTitle", visibility: "nav.visibility", danger: "nav.dangerZone" };

const ITEM =
  "flex h-8 items-center justify-between gap-2 rounded-md px-2.5 text-[13px] font-medium text-text-muted transition-colors hover:bg-surface hover:text-text";
const GROUP_LABEL = "px-2.5 pb-1 text-[11px] font-semibold tracking-wide text-text-subtle uppercase";

/** The danger zone is listed only to whom it offers something. */
function hasDangerActions(workspace: WorkspaceResponse): boolean {
  const access = workspace.my_role;
  return canArchiveWorkspace(access) || canDeleteWorkspace(access) || canLeaveWorkspace(workspace, access);
}

/**
 * The workspace's settings, one section at a time beside a grouped list of links that stays put
 * while the section scrolls. The open section lives in `?section=` (General on the bare URL).
 */
export function WorkspaceSettingsPage() {
  const { t } = useTranslation(["workspaces"]);
  const workspace = useCurrentWorkspace();
  const [params] = useSearchParams();
  const withDanger = hasDangerActions(workspace);

  const requested = params.get("section");
  const section: Section = isSection(requested) && (requested !== "danger" || withDanger) ? requested : "general";

  const navLink = (target: Section) => (
    <Link
      key={target}
      to={{ search: target === "general" ? "" : `?section=${target}` }}
      replace
      aria-current={target === section ? "page" : undefined}
      className={cn(ITEM, target === "danger" && "text-danger hover:text-danger", target === section && "bg-surface-2 text-text")}
    >
      {t(`workspaces:settings.nav.${NAV_KEY[target]}`)}
    </Link>
  );
  const soonItem = (key: string) => (
    <span key={key} aria-disabled="true" className={cn(ITEM, "cursor-default text-text-subtle hover:bg-transparent hover:text-text-subtle")}>
      {t(`workspaces:settings.nav.${key}`)}
      <span className="text-[10.5px] font-normal">{t("workspaces:settings.soon")}</span>
    </span>
  );
  const group = (key: string, items: ReactNode) => (
    <>
      <div key={key} aria-labelledby={`ws-nav-${key}`} className="flex flex-col">
        <div id={`ws-nav-${key}`} className={GROUP_LABEL}>
          {t(`workspaces:settings.navGroups.${key}`)}
        </div>
        {items}
      </div>
      <Separator />
    </>
  );

  return (
    <>
      <PageChrome
        crumbs={[{ label: workspace.name, to: ROUTES.workspace(workspace.slug) }, { label: t("workspaces:settings.title") }]}
      />
      <div className="grid min-h-full md:grid-cols-[220px_1fr]">
        {/* The column keeps the border full height; the nav inside it sticks while the page scrolls. */}
        <aside className="border-border md:border-r">
          <nav
            aria-label={t("workspaces:settings.sectionsLabel")}
            className="flex flex-col gap-5 p-3 md:sticky md:top-(--chrome-h) md:py-6 md:pr-3 md:pl-6"
          >
            {GROUPS.map(({ key, entries }) =>
              group(key, entries.map((entry) => ("section" in entry ? navLink(entry.section) : soonItem(entry.soon)))),
            )}
            {withDanger ? group("danger", navLink("danger")) : null}
          </nav>
        </aside>

        <div className="min-w-0 max-w-settings px-6 py-6 md:px-8">
          <h1 className="mb-6 text-page font-semibold tracking-tight text-text">{t(`workspaces:settings.${HEADING_KEY[section]}`)}</h1>
          {section === "general" ? <WorkspaceGeneralForm workspace={workspace} /> : null}
          {section === "visibility" ? <WorkspaceVisibilityForm workspace={workspace} /> : null}
          {section === "danger" ? <WorkspaceDangerZone workspace={workspace} /> : null}
        </div>
      </div>
    </>
  );
}
