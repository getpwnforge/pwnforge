import * as React from "react";
import { useNavigate, NavLink, Link, useMatch } from "react-router";
import { useTranslation } from "react-i18next";
import {
  Home,
  Flag,
  FileText,
  ChartNoAxesColumn,
  Users,
  Settings,
  Folder,
  Folders,
  Search,
  ChevronDown,
  Check,
  Ellipsis,
  User,
  LogOut,
  CircleQuestionMark,
  // PanelLeftClose,
} from "lucide-react"
import type { LucideIcon } from "lucide-react"

import { ROUTES } from "@/lib/routes"
import { cn } from "@/lib/utils"
import { displayName } from "@/lib/display-name"

import { useWorkspaces, useWorkspaceSlug } from "@/hooks/useWorkspaces"
import { useTeams } from "@/hooks/useTeams"
import { usePendingTeamInvitations, usePendingWorkspaceInvitations } from "@/hooks/useInvitations"
import { useRecentWorkspaces } from "@/stores/recent-workspaces"
import { useCommandPalette } from "@/hooks/useCommandPalette";
import { useAuth } from "@/hooks/useAuth"
import { PendingInvitationsMenu } from "@/components/layout/PendingInvitationsMenu"

// import { useSidebarStore } from "@/stores/sidebar-store"


import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from "@/components/ui/command"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { Kbd } from "@/components/ui/kbd"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuLabel,
} from "@/components/ui/dropdown-menu"
import { Skeleton } from "@/components/ui/skeleton"
import { Logo } from "@/components/brand/Logo"
import { WsIcon } from "@/components/brand/WsIcon";
import { Tag } from "@/components/ui/tag";
import { UserAvatar } from "@/components/ui/user-avatar";
import { Separator } from "@/components/ui/separator";
import { toast } from "sonner";
import { Sidebar, SidebarContent, SidebarFooter, SidebarGroup, SidebarGroupLabel, SidebarHeader, SidebarMenu, SidebarMenuBadge, SidebarMenuButton, SidebarMenuItem, SidebarRail, SidebarTrigger } from "../ui/sidebar";
import { useSidebar } from "../ui/sidebar-context";
import type { WorkspaceSummary } from "@/lib/api/types";
import { avatarUrl } from "@/lib/avatar";


function workspaceNav(slug: string): NavItem[] {
  return [
    { key: "dashboard",  labelKey: "sidebar.dashboard",  icon: Home,              to: ROUTES.workspace(slug), end: true },
    { key: "challenges", labelKey: "sidebar.challenges", icon: Flag,              to: ROUTES.challenges(slug), badgeKey: "challenges" },
    { key: "writeups",   labelKey: "sidebar.writeups",   icon: FileText,          to: ROUTES.writeups(slug), badgeKey: "writeups" },
    { key: "stats",      labelKey: "sidebar.stats",      icon: ChartNoAxesColumn, to: ROUTES.stats(slug) },
    { key: "members",    labelKey: "sidebar.members",    icon: Users,             to: ROUTES.members(slug), badgeKey: "members" },
    { key: "settings",   labelKey: "sidebar.settings",   icon: Settings,          to: ROUTES.settings(slug) },
  ];
}

const GLOBAL_NAV: NavItem[] = [
  { key: "workspaces", labelKey: "sidebar.workspaces", icon: Folders,             to: ROUTES.workspaces, badgeKey: "workspaces" },
  { key: "teams",      labelKey: "sidebar.teams",      icon: Users,              to: ROUTES.teams, badgeKey: "teams" },
  { key: "my-stats",   labelKey: "sidebar.myStats",    icon: ChartNoAxesColumn,  to: ROUTES.myStats },
  { key: "help",       labelKey: "sidebar.help",       icon: CircleQuestionMark, to: ROUTES.help },
];

const systemTags = new Set(["web", "crypto", "pwn", "reverse", "forensics", "misc"])

// No route lists a workspace's tags yet.
const activeTags: string[] = []

type NavItem = {
  key: string
  labelKey: string
  icon: LucideIcon
  to: string
  end?: boolean
  badgeKey?: string
}

