import * as React from 'react';
import { Link } from "react-router";
import { cn } from "@/lib/utils";

type Crumb = {
  label: string;
  to?: string;
}

interface PageChromeProps {
  /** Breadcrumb segments, from root to current page. Declared by each page. */
  crumbs?: Crumb[]
  /** Contextual actions rendered on the right (buttons, status text, badges). */
  children?: React.ReactNode
}

export function PageChrome({ crumbs = [], children }: Readonly<PageChromeProps>) {
  const hasActions = React.Children.count(children) > 0;

  if (crumbs.length === 0 && !hasActions) {
    return null;
  }

  return (
    <div className="sticky top-0 z-20 flex h-(--chrome-h) shrink-0 items-center gap-3 border-b border-border bg-background px-4 sm:px-8">
      {crumbs.length > 0 && (
        <nav
          aria-label="Breadcrumb"
          className="flex min-w-0 items-center gap-1.5 text-sm text-muted-foreground"
        >
          {crumbs.map((crumb, i) => {
            const isLast = i === crumbs.length - 1;
            return (
              <React.Fragment key={`${i}-${crumb.label}`}>
                {i > 0 && (
                  <span aria-hidden className={cn("shrink-0 text-text-subtle", !isLast && "hidden sm:inline")}>
                    /
                  </span>
                )}
                {crumb.to && !isLast ? (
                  <Link
                    to={crumb.to}
                    className="hidden transition-colors hover:text-foreground sm:inline"
                  >
                    {crumb.label}
                  </Link>
                ) : (
                  <span
                    aria-current={isLast ? "page" : undefined}
                    className={cn("truncate", isLast ? "text-text" : "hidden sm:inline")}
                  >
                    {crumb.label}
                  </span>
                )}
              </React.Fragment>
            );
          })}
        </nav>
      )}

      {hasActions && (
        <div className="ml-auto flex shrink-0 items-center gap-2">{children}</div>
      )}
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
