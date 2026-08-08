import type { Size } from "@/components/ui/types"
import { wsColorFromName, type WsColor } from "@/lib/ws-color"

const sizeClasses: Record<Size, { box: string; text: string }> = {
  sm: { box: "h-6 w-6", text: "text-sm" },
  md: { box: "h-8 w-8", text: "text-base" },
  lg: { box: "h-10 w-10", text: "text-lg" },
}

const bgClasses: Record<WsColor, string> = {
  ember:  "bg-ws-ember",
  blue:   "bg-ws-blue",
  violet: "bg-ws-violet",
  teal:   "bg-ws-teal",
  rose:   "bg-ws-rose",
  amber:  "bg-ws-amber",
}

export function WsIcon({ name, color, size = "md" } : Readonly<{ size?: Size, name: string, color?: WsColor }>) {
  const { box, text } = sizeClasses[size]
  const initials = name.split(/[\s/]/).filter(Boolean).slice(0, 2).map((s: string) => s[0]).join('').toUpperCase();
  const resolved = color ?? wsColorFromName(name)
  return (
    <div className={`flex items-center shrink-0 justify-center rounded-md text-white ${box} ${bgClasses[resolved]}`}>
      <span className={`font-semibold font-mono ${text}`}>{initials}</span>
    </div>
  )
}
