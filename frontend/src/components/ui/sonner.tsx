import { useTheme } from "next-themes"
import { Toaster as Sonner, type ToasterProps } from "sonner"
import { CircleCheckIcon, InfoIcon, TriangleAlertIcon, OctagonXIcon, Loader2Icon } from "lucide-react"

const Toaster = ({ ...props }: ToasterProps) => {
  const { theme = "system" } = useTheme()

  return (
    <Sonner richColors
      theme={theme as ToasterProps["theme"]}
      className="toaster group text-left"
      icons={{
        success: (
          <CircleCheckIcon className="size-4" />
        ),
        info: (
          <InfoIcon className="size-4" />
        ),
        warning: (
          <TriangleAlertIcon className="size-4" />
        ),
        error: (
          <OctagonXIcon className="size-4" />
        ),
        loading: (
          <Loader2Icon className="size-4 animate-spin" />
        ),
      }}
      style={
        {
          // Neutral toasts
          "--normal-bg": "var(--color-surface)",
          "--normal-text": "var(--color-text)",
          "--normal-border": "var(--color-border)",

          // Success toasts
          "--success-bg": "var(--color-success-soft)",
          "--success-text": "var(--color-success-text)",
          "--success-border": "var(--color-success)",

          // Warning toasts
          "--warning-bg": "var(--color-warning-soft)",
          "--warning-text": "var(--color-warning-text)",
          "--warning-border": "var(--color-warning)",

          // Error toasts
          "--error-bg": "var(--color-danger-soft)",
          "--error-text": "var(--color-danger-text)",
          "--error-border": "var(--color-danger)",

          // Info toasts
          "--info-bg": "var(--color-info-soft)",
          "--info-text": "var(--color-info-text)",
          "--info-border": "var(--color-info)",

          "--border-radius": "var(--radius)",
        } as React.CSSProperties
      }
      toastOptions={{
        classNames: {
          toast: "cn-toast",
          title: "text-md! font-bold! leading-normal!",
          description: "text-xs! font-normal! text-text-muted!",
        },
      }}
      {...props}
    />
  )
}

export { Toaster }