function RailTooltip({ label }: Readonly<{ label: string }>) {
  return (
    <span
      role="tooltip"
      className="pointer-events-none absolute left-full top-1/2 z-50 ml-3 -translate-y-1/2 whitespace-nowrap rounded-md border border-border bg-surface-2 px-2.5 py-1 text-xs text-text opacity-0 shadow-md transition-opacity duration-150 group-hover/rail:opacity-100"
    >
      {label}
    </span>
  )
}

function SearchBar({ collapsed }: Readonly<{ collapsed?: boolean }>) {
  const navigate = useNavigate();
  const workspaceSlug = useWorkspaceSlug();

  const items = workspaceSlug ? workspaceNav(workspaceSlug) : GLOBAL_NAV;
  // Usually already cached by the list pages and the selector: filtered locally by cmdk.
  const { data: workspaces = [] } = useWorkspaces()
  const { data: teams = [] } = useTeams()

  const { t } = useTranslation('nav')

  const open = useCommandPalette((s) => s.open)
  const setOpen = useCommandPalette((s) => s.setOpen)

  const go = (to: string) => {
    setOpen(false);
    void navigate(to);
  };

  const dialog = (
    <CommandDialog open={open} onOpenChange={setOpen}>
      <Command>
        <CommandInput placeholder={t("search.placeholder")} />
        <CommandList>
          <CommandEmpty>{t("search.noResults")}</CommandEmpty>
          <CommandGroup heading={t("search.suggestions")}>
            {items.map((item) => (
              <CommandItem key={item.key} value={t(item.labelKey)} onSelect={() => go(item.to)}>
                {t(item.labelKey)}
              </CommandItem>
            ))}
          </CommandGroup>
          {workspaces.length > 0 && (
            <CommandGroup heading={t("palette.workspaces")}>
              {workspaces.map((w) => (
                <CommandItem key={w.id} value={`${w.name} ${w.slug}`} onSelect={() => go(ROUTES.workspace(w.slug))} className="gap-2.5">
                  <WsIcon name={w.name} color={w.color} src={avatarUrl("workspaces", w.id, w.avatar_version, 64)} size="sm" />
                  <span className="truncate">{w.name}</span>
                </CommandItem>
              ))}
            </CommandGroup>
          )}
          {teams.length > 0 && (
            <CommandGroup heading={t("palette.teams")}>
              {teams.map((team) => (
                <CommandItem key={team.id} value={`${team.name} ${team.slug}`} onSelect={() => go(ROUTES.team(team.slug))} className="gap-2.5">
                  <WsIcon name={team.name} color={team.color} src={avatarUrl("teams", team.id, team.avatar_version, 64)} size="sm" />
                  <span className="truncate">{team.name}</span>
                </CommandItem>
              ))}
            </CommandGroup>
          )}
        </CommandList>
      </Command>
    </CommandDialog>
  )

  if (collapsed) {
    return (
      <>
        <button
          type="button"
          onClick={() => setOpen(true)}
          aria-label={t("search.placeholder")}
          className="group/rail relative grid size-8 shrink-0 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground"
        >
          <Search className="size-4" />
          <RailTooltip label={`${t("search.placeholder")} · ⌘K`} />
        </button>
        {dialog}
      </>
    )
  }

  return (
    <div className="relative">
      <InputGroup className="w-full">
        <InputGroupInput placeholder={t("search.placeholder")} onClick={() => setOpen(true)} />
        <InputGroupAddon>
          <Search />
        </InputGroupAddon>
        <InputGroupAddon align="inline-end">
          <Kbd className="bg-surface-2 rounded-md!">⌘ K</Kbd>
        </InputGroupAddon>
      </InputGroup>

      {dialog}
    </div>
  );
}

