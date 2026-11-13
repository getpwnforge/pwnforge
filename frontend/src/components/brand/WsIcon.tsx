import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import type { Size } from "@/components/ui/types"
import { cn } from "@/lib/utils"
import { wsColorFromName, type WsColor } from "@/lib/ws-color"

const sizeClasses: Record<Size, { box: string; text: string }> = {
  sm: { box: "size-6", text: "text-sm" },
  md: { box: "size-8", text: "text-base" },
  lg: { box: "size-10", text: "text-lg" },
}

const bgClasses: Record<WsColor, string> = {
  ember:  "bg-ws-ember",
  blue:   "bg-ws-blue",
  violet: "bg-ws-violet",
  teal:   "bg-ws-teal",
  rose:   "bg-ws-rose",
  amber:  "bg-ws-amber",
}

type Props = Readonly<{
  size?: Size
  name: string
  color?: WsColor
  /** The uploaded image; null or absent shows the monogram. */
  src?: string | null
}>

export function WsIcon({ name, color, size = "md", src }: Props) {
  if (!src) return <Monogram name={name} color={color} size={size} />
  // Radix only shows the image once it has loaded: a missing or failing one
  // (avatar deleted in the meantime, 404) leaves the monogram in place.
  return (
    <Avatar className={cn("shrink-0 rounded-md", sizeClasses[size].box)}>
      <AvatarImage src={src} alt="" className="rounded-md object-cover" />
      <AvatarFallback className="rounded-md bg-transparent p-0">
        <Monogram name={name} color={color} size={size} />
      </AvatarFallback>
    </Avatar>
  )
}

/** The monogram, unchanged: initials on the workspace colour. */
function Monogram({ name, color, size }: Readonly<{ name: string; color?: WsColor; size: Size }>) {
  const { box, text } = sizeClasses[size]
  const initials = name.split(/[\s/]/).filter(Boolean).slice(0, 2).map((s: string) => s[0]).join("").toUpperCase()
  const resolved = color ?? wsColorFromName(name)
  return (
    <div className={cn("flex items-center shrink-0 justify-center rounded-md text-white", box, bgClasses[resolved])}>
      <span className={cn("font-semibold font-mono", text)}>{initials}</span>
    </div>
  )
}
