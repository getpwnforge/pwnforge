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
  CircleQuestionMark,
  Bell,
  Search,
  ChevronDown,
  Check,
  Ellipsis,
  User,
  LogOut,
  // PanelLeftClose,
  RefreshCcwIcon,
} from "lucide-react"
import type { LucideIcon } from "lucide-react"

import { ROUTES } from "@/lib/routes"
import { cn } from "@/lib/utils"

import { useWorkspaces, useWorkspaceId } from "@/hooks/useWorkspaces"
import {
  useNotifications,
  useMarkNotificationRead,
  useMarkAllNotificationsRead,
  type Notification,
} from "@/hooks/useNotifications"
import { useCommandPalette } from "@/hooks/useCommandPalette";
import { useAuth } from "@/hooks/useAuth"

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
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";
import { Sidebar, SidebarContent, SidebarFooter, SidebarGroup, SidebarGroupLabel, SidebarHeader, SidebarMenu, SidebarMenuBadge, SidebarMenuButton, SidebarMenuItem, SidebarRail, SidebarTrigger } from "../ui/sidebar";
import { useSidebar } from "../ui/sidebar-context";
import type { WsColor } from "@/lib/ws-color";


function workspaceNav(id: string): NavItem[] {
  return [
    { key: "dashboard",  labelKey: "sidebar.dashboard",  icon: Home,              to: ROUTES.workspace(id), end: true },
    { key: "challenges", labelKey: "sidebar.challenges", icon: Flag,              to: ROUTES.challenges(id), badgeKey: "challenges" },
    { key: "writeups",   labelKey: "sidebar.writeups",   icon: FileText,          to: ROUTES.writeups(id), badgeKey: "writeups" },
    { key: "stats",      labelKey: "sidebar.stats",      icon: ChartNoAxesColumn, to: ROUTES.stats(id) },
    { key: "members",    labelKey: "sidebar.members",    icon: Users,             to: ROUTES.members(id), badgeKey: "members" },
    { key: "settings",   labelKey: "sidebar.settings",   icon: Settings,          to: ROUTES.settings(id) },
  ];
}

const GLOBAL_NAV: NavItem[] = [
  { key: "workspaces", labelKey: "sidebar.workspaces", icon: Folders,             to: ROUTES.workspaces },
  { key: "teams",      labelKey: "sidebar.teams",      icon: Users,              to: ROUTES.teams, badgeKey: "teams" },
  { key: "my-stats",   labelKey: "sidebar.myStats",    icon: ChartNoAxesColumn,  to: ROUTES.myStats },
  { key: "help",       labelKey: "sidebar.help",       icon: CircleQuestionMark, to: ROUTES.help },
];

const systemTags = new Set(["web", "crypto", "pwn", "reverse", "forensics", "misc"])

type NavItem = {
  key: string
  labelKey: string
  icon: LucideIcon
  to: string
  end?: boolean
  badgeKey?: string
}

// Locale-aware relative time without i18n key sprawl.
function relativeTime(iso: string, locale: string) {
  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: "auto" })
  const min = Math.round((new Date(iso).getTime() - Date.now()) / 60000)
  if (Math.abs(min) < 60) return rtf.format(min, "minute")
  const h = Math.round(min / 60)
  if (Math.abs(h) < 24) return rtf.format(h, "hour")
  return rtf.format(Math.round(h / 24), "day")
}