function WorkspaceTrigger({
  workspace,
  isLoading,
}: Readonly<{
  workspace: WorkspaceSummary | null;
  isLoading: boolean;
}>) {
  const { t } = useTranslation("nav");
  const platform = workspace?.platform ? t(`platforms.${workspace.platform}`) : null;

  if (isLoading) return <TriggerSkeleton />;

  if (!workspace) {
    return (
      <>
        <div className="grid size-7.5 shrink-0 place-items-center rounded-md bg-surface-2 text-muted-foreground">
          <Folder className="size-4" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium text-text">
            {t("sections.personalSpace")}
          </div>
          <div className="truncate text-xs text-muted-foreground">{t("noWorkspaceOpen")}</div>
        </div>
      </>
    );
  }

  return (
    <>
      <WsIcon name={workspace.name} color={workspace.color} src={avatarUrl("workspaces", workspace.id, workspace.avatar_version, 64)} />
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium text-text">{workspace.name}</div>
        <div className="truncate text-xs text-muted-foreground">
          {t(`workspaceModes.${workspace.mode}`)}
          {` · ${platform}`}
        </div>
      </div>
    </>
  );
}

function WorkspaceSelector({ collapsed }: Readonly<{ collapsed?: boolean }>) {
  const { t } = useTranslation('nav')
  const { isMobile } = useSidebar()
  const navigate = useNavigate()
  const { data: workspaces = [], isLoading } = useWorkspaces()
  const [open, setOpen] = React.useState(false)

  const currentId = useWorkspaceSlug()
  const currentWorkspace = currentId
    ? (workspaces.find((w) => w.slug === currentId) ?? null)
    : null
  const platform = currentWorkspace?.platform ? t(`platforms.${currentWorkspace.platform}`) : null


  function selectWorkspace(slug: string) {
    setOpen(false)
    void navigate(ROUTES.workspace(slug))
  }

  const commandList = (
    <Command>
      <CommandInput placeholder={t("search.placeholder")} />
      <CommandList>
        {isLoading ? (
          <div className="p-2">
            <ListSkeleton />
          </div>
        ) : (
          <>
            <CommandEmpty>{t("search.noResults")}</CommandEmpty>
            {workspaces.length > 0 && (
              <CommandGroup heading={t("search.recent")}>
                {workspaces.map((w) => (
                  <CommandItem
                    key={w.id}
                    value={`${w.name} ${w.slug}`}
                    onSelect={() => selectWorkspace(w.slug)}
                    className="gap-2.5"
                  >
                    <WsIcon name={w.name} color={w.color} src={avatarUrl("workspaces", w.id, w.avatar_version, 64)} size="sm" />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm">{w.name}</div>
                      <div className="truncate text-xs text-muted-foreground">
                        {t(`workspaceModes.${w.mode}`)}
                        {` · ${platform}`}
                      </div>
                    </div>
                    {w.slug === currentId && (
                      <Check className="ml-auto size-4 shrink-0 text-primary" />
                    )}
                  </CommandItem>
                ))}
              </CommandGroup>
            )}
            <CommandSeparator />
            <CommandGroup>
              <CommandItem
                value="all-workspaces"
                onSelect={() => {
                  setOpen(false)
                  void navigate("/workspaces")
                }}
                className="gap-2.5 text-muted-foreground"
              >
                <Folders className="size-4" />
                {t("allWorkspaces")}
              </CommandItem>
            </CommandGroup>
          </>
        )}
      </CommandList>
    </Command>
  )

  if (collapsed) {
    let triggerIcon: React.ReactNode

    if (currentId && isLoading) {
      triggerIcon = <Skeleton className="size-6 rounded-md" />
    } else if (currentWorkspace) {
      triggerIcon = <WsIcon name={currentWorkspace.name} color={currentWorkspace.color} src={avatarUrl("workspaces", currentWorkspace.id, currentWorkspace.avatar_version, 64)} size="sm" />
    } else {
      triggerIcon = <Folder className="size-4" />
    }

    return (
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <button
            type="button"
            aria-label={t("switchWorkspace")}
            className="group/rail relative grid size-8 shrink-0 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground"
          >
            {triggerIcon}
            <RailTooltip label={currentWorkspace?.name ?? t("sections.personalSpace")} />
          </button>
        </PopoverTrigger>
        <PopoverContent align="start" side={isMobile ? "bottom" : "right"} sideOffset={4} className="w-72 p-0">
          {commandList}
        </PopoverContent>
      </Popover>
    )
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={t("switchWorkspace")}
          className="flex w-full items-center gap-2.5 rounded-md border border-border-strong bg-surface p-2 text-left transition-colors hover:bg-surface-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <WorkspaceTrigger workspace={currentWorkspace} isLoading={Boolean(currentId && isLoading)} />
          <ChevronDown
            className={`size-4 shrink-0 text-text-subtle transition-transform ${open ? "rotate-180" : ""}`}
          />
        </button>
      </PopoverTrigger>

      <PopoverContent align="start" side={isMobile ? "bottom" : "right"} className="w-(--radix-popover-trigger-width) p-0">
        {commandList}
      </PopoverContent>
    </Popover>
  )
}

