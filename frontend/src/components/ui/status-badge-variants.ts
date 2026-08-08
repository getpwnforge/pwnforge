export type StatusVariant = "default" | "success" | "warning" | "danger"

export const statusBorder: Record<StatusVariant, string> = {
  default: "border-border-strong",
  success: "border-success/45",
  warning: "border-warning/45",
  danger: "border-danger/45",
}
