import { DownloadIcon, PlusIcon, SearchIcon } from "lucide-react";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { FormError } from "@/components/common/FormError";
import { PendingWorkspaceInvitationsCard } from "@/components/invitations/PendingInvitationsCard";
import { PageChrome } from "@/components/layout/PageChrome";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Empty, EmptyContent, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsList, TabsPanel, TabsTab } from "@/components/ui/tabs";
import { CreateWorkspaceDialog } from "@/components/workspaces/CreateWorkspaceDialog";
import { WorkspaceCard } from "@/components/workspaces/WorkspaceCard";
import { WorkspaceFiltersMenu } from "@/components/workspaces/WorkspaceFilterMenu";
import { usePageTitle } from "@/hooks/usePageTitle";
import { useWorkspaces } from "@/hooks/useWorkspaces";
import type { WorkspaceSummary } from "@/lib/api/types";
import { countFacets, filterWorkspaces, NO_FACETS, type WorkspaceFacets, type WorkspaceTab } from "@/lib/workspace-filters";

const TABS = ["all", "active", "archived"] as const satisfies readonly WorkspaceTab[];
const isTab = (value: unknown): value is WorkspaceTab => (TABS as readonly unknown[]).includes(value);

const GRID = "grid gap-3 sm:grid-cols-2 lg:grid-cols-3";
const HEADING = "mb-2 text-sm font-medium uppercase tracking-wide text-text-subtle";

type TailPlace = "favorites" | "others" | "alone";

/** Invitations and the dashed card end the last grid shown, or make their own. */
function tailPlace(favorites: number, others: number): TailPlace {
  if (others > 0) return "others";
  return favorites > 0 ? "favorites" : "alone";
}

/** Why the list is empty, or null when the tail is enough to explain it. */
function emptyKeyFor(tab: WorkspaceTab, filtering: boolean) {
  if (filtering) return "workspaces:list.noMatch" as const;
  return tab === "archived" ? ("workspaces:list.noArchived" as const) : null;
}

