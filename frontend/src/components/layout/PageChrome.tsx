import * as React from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import {
  Breadcrumb,
  BreadcrumbEllipsis,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";

type Crumb = {
  label: string;
  /** Where the crumb leads. Left out for a level with no page of its own. */
  to?: string;
};

interface PageChromeProps {
  /** Breadcrumb segments, from root to current page. Declared by each page. */
  crumbs?: Crumb[];
  /** Contextual actions rendered on the right (buttons, status text, badges). */
  children?: React.ReactNode;
}

/** Beyond this many crumbs, the middle ones fold into "…". */
const MAX_VISIBLE = 4;

function CrumbLink({ crumb }: Readonly<{ crumb: Crumb }>) {
  if (!crumb.to) return <span>{crumb.label}</span>;
  return (
    <BreadcrumbLink asChild>
      <Link to={crumb.to}>{crumb.label}</Link>
    </BreadcrumbLink>
  );
}

/** "…" opening the levels it hides. */
function FoldedCrumbs({ crumbs }: Readonly<{ crumbs: Crumb[] }>) {
  const { t } = useTranslation("nav");
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size="icon-sm" variant="ghost" aria-label={t("breadcrumbMore")}>
          <BreadcrumbEllipsis />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        {crumbs.map((crumb) =>
          crumb.to ? (
            <DropdownMenuItem key={crumb.to} asChild>
              <Link to={crumb.to}>{crumb.label}</Link>
            </DropdownMenuItem>
          ) : (
            <DropdownMenuItem key={crumb.label} disabled>{crumb.label}</DropdownMenuItem>
          ),
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function PageChrome({ crumbs = [], children }: Readonly<PageChromeProps>) {
  const { t } = useTranslation("nav");
  const hasActions = React.Children.count(children) > 0;
  const current = crumbs.at(-1);

  if (!current && !hasActions) return null;

  const parents = crumbs.slice(0, -1);
  const folded = crumbs.length > MAX_VISIBLE ? parents.slice(1, -1) : [];
  const shown = folded.length > 0 ? [...parents.slice(0, 1), ...parents.slice(-1)] : parents;

  return (
    <div className="sticky top-0 z-20 flex h-(--chrome-h) shrink-0 items-center gap-3 border-b border-border bg-background px-4 sm:px-8">
      {current && (
        <Breadcrumb aria-label={t("breadcrumb")} className="min-w-0">
          <BreadcrumbList className="flex-nowrap">
            {parents.length > 0 && (
              <>
                {/* Phones: every level above folds into "…". */}
                <BreadcrumbItem className="sm:hidden">
                  <FoldedCrumbs crumbs={parents} />
                </BreadcrumbItem>
                <BreadcrumbSeparator className="sm:hidden">/</BreadcrumbSeparator>
              </>
            )}
            {shown.map((crumb, i) => (
              <React.Fragment key={crumb.to ?? crumb.label}>
                <BreadcrumbItem className="hidden sm:inline-flex">
                  <CrumbLink crumb={crumb} />
                </BreadcrumbItem>
                <BreadcrumbSeparator className="hidden sm:inline-flex">/</BreadcrumbSeparator>
                {i === 0 && folded.length > 0 ? (
                  <>
                    <BreadcrumbItem className="hidden sm:inline-flex">
                      <FoldedCrumbs crumbs={folded} />
                    </BreadcrumbItem>
                    <BreadcrumbSeparator className="hidden sm:inline-flex">/</BreadcrumbSeparator>
                  </>
                ) : null}
              </React.Fragment>
            ))}
            <BreadcrumbItem className="min-w-0">
              <BreadcrumbPage className="truncate font-medium">{current.label}</BreadcrumbPage>
            </BreadcrumbItem>
          </BreadcrumbList>
        </Breadcrumb>
      )}

      {hasActions && <div className="ml-auto flex shrink-0 items-center gap-2">{children}</div>}
    </div>
  );
}

/* ---------------------------------------------------------------------------
 * FOR LATER (route-driven breadcrumbs)
 *
 * Derives the breadcrumb trail from the matched route chain instead of having
 * each page declare it. Requires:
 *   1. A data router (createBrowserRouter). useMatches() is unavailable with
 *      the <BrowserRouter> + <Routes> component API.
 *   2. The per-resource query hooks (useChallenge, useTeam, useWriteup), which
 *      land in Phase 4+. Only WorkspaceCrumb and static crumbs are wirable now.
 *
 * Migration note: `crumbs` prop and route handles can coexist. Have PageChrome
 * fall back to useBreadcrumbs() when no `crumbs` prop is passed, so pages can
 * be migrated one at a time instead of in a single sweep.
 *
 * --- Type -------------------------------------------------------------------
 *
 * // src/lib/router.tsx
 * export type CrumbHandle = {
 *   crumb: (match: UIMatch) => React.ReactNode
 * }
 *
 * --- Route declaration ------------------------------------------------------
 *
 * {
 *   path: "w/:workspaceId",
 *   handle: {
 *     crumb: (m) => <WorkspaceCrumb id={m.params.workspaceId!} />,
 *   } satisfies CrumbHandle,
 *   children: [
 *     {
 *       path: "challenges",
 *       handle: { crumb: () => <StaticCrumb k="sidebar.challenges" /> },
 *       children: [
 *         { index: true, element: <ChallengesPage /> },
 *         {
 *           path: ":challengeId",
 *           handle: {
 *             crumb: (m) => (
 *               <ChallengeCrumb
 *                 workspaceId={m.params.workspaceId!}
 *                 challengeId={m.params.challengeId!}
 *               />
 *             ),
 *           },
 *           element: <ChallengeDetailPage />,
 *         },
 *       ],
 *     },
 *   ],
 * }
 *
 * --- Hook -------------------------------------------------------------------
 *
 * function useBreadcrumbs() {
 *   return useMatches()
 *     .filter((m): m is UIMatch<unknown, CrumbHandle> =>
 *       typeof (m.handle as Partial<CrumbHandle> | undefined)?.crumb === "function",
 *     )
 *     .map((m) => ({ id: m.id, node: m.handle.crumb(m) }))
 * }
 *
 * --- Crumb components -------------------------------------------------------
 *
 * // Params are passed as props, not read via useParams: these render inside
 * // AppShell, not in a route-rendered child, so useParams would come back empty.
 * // Data comes from the regular query hooks. Same query key as the page itself,
 * // so this reads the cache rather than firing a second request.
 *
 * function WorkspaceCrumb({ id }: Readonly<{ id: string }>) {
 *   const { data: workspaces = [], isLoading } = useWorkspaces()
 *   const workspace = workspaces.find((w) => w.id === id)
 *   if (isLoading && !workspace) return <Skeleton className="h-3.5 w-24" />
 *   return <>{workspace?.name ?? id}</>
 * }
 *
 * function ChallengeCrumb({
 *   workspaceId, challengeId,
 * }: Readonly<{ workspaceId: string; challengeId: string }>) {
 *   const { data: challenge, isLoading } = useChallenge(workspaceId, challengeId)
 *   if (isLoading) return <Skeleton className="h-3.5 w-32" />
 *   return <>{challenge?.name ?? challengeId}</>
 * }
 *
 * function StaticCrumb({ k }: Readonly<{ k: string }>) {
 *   const { t } = useTranslation("nav")
 *   return <>{t(k)}</>
 * }
 *
 * --- Rendering --------------------------------------------------------------
 *
 * const crumbs = useBreadcrumbs()
 *
 * {crumbs.map((c, i) => (
 *   <React.Fragment key={c.id}>
 *     {i > 0 && <span className="text-text-subtle">/</span>}
 *     <span className={cn(i === crumbs.length - 1 && "text-text")}>{c.node}</span>
 *   </React.Fragment>
 * ))}
 *
 * ------------------------------------------------------------------------- */
