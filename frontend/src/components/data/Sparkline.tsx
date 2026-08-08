import { Area, AreaChart, XAxis } from "recharts"
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart"

export type SparkPoint = { label: string; value: number }

const config = {
  value: { label: "Value", color: "var(--color-chart-1)" },
} satisfies ChartConfig

export function SparklineChart({
  data,
  className = "h-36 w-full",
  interactive = true,
  unit = "",
}: Readonly<{
  data: SparkPoint[]
  className?: string
  interactive?: boolean
  unit?: string
}>) {
  return (
    <ChartContainer config={config} className={className}>
      <AreaChart data={data} margin={{ top: 6, right: 4, bottom: 0, left: 4 }}>
        <XAxis dataKey="label" hide />
        {interactive && (
          <ChartTooltip
            cursor={{ stroke: "var(--color-border-strong)", strokeWidth: 1 }}
            content={<ChartTooltipContent formatter={(v) => `${v}${unit}`} />}
          />
        )}
        <Area
          dataKey="value"
          type="monotone"
          stroke="var(--color-value)"
          strokeWidth={2}
          fill="var(--color-value)"
          fillOpacity={0.08}
          dot={false}
          activeDot={interactive ? { r: 3 } : false}
          isAnimationActive={false}
        />
      </AreaChart>
    </ChartContainer>
  )
}