export function WorkspacesPage() {
  const { t } = useTranslation(["workspaces", "nav"]);
  usePageTitle(t("workspaces:list.title"));
  const workspaces = useWorkspaces();

  const [tab, setTab] = useState<WorkspaceTab>("active");
  const [query, setQuery] = useState("");
  const [facets, setFacets] = useState<WorkspaceFacets>(NO_FACETS);
  const [creating, setCreating] = useState(false);

  const visible = filterWorkspaces(workspaces.data ?? [], { tab, query, facets });
  const filtering = query.trim() !== "" || countFacets(facets) > 0;
  const emptyKey = emptyKeyFor(tab, filtering);

  const clearFilters = () => {
    setQuery("");
    setFacets(NO_FACETS);
  };

  // Invitations, then the dashed card. They are not workspaces: search and filters never hide
  // them. The archived tab shows only what was archived, so it gets no tail.
  const tail = (
    <>
      <PendingWorkspaceInvitationsCard />
      <button
        type="button"
        onClick={() => setCreating(true)}
        className="flex min-h-40 cursor-pointer flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-border text-text-muted hover:border-border-strong"
      >
        <span className="grid size-9 place-items-center rounded-md bg-surface">
          <PlusIcon aria-hidden />
        </span>
        <span className="text-sm font-medium text-text">{t("workspaces:list.newCard.title")}</span>
        <span className="text-xs">{t("workspaces:list.newCard.hint")}</span>
      </button>
    </>
  );

  return (
    <>
      <div className="flex min-w-0 flex-col">
        <PageChrome crumbs={[{ label: t("nav:sections.personalSpace") }, { label: t("workspaces:list.title") }]} />
        <div id="main">
          <div id="page" className="flex flex-col gap-6 px-6 py-8">
            <div id="page-header" className="flex flex-wrap items-end justify-between gap-4">
              <div>
                <h1 className="text-page font-semibold text-text">{t("workspaces:list.title")}</h1>
                <p className="text-text-muted">{t("workspaces:list.subtitle")}</p>
              </div>
              <div className="flex gap-2">
                <ImportMenu />
                <Button variant="primary" onClick={() => setCreating(true)}>
                  <PlusIcon aria-hidden />
                  {t("workspaces:list.newWorkspace")}
                </Button>
              </div>
            </div>

            <Tabs value={tab} onValueChange={(value) => { if (isTab(value)) setTab(value); }} className="flex flex-col gap-4">
              <div className="flex flex-wrap items-center gap-2">
                <InputGroup className="w-full max-w-sm">
                  <InputGroupAddon>
                    <SearchIcon aria-hidden />
                  </InputGroupAddon>
                  <InputGroupInput
                    type="search"
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    placeholder={t("workspaces:list.searchPlaceholder")}
                    aria-label={t("workspaces:list.searchPlaceholder")}
                  />
                </InputGroup>
                <div className="flex-1" />
                <TabsList>
                  {TABS.map((value) => (
                    <TabsTab key={value} value={value}>
                      {t(`workspaces:list.tabs.${value}`)}
                    </TabsTab>
                  ))}
                </TabsList>
                <WorkspaceFiltersMenu value={facets} onChange={setFacets} />
              </div>

              {/* One panel for the three tabs: the content is the same grid, filtered by `tab`. */}
              <TabsPanel value={tab} className="flex flex-col gap-6">
                {workspaces.isPending ? (
                  <div className={GRID}>
                    {[0, 1, 2].map((i) => <Skeleton key={i} className="h-40 rounded-lg" />)}
                  </div>
                ) : null}
                {workspaces.isError ? <FormError error={workspaces.error} /> : null}

                {workspaces.isSuccess && visible.length === 0 && emptyKey ? (
                  <Empty>
                    <EmptyHeader>
                      <EmptyTitle>{t(emptyKey)}</EmptyTitle>
                    </EmptyHeader>
                    {filtering ? (
                      <EmptyContent>
                        <Button variant="secondary" onClick={clearFilters}>
                          {t("workspaces:list.clearFilters")}
                        </Button>
                      </EmptyContent>
                    ) : null}
                  </Empty>
                ) : null}

                {workspaces.isSuccess ? (
                  <WorkspaceGrids workspaces={visible} tail={tab === "archived" || filtering ? null : tail} />
                ) : null}
              </TabsPanel>
            </Tabs>
          </div>
        </div>
      </div>
      <CreateWorkspaceDialog open={creating} onOpenChange={setCreating} />
    </>
  );
}

/** Favorites, then the others, with `tail` ending the last grid shown; null on the archived tab. */
function WorkspaceGrids({ workspaces, tail }: Readonly<{ workspaces: readonly WorkspaceSummary[]; tail: ReactNode }>) {
  const { t } = useTranslation(["workspaces"]);
  const favorites = workspaces.filter((ws) => ws.favorite);
  const others = workspaces.filter((ws) => !ws.favorite);
  const place = tailPlace(favorites.length, others.length);

  return (
    <>
      {favorites.length > 0 ? (
        <section>
          <h2 className={HEADING}>
            <span aria-hidden className="text-lg">★</span> {t("workspaces:list.favorites")}
          </h2>
          <div className={GRID}>
            {favorites.map((ws) => <WorkspaceCard key={ws.id} workspace={ws} />)}
            {place === "favorites" ? tail : null}
          </div>
        </section>
      ) : null}
      {others.length > 0 ? (
        <section>
          <h2 className={HEADING}>{t("workspaces:list.allWorkspaces")}</h2>
          <div className={GRID}>
            {others.map((ws) => <WorkspaceCard key={ws.id} workspace={ws} />)}
            {tail}
          </div>
        </section>
      ) : null}
      {place === "alone" && tail ? <div className={GRID}>{tail}</div> : null}
    </>
  );
}

/** Import entries, disabled until the import dialogs exist. */
function ImportMenu() {
  const { t } = useTranslation(["workspaces"]);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="secondary">
          <DownloadIcon aria-hidden />
          {t("workspaces:list.import")}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem disabled>
          {t("workspaces:list.importCtftime")}
          <span className="ml-auto text-xs">{t("workspaces:list.soon")}</span>
        </DropdownMenuItem>
        <DropdownMenuItem disabled>
          {t("workspaces:list.importBackup")}
          <span className="ml-auto text-xs">{t("workspaces:list.soon")}</span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
