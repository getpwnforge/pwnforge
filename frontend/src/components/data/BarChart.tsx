import { useMemo } from "react"
import {
  Bar,
  BarChart,
  Rectangle,
  XAxis,
  type BarShapeProps,
} from "recharts"
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart"

export type BarPoint = { label: string; value: number }

const config = {
  value: { label: "Value", color: "var(--color-chart-1)" },
} satisfies ChartConfig

// Opacity encodes magnitude alongside bar height. The floor keeps small values
// legible against the chart surface instead of fading them out entirely.
const MIN_OPACITY = 0.35
const MAX_OPACITY = 1

function opacityFor(value: number, peak: number) {
  if (peak <= 0) return MAX_OPACITY
  const ratio = Math.min(Math.abs(value) / peak, 1)
  return MIN_OPACITY + ratio * (MAX_OPACITY - MIN_OPACITY)
}

// Passed to <Bar shape={...} /> as an element; Recharts clones it with the
// computed rectangle props, so everything but `peak` arrives from the chart.
function ValueBar({
  peak,
  ...props
}: Readonly<Partial<BarShapeProps> & { peak: number }>) {
  const point = props.payload as BarPoint | undefined
  return (
    <Rectangle
      {...props}
      fillOpacity={opacityFor(point?.value ?? 0, peak)}
    />
  )
}

export function BarChartComponent({
  data,
  className = "h-36 w-full",
  interactive = true,
}: Readonly<{
  data: BarPoint[]
  className?: string
  interactive?: boolean
}>) {
  const peak = useMemo(
    () => Math.max(0, ...data.map((d) => Math.abs(d.value))),
    [data],
  )

  return (
    <ChartContainer config={config} className={className}>
      <BarChart data={data} barCategoryGap="4%">
        <XAxis
          dataKey="label"
          axisLine={false}
          tickLine={false}
          tickMargin={5}
        />
        {interactive && (
          <ChartTooltip
            content={<ChartTooltipContent />}
          />
        )}
        <Bar
          dataKey="value"
          fill="var(--color-value)"
          shape={<ValueBar peak={peak} />}
        />
      </BarChart>
    </ChartContainer>
  )
}
