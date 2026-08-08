import { LogoMark } from "./LogoMark"
import type { LogoType, Size } from "@/components/ui/types"

const text: Record<Size, { word: string; tag: string; gap: string }> = {
  sm: { word: "text-h3", tag: "text-nav", gap: "gap-2" },
  md: { word: "text-page", tag: "text-xs", gap: "gap-2.5" },
  lg: { word: "text-display-sm", tag: "text-sm", gap: "gap-3" },
}

export function Logo({ size = "lg", type = "bg", variant = "default", wordmark = true } : Readonly<{ size?: Size; type?: LogoType; variant?: "default" | "minimal"; wordmark?: boolean }>) {
  const { word, gap } = text[size]
  return (
    <div className={`flex shrink-0 items-center ${wordmark ? gap : ""}`}>
      <LogoMark size={size} type={type} />
      {wordmark && (
        <div className="flex flex-col items-start">
          <span className={`font-bold tracking-head ${word} leading-tight `}>
            Pwn<span className="text-primary">Forge</span>
          </span>
          {variant === "default" && (
            <span className={`${text[size].tag} text-primary tracking-large font-normal mt-0.5 leading-none`}>
              Forge your wins
            </span>
          )}
        </div>
      )}
    </div>
  )
}

export default Logo