function groupByRecency(items: Notification[]) {
  const startOfToday = new Date()
  startOfToday.setHours(0, 0, 0, 0)
  const weekAgo = new Date(startOfToday)
  weekAgo.setDate(weekAgo.getDate() - 7)

  const buckets = { today: [] as Notification[], week: [] as Notification[], earlier: [] as Notification[] }
  for (const n of [...items].sort((a, b) => b.createdAt.localeCompare(a.createdAt))) {
    const d = new Date(n.createdAt)
    if (d >= startOfToday) buckets.today.push(n)
    else if (d >= weekAgo) buckets.week.push(n)
    else buckets.earlier.push(n)
  }
  return [
    { key: "today", items: buckets.today },
    { key: "week", items: buckets.week },
    { key: "earlier", items: buckets.earlier },
  ].filter((g) => g.items.length > 0)
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

export function NotificationsMenu() {
  const { t, i18n } = useTranslation(['nav', 'common'])

  const [open, setOpen] = React.useState(false)
  const { data: notifications = [], isLoading } = useNotifications()
  const markRead = useMarkNotificationRead()
  const markAll = useMarkAllNotificationsRead()

  const unread = notifications.filter((n) => !n.read).length
  const groups = groupByRecency(notifications)

  let content: React.ReactNode

  if (isLoading) {
    content = <NotificationsSkeleton />
  } else if (notifications.length === 0) {
    content = (
      <Empty className="h-full bg-muted/30">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <Bell />
          </EmptyMedia>
          <EmptyTitle>{t("nav:notifications.noNew")}</EmptyTitle>
          <EmptyDescription className="max-w-xs text-pretty">
            {t("nav:notifications.noNewDesc")}
          </EmptyDescription>
        </EmptyHeader>
        <EmptyContent>
          <Button variant="primary">
            <RefreshCcwIcon />
            Refresh
          </Button>
        </EmptyContent>
      </Empty>
    )
  } else {
    content = groups.map((g) => (
      <section key={g.key}>
        <div className="px-3.5 pb-1 pt-2 text-nav uppercase tracking-nav text-text-subtle">
          {t(`common:time.${g.key}`)}
        </div>
        {g.items.map((n) => (
          <NotificationRow
            key={n.id}
            n={n}
            locale={i18n.language}
            onOpen={() => {
              if (!n.read) markRead.mutate(n.id)
              setOpen(false)
              // navigate(targetFor(n)) // once notification targets are wired
            }}
          />
        ))}
      </section>
    ))
}

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={t("nav:notifications.title")}
          className="rounded-md p-1 hover:bg-surface-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <NotificationBell count={unread} />
        </button>
      </PopoverTrigger>

      <PopoverContent align="end" className="w-96 p-0">
        <header className="flex items-center justify-between border-b border-border px-3.5 py-3">
          <div className="flex items-center gap-2">
            <span className="text-sm font-semibold text-text">{t("nav:notifications.title")}</span>
            {unread > 0 && (
              <span className="rounded-full bg-ember-soft px-1.5 py-px font-mono text-nav font-semibold text-primary">
                {unread}
              </span>
            )}
          </div>
          <button
            type="button"
            onClick={() => markAll.mutate()}
            disabled={unread === 0}
            className="text-xs text-muted-foreground hover:text-text disabled:opacity-40"
          >
            {t("nav:notifications.markAllRead")}
          </button>
        </header>

        <div className="max-h-96 overflow-y-auto">
          {content}
        </div>

        <div className="border-t border-border p-2.5 text-center">
          <button type="button" className="text-sm font-medium text-primary hover:underline">
            {t("nav:notifications.viewAll")}
          </button>
        </div>
      </PopoverContent>
    </Popover>
  )
}

function NotificationRow({
  n, locale, onOpen,
}: Readonly<{ n: Notification; locale: string; onOpen: () => void }>) {
  return (
    <button
      type="button"
      onClick={onOpen}
      className={cn(
        "flex w-full gap-2.5 px-3.5 py-2.5 text-left transition-colors hover:bg-surface-2",
        !n.read && "bg-ember-soft/20",
      )}
    >
      <span className="flex w-2 shrink-0 justify-center pt-1.5">
        {!n.read && <span className="size-1.75 rounded-full bg-primary" />}
      </span>

      {n.source.kind === "workspace" ? (
        <WsIcon name={n.source.name} color={n.source.color} size="sm" />
      ) : (
        <UserAvatar name={n.source.name} size="sm" className="shrink-0" />
      )}

      <span className="min-w-0 flex-1">
        <span className={cn("block text-sm leading-snug", n.read ? "text-muted-foreground" : "text-text")}>
          {n.actor && <b className="font-semibold">{n.actor} </b>}
          {n.text}{" "}
          {n.highlight && (
            <b className={cn("font-semibold", n.read ? "text-text-muted" : "text-primary")}>
              {n.highlight}
            </b>
          )}
        </span>
        <span className="mt-0.5 block text-xs text-text-subtle">
          {n.context} · {relativeTime(n.createdAt, locale)}
        </span>
      </span>
    </button>
  )
}

