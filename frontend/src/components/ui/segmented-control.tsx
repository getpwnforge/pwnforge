// components/ui/segmented-control.tsx
// Thin wrapper over ToggleGroup for exclusive value selection (filters,
// time ranges, view modes, settings rows). Not a tab list: there is no
// associated panel, so the radiogroup semantics of ToggleGroup apply.
// For real tabbed navigation with panels, use Tabs instead.

import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { cn } from "@/lib/utils"

export type SegmentedOption<T extends string> = Readonly<{
  value: T
  label: string
}>

type SegmentedControlProps<T extends string> = Readonly<{
  options: readonly SegmentedOption<T>[]
  value: T
  onChange: (value: T) => void
  /** Required: the radiogroup needs an accessible name. */
  label: string
  className?: string
  disabled?: boolean
}>

export function SegmentedControl<T extends string>({
  options,
  value,
  onChange,
  label,
  className,
  disabled,
}: SegmentedControlProps<T>) {
  return (
    <ToggleGroup
      type="single"
      value={value}
      // Radix emits "" when the active item is clicked again.
      // Ignoring it keeps the control always resolved to one value.
      onValueChange={(next) => {
        if (next) onChange(next as T)
      }}
      aria-label={label}
      disabled={disabled}
      className={cn(
        "inline-flex w-auto items-center gap-0.5 rounded-base bg-surface-2 p-1",
        className,
      )}
    >
      {options.map((option) => (
        <ToggleGroupItem
          key={option.value}
          value={option.value}
          aria-label={option.label}
          className={cn(
            "h-7 min-w-0 flex-none rounded-sm px-2.5 text-sm font-normal",
            "text-muted-foreground hover:bg-transparent hover:text-foreground",
            "data-[state=on]:bg-card data-[state=on]:text-foreground",
            "data-[state=on]:shadow-sm data-[state=on]:font-medium",
          )}
        >
          {option.label}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  )
}