function TriggerSkeleton() {
  return (
    <>
      <Skeleton className="size-7.5 shrink-0 rounded-md" />
      <div className="min-w-0 flex-1 space-y-1.5">
        <Skeleton className="h-3 w-2/3" />
        <Skeleton className="h-2.5 w-1/2" />
      </div>
    </>
  )
}

function ListSkeleton() {
  return (
    <div className="space-y-1">
      {[0, 1, 2].map((i) => (
        <div key={i} className="flex items-center gap-2.5 rounded-md p-2">
          <Skeleton className="size-5.5 shrink-0 rounded" />
          <div className="flex-1 space-y-1.5">
            <Skeleton className="h-3 w-1/2" />
            <Skeleton className="h-2.5 w-1/3" />
          </div>
        </div>
      ))}
    </div>
  )
}

function SidebarNavLink({
  item, onNavigate, badge,
}: Readonly<{ item: NavItem; onNavigate?: () => void; badge?: number }>) {
  const { t } = useTranslation('nav')
  const { state } = useSidebar()
  const collapsed = state === "collapsed"
  const Icon = item.icon
  const match = useMatch({ path: item.to, end: item.end })

  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild isActive={!!match} tooltip={t(item.labelKey)}>
        <NavLink to={item.to} end={item.end} onClick={onNavigate} className="text-text-muted">
          <Icon className={cn(match && "text-ember")} />
          {!collapsed && <span className={cn(match && 'text-foreground')}>{t(item.labelKey)}</span>}
        </NavLink>
      </SidebarMenuButton>
      {badge !== undefined && badge > 0 && <SidebarMenuBadge>{badge}</SidebarMenuBadge>}
    </SidebarMenuItem>
  )
}

