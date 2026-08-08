import { Badge } from "@/components/ui/badge"

type CategoryBadgeProps = {
  label: string // from the seeded `categories` table
  color: string // CSS color from the same row
}

export function CategoryBadge({ label, color }: Readonly<CategoryBadgeProps>) {
  return (
    <Badge variant="default" dot={color}>
      {label}
    </Badge>
  )
}
