import {
  Avatar,
  AvatarImage,
  AvatarFallback,
  AvatarBadge,
  AvatarGroup,
  AvatarGroupCount,
} from "@/components/ui/avatar"

type Size = "sm" | "default" | "lg" // native shadcn scale (24 / 32 / 40)
type Status = "online" | "idle" | "offline"

// Presence dot colors, mapped to design-system tokens.
const STATUS: Record<Status, string> = {
  online: "bg-success",
  idle: "bg-warning",
  offline: "bg-text-subtle",
}

// Deterministic hue from the name (same formula as the old DS avatar).
function hue(name: string) {
  let h = 0
  for (let i = 0; i < name.length; i++) h = (h * 31 + (name.codePointAt(i) ?? 0)) % 360
  return h
}

function initials(name: string) {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0])
    .join("")
    .toUpperCase()
}

type UserAvatarProps = {
  name: string
  src?: string | null
  size?: Size
  status?: Status
  tinted?: boolean // deterministic per-name color on the fallback
  className?: string
}

export function UserAvatar({
  name,
  src,
  size = "default",
  status,
  tinted = true,
  className,
}: Readonly<UserAvatarProps>) {
  const h = hue(name)
  // Inline style wins over the fallback's native bg-muted / text-muted-foreground.
  const fallbackStyle = tinted
    ? { background: `oklch(32% 0.05 ${h})`, color: `oklch(82% 0.04 ${h})` }
    : undefined

  return (
    <Avatar size={size} className={className} title={name}>
      {src ? <AvatarImage src={src} alt={name} /> : null}
      <AvatarFallback style={fallbackStyle}>{initials(name)}</AvatarFallback>
      {status ? <AvatarBadge className={STATUS[status]} /> : null}
    </Avatar>
  )
}

type GroupPerson = { name: string; src?: string | null }

export function UserAvatarGroup({
  people,
  max = 4,
  size = "default",
  tinted = true,
}: Readonly<{
  people: GroupPerson[]
  max?: number
  size?: Size
  tinted?: boolean
}>) {
  const shown = people.slice(0, max)
  const extra = people.length - shown.length
  return (
    <AvatarGroup>
      {shown.map((p) => (
        <UserAvatar key={p.name} name={p.name} src={p.src} size={size} tinted={tinted} />
      ))}
      {extra > 0 ? <AvatarGroupCount>+{extra}</AvatarGroupCount> : null}
    </AvatarGroup>
  )
}