function SidebarNav({
  items, counts = {}, onNavigate,
}: Readonly<{ items: NavItem[]; counts?: Record<string, number>; onNavigate?: () => void }>) {
  const { t } = useTranslation('nav')
  const { data: workspaces = [] } = useWorkspaces()
  const recentSlugs = useRecentWorkspaces((s) => s.slugs)
  // In visit order, and only those the list still holds.
  const recentWorkspaces = recentSlugs.flatMap((slug) => workspaces.filter((w) => w.slug === slug))
  const { state } = useSidebar()
  const collapsed = state === "collapsed"

  return (
    <>
      <SidebarGroup>
        <SidebarGroupLabel>
          {t(items === GLOBAL_NAV ? "sections.personalSpace" : "sections.workspace")}
        </SidebarGroupLabel>
        <SidebarMenu>
          {items.map((item) => (
            <SidebarNavLink
              key={item.key}
              item={item}
              onNavigate={onNavigate}
              badge={item.badgeKey ? counts[item.badgeKey] : undefined}
            />
          ))}
        </SidebarMenu>
      </SidebarGroup>

      {items === GLOBAL_NAV ? (
        recentWorkspaces.length > 0 && <SidebarGroup>
          <SidebarGroupLabel>{t("sections.recentWorkspaces")}</SidebarGroupLabel>
          <SidebarMenu>
            {recentWorkspaces.map((w) => (
              <SidebarMenuItem key={w.id}>
                <SidebarMenuButton asChild tooltip={w.name}>
                  <NavLink to={ROUTES.workspace(w.slug)} onClick={onNavigate}>
                    <WsIcon name={w.name} color={w.color} src={avatarUrl("workspaces", w.id, w.avatar_version, 64)} size="sm" />
                    {!collapsed && <span>{w.name}</span>}
                  </NavLink>
                </SidebarMenuButton>
              </SidebarMenuItem>
            ))}
          </SidebarMenu>
        </SidebarGroup>
      ) : (
        <>
          {activeTags.length > 0 && (
            <SidebarGroup>
              <SidebarGroupLabel>{t("sections.activeTags")}</SidebarGroupLabel>
              <div className="flex flex-wrap gap-1 px-2">
                {activeTags.map((tag) => (
                  <Tag key={tag} variant={systemTags.has(tag) ? "system" : "custom"} className="text-xs">{tag}</Tag>
                ))}
              </div>
            </SidebarGroup>
          )}

          <SidebarGroup>
            <Separator className="mb-2" />
            <SidebarGroupLabel>{t("sections.navigate")}</SidebarGroupLabel>
            <SidebarMenu>
              <SidebarMenuItem>
                <SidebarMenuButton asChild tooltip={t("allWorkspaces")}>
                  <NavLink to={ROUTES.workspaces}>
                    <Folders />
                    {!collapsed && <span>{t("allWorkspaces")}</span>}
                  </NavLink>
                </SidebarMenuButton>
              </SidebarMenuItem>
              <SidebarMenuItem>
                <SidebarMenuButton asChild tooltip={t("myTeams")}>
                  <NavLink to={ROUTES.teams}>
                    <Users />
                    {!collapsed && <span>{t("myTeams")}</span>}
                  </NavLink>
                </SidebarMenuButton>
              </SidebarMenuItem>
            </SidebarMenu>
          </SidebarGroup>
        </>
      )}
    </>
  )
}