function NotificationsSkeleton() {
  return (
    <div className="space-y-1 p-2">
      {[0, 1, 2].map((i) => (
        <div key={i} className="flex gap-2.5 px-2 py-2.5">
          <Skeleton className="size-6 shrink-0 rounded-md" />
          <div className="flex-1 space-y-1.5">
            <Skeleton className="h-3 w-3/4" />
            <Skeleton className="h-2.5 w-1/2" />
          </div>
        </div>
      ))}
    </div>
  )
}

function NotificationBell({ count }: Readonly<{ count: number }>) {
  return (
    <div className="relative">
      <Bell className="size-5 text-muted-foreground hover:text-foreground" />
      {count > 0 && (
        <span className="absolute -top-1 -right-1 flex size-4 items-center justify-center rounded-full bg-danger text-[10px] font-semibold leading-none text-danger-contrast">
          {count > 9 ? "9+" : count}
        </span>
      )}
    </div>
  )
}

function SearchBar({ collapsed }: Readonly<{ collapsed?: boolean }>) {
  const navigate = useNavigate();
  const workspaceId = useWorkspaceId();

  const items = workspaceId ? workspaceNav(workspaceId) : GLOBAL_NAV;

  const { t } = useTranslation('nav')

  const open = useCommandPalette((s) => s.open)
  const setOpen = useCommandPalette((s) => s.setOpen)

  const go = (to: string) => {
    setOpen(false);
    navigate(to);
  };

  const dialog = (
    <CommandDialog open={open} onOpenChange={setOpen}>
      <Command>
        <CommandInput placeholder={t("search.placeholder")} />
        <CommandList>
          <CommandEmpty>{t("search.noResults")}</CommandEmpty>
          <CommandGroup heading="Suggestions">
            {items.map((item) => (
              <CommandItem key={item.key} value={t(item.labelKey)} onSelect={() => go(item.to)}>
                {t(item.labelKey)}
              </CommandItem>
            ))}
          </CommandGroup>
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
  workspace: { name: string; type: string; platform?: string; color?: WsColor } | null;
  isLoading: boolean;
}>) {
  const { t } = useTranslation("nav");

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
      <WsIcon name={workspace.name} color={workspace.color} />
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium text-text">{workspace.name}</div>
        <div className="truncate text-xs text-muted-foreground">
          {t(`workspacesTypes.${workspace.type}`)} · {workspace.platform}
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

  const currentId = useWorkspaceId()
  const currentWorkspace = currentId
    ? (workspaces.find((w: { id: string }) => w.id === currentId) ?? null)
    : null


  function selectWorkspace(id: string) {
    setOpen(false)
    navigate(`/w/${id}`)
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
                {workspaces.map((w: { id: string; name: string; platform?: string; type: string; color?: "ember" | "blue" | "violet" | "teal" | "rose" | "amber" }) => (
                  <CommandItem
                    key={w.id}
                    value={`${w.name} ${w.platform} ${w.type}`}
                    onSelect={() => selectWorkspace(w.id)}
                    className="gap-2.5"
                  >
                    <WsIcon name={w.name} color={w.color} size="sm" />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm">{w.name}</div>
                      <div className="truncate text-xs text-muted-foreground">
                        {t(`workspacesTypes.${w.type}`)} · {w.platform}
                      </div>
                    </div>
                    {w.id === currentId && (
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
                  navigate("/workspaces")
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
      triggerIcon = <WsIcon name={currentWorkspace.name} color={currentWorkspace.color} size="sm" />
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
        <NavLink to={item.to} end={item.end} onClick={onNavigate}>
          <Icon />
          {!collapsed && <span>{t(item.labelKey)}</span>}
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
  const workspaceId = useWorkspaceId()
  const workspace = workspaces.find((w) => w.id === workspaceId)
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
        <SidebarGroup>
          <SidebarGroupLabel>{t("sections.recentWorkspaces")}</SidebarGroupLabel>
          <SidebarMenu>
            {workspaces.map((w) => (
              <SidebarMenuItem key={w.id}>
                <SidebarMenuButton asChild tooltip={w.name}>
                  <NavLink to={`/w/${w.id}`} onClick={onNavigate}>
                    <WsIcon name={w.name} color={w.color} size="sm" />
                    {!collapsed && <span>{w.name}</span>}
                  </NavLink>
                </SidebarMenuButton>
              </SidebarMenuItem>
            ))}
          </SidebarMenu>
        </SidebarGroup>
      ) : (
        <>
          <SidebarGroup>
            <SidebarGroupLabel>{t("sections.activeTags")}</SidebarGroupLabel>
            <div className="flex flex-wrap gap-1 px-2">
              {workspace?.tags?.map((tag) => (
                <Tag key={tag} variant={systemTags.has(tag) ? "system" : "custom"} className="text-xs">{tag}</Tag>
              ))}
            </div>
          </SidebarGroup>

          <SidebarGroup>
            <Separator className="mb-2" />
            <SidebarGroupLabel>{t("sections.navigate")}</SidebarGroupLabel>
            <SidebarMenu>
              <SidebarMenuItem>
                <SidebarMenuButton asChild tooltip={t("allWorkspaces")}>
                  <NavLink to="/workspaces">
                    <Folders />
                    {!collapsed && <span>{t("allWorkspaces")}</span>}
                  </NavLink>
                </SidebarMenuButton>
              </SidebarMenuItem>
              <SidebarMenuItem>
                <SidebarMenuButton asChild tooltip={t("myTeams")}>
                  <NavLink to="/teams">
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

  const displayName = user?.username ?? ""
  const handle = user ? `@${user.username}` : ""

  async function handleLogout() {
    try {
      await logout()
    } catch {
      // The local session is gone either way — the cache has been dropped —
      // so we still send the user to the sign-in screen. But the refresh token
      // may well have survived server-side, and the cookies are httpOnly so
      // the client cannot clear them itself. On a shared machine that is worth
      // saying out loud rather than swallowing.
      toast.error(t("auth:logout.failedTitle"), {
        description: t("auth:logout.failedDescription"),
        id: "logout-failed",
      })
    }

    navigate(ROUTES.login, { replace: true })
  }

  const menuContent = (
    <DropdownMenuContent className="w-fit">
      <DropdownMenuLabel className="flex items-center gap-2.5 p-2">
        <UserAvatar name={displayName}/>
        <div className="min-w-0 flex-1">
          <div className="text-base font-medium text-text">{displayName}</div>
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
            aria-label="User menu"
            className="group/rail relative grid size-11 shrink-0 place-items-center rounded-md hover:bg-surface-2"
          >
            <UserAvatar name={displayName} />
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
          aria-label="User menu"
          className="flex items-center gap-2.5 p-2 text-left rounded-md border border-transparent hover:bg-surface-2 hover:border-border-strong focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring w-full"
        >
          <UserAvatar name={displayName}/>
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-medium text-text">{displayName}</div>
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
  const workspaceId = useWorkspaceId()
  const items = workspaceId ? workspaceNav(workspaceId) : GLOBAL_NAV
  const { state, isMobile, setOpenMobile } = useSidebar()
  const collapsed = state === "collapsed"

  function closeOnMobile() {
    if (isMobile) setOpenMobile(false)
  }

  return (
    <Sidebar collapsible="icon">
      <SidebarHeader>
        <div className={cn("flex w-full items-center", collapsed ? "flex-col gap-2" : "justify-between gap-2.5")}>
          <Logo size="md" wordmark={!collapsed} />
          <div className={cn("flex items-center gap-1", collapsed && "flex-col")}>
            {!collapsed && <NotificationsMenu />}
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
        <SidebarNav items={items} onNavigate={closeOnMobile} />
      </SidebarContent>

      <SidebarFooter className={cn(collapsed && "items-center")}>
        <Separator />
        <UserMenu collapsed={collapsed} />
      </SidebarFooter>

      <SidebarRail />
    </Sidebar>
  )
}
