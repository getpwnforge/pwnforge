import type { Size, LogoType } from "@/components/ui/types"

const sizeClasses: Record<Size, string> = {
  sm: "h-8 w-8",
  md: "h-11 w-11",
  lg: "h-14 w-14",
}

const logoTypes: Record<LogoType, string> = {
  bg: "/favicon.svg",
  noBg: "/icon_nobg.svg",
}

export function LogoMark({ size = "md", type = "bg" } : Readonly<{ size?: Size; type?: LogoType }>) {
  return (
    <img
      src={logoTypes[type]}
      alt="PwnForge"
      className={sizeClasses[size]}
    />
  )
}