function UserMenu({collapsed}: Readonly<{collapsed?: boolean}>) {
  const [open, setOpen] = React.useState(false)
  const { t } = useTranslation('nav')
  const { user, logout } = useAuth()
  const navigate = useNavigate()

  // First and last name when set, otherwise the username; never "null null".
  const name = user ? displayName(user) : ""
  const handle = user ? `@${user.username}` : ""
  const avatar = user ? (avatarUrl("users", user.id, user.avatar_version, 64) ?? undefined) : undefined

  async function handleLogout() {
    try {
      await logout()
    } catch {
      // The local session is gone either way (the cache has been dropped),
      // so we still send the user to the sign-in screen. But the refresh token
      // may well have survived server-side, and the cookies are httpOnly so
      // the client cannot clear them itself. On a shared machine that is worth
      // saying out loud rather than swallowing.
      toast.error(t("auth:logout.failedTitle"), {
        description: t("auth:logout.failedDescription"),
        id: "logout-failed",
      })
    }

    await navigate(ROUTES.login, { replace: true })
  }

  const menuContent = (
    <DropdownMenuContent className="w-fit">
      <DropdownMenuLabel className="flex items-center gap-2.5 p-2">
        <UserAvatar name={name} src={avatar} />
        <div className="min-w-0 flex-1">
          <div className="text-base font-medium text-text">{name}</div>
          <div className="text-xs text-muted-foreground font-mono">
            {user?.email}
          </div>
        </div>
      </DropdownMenuLabel>
      <DropdownMenuSeparator />
      <DropdownMenuItem className="flex items-center gap-2.5 p-2 cursor-pointer hover:bg-surface-2" asChild>
        <Link to={ROUTES.profile}>
          <User className="size-4 shrink-0" />
          <span>{t("userMenu.profile")}</span>
        </Link>
      </DropdownMenuItem>
      <DropdownMenuItem className="flex items-center gap-2.5 p-2 cursor-pointer hover:bg-surface-2" asChild>
        <Link to={ROUTES.myStats}>
          <ChartNoAxesColumn className="size-4 shrink-0" />
          <span>{t("userMenu.stats")}</span>
        </Link>
      </DropdownMenuItem>
      <DropdownMenuItem className="flex items-center gap-2.5 p-2 cursor-pointer hover:bg-surface-2" asChild>
        <Link to={ROUTES.mySettings}>
          <Settings className="size-4 shrink-0" />
          <span>{t("userMenu.settings")}</span>
        </Link>
      </DropdownMenuItem>
      <DropdownMenuItem className="flex items-center gap-2.5 p-2 cursor-pointer hover:bg-surface-2" asChild>
        <Link to={ROUTES.help}>
          <CircleQuestionMark className="size-4 shrink-0" />
          <span>{t("userMenu.help")}</span>
        </Link>
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      {/* Not a link: signing out is a request that revokes the refresh token
          server-side, not a destination. */}
      <DropdownMenuItem
        variant="destructive"
        className="flex items-center gap-2.5 p-2 cursor-pointer hover:bg-surface-2"
        onSelect={handleLogout}
      >
        <LogOut className="size-4 shrink-0" />
        <span>{t("userMenu.logout")}</span>
      </DropdownMenuItem>
    </DropdownMenuContent>
  )

  if (collapsed) {
    return (
      <DropdownMenu open={open} onOpenChange={setOpen}>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            aria-label={t("userMenu.open")}
            className="group/rail relative grid size-11 shrink-0 place-items-center rounded-md hover:bg-surface-2"
          >
            <UserAvatar name={name} src={avatar} />
            <RailTooltip label={handle} />
          </button>
        </DropdownMenuTrigger>
        {menuContent}
      </DropdownMenu>
    )
  }

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          aria-label={t("userMenu.open")}
          className="flex items-center gap-2.5 p-2 text-left rounded-md border border-transparent hover:bg-surface-2 hover:border-border-strong focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring w-full"
        >
          <UserAvatar name={name} src={avatar} />
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-medium text-text">{name}</div>
            <div className="truncate text-xs text-muted-foreground font-mono">
              {handle}
            </div>
          </div>
          <Ellipsis className="size-4 shrink-0 text-text-subtle" />
        </button>
      </DropdownMenuTrigger>
      {menuContent}
    </DropdownMenu>
  )
}

export function AppSidebar() {
  const workspaceSlug = useWorkspaceSlug()
  const items = workspaceSlug ? workspaceNav(workspaceSlug) : GLOBAL_NAV
  const { state, isMobile, setOpenMobile } = useSidebar()
  const collapsed = state === "collapsed"
  // Mounted under RequireAuth only: an anonymous visitor never fires these requests.
  const pendingTeams = usePendingTeamInvitations().data?.length ?? 0
  const pendingWorkspaces = usePendingWorkspaceInvitations().data?.length ?? 0


  function closeOnMobile() {
    if (isMobile) setOpenMobile(false)
  }

  return (
    <Sidebar collapsible="icon">
      <SidebarHeader>
        <div className={cn("flex w-full items-center", collapsed ? "flex-col gap-2" : "justify-between gap-2.5")}>
          <Logo size="md" wordmark={!collapsed} />
          <div className={cn("flex items-center gap-1", collapsed && "flex-col")}>
            {!collapsed && <PendingInvitationsMenu />}
            <SidebarTrigger />
          </div>
        </div>
      </SidebarHeader>

      <SidebarContent>
        <SidebarGroup className="gap-4">
          <SearchBar collapsed={collapsed} />
          <WorkspaceSelector collapsed={collapsed} />
        </SidebarGroup>
        <Separator className="my-1" />
        <SidebarNav
          items={items}
          counts={{ teams: pendingTeams, workspaces: pendingWorkspaces }}
          onNavigate={closeOnMobile}
        />
      </SidebarContent>

      <SidebarFooter className={cn(collapsed && "items-center")}>
        <Separator />
        <UserMenu collapsed={collapsed} />
      </SidebarFooter>

      <SidebarRail />
    </Sidebar>
  )
}
