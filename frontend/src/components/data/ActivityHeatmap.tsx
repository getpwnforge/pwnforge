import { useMemo } from "react"
import { cn } from "@/lib/utils"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"

export type ActivityDay = { date: string; count: number }

// Explicit px arbitrary values, not the rem-based spacing scale (gap-0.75,
// size-2.5, etc): this app's <html> sets font-size to 14px (see globals.css),
// so 1rem there is 14px, not the browser-default 16px the scale assumes. That
// silently desyncs the CSS pitch from these constants used to place the
// month labels — keep CELL/GAP and the arbitrary values below in lockstep.
const CELL = 10 // px, keep in sync with the size-[10px] utility below
const GAP = 3 // px, keep in sync with the gap-[3px] utility below
const COL_W = CELL + GAP

const HEAT_BG: Record<0 | 1 | 2 | 3 | 4, string> = {
  0: "bg-heat-0",
  1: "bg-heat-1",
  2: "bg-heat-2",
  3: "bg-heat-3",
  4: "bg-heat-4",
}

const DAY_ROW_LABELS = ["", "Mon", "", "Wed", "", "Fri", ""] // Sun..Sat
const MONTH_NAMES = [
  "Jan", "Feb", "Mar", "Apr", "May", "Jun",
  "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
]

function toDateKey(d: Date): string {
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, "0")
  const day = String(d.getDate()).padStart(2, "0")
  return `${y}-${m}-${day}`
}

function parseDateKey(key: string): Date {
  const [y, m, d] = key.split("-").map(Number)
  return new Date(y, m - 1, d)
}

function formatFull(d: Date): string {
  return `${MONTH_NAMES[d.getMonth()]} ${d.getDate()}, ${d.getFullYear()}`
}

// Quantile buckets (not a flat ratio-to-max) so a single outlier day doesn't
// flatten every other day into the lowest level.
function computeThresholds(counts: number[]): [number, number, number] {
  const nonZero = counts.filter((c) => c > 0).sort((a, b) => a - b)
  if (nonZero.length === 0) return [1, 2, 3]
  const q = (p: number) =>
    nonZero[Math.min(nonZero.length - 1, Math.floor(p * nonZero.length))]
  return [q(0.25), q(0.5), q(0.75)]
}

function levelFor(count: number, [q1, q2, q3]: [number, number, number]): 0 | 1 | 2 | 3 | 4 {
  if (count <= 0) return 0
  if (count <= q1) return 1
  if (count <= q2) return 2
  if (count <= q3) return 3
  return 4
}

type CellData = { date: Date; key: string; count: number; inRange: boolean }

export function ActivityHeatmap({
  data,
  weeks = 53,
  endDate,
  unit = "activities",
  className,
}: Readonly<{
  data: ActivityDay[]
  weeks?: number
  endDate?: Date | string
  unit?: string
  className?: string
}>) {
  const endDay = useMemo(() => {
    let raw: Date
    if (endDate === undefined) {
      raw = new Date()
    } else if (typeof endDate === "string") {
      raw = parseDateKey(endDate)
    } else {
      raw = endDate
    }
    return new Date(raw.getFullYear(), raw.getMonth(), raw.getDate())
  }, [endDate])

  const countByDate = useMemo(() => {
    const m = new Map<string, number>()
    for (const d of data) m.set(d.date, (m.get(d.date) ?? 0) + d.count)
    return m
  }, [data])

  const columns = useMemo(() => {
    const lastSunday = new Date(endDay)
    lastSunday.setDate(lastSunday.getDate() - lastSunday.getDay())
    const gridStart = new Date(lastSunday)
    gridStart.setDate(gridStart.getDate() - (weeks - 1) * 7)

    const cols: CellData[][] = []
    const cursor = new Date(gridStart)
    for (let w = 0; w < weeks; w++) {
      const col: CellData[] = []
      for (let d = 0; d < 7; d++) {
        const key = toDateKey(cursor)
        const inRange = cursor <= endDay
        col.push({
          date: new Date(cursor),
          key,
          count: inRange ? (countByDate.get(key) ?? 0) : 0,
          inRange,
        })
        cursor.setDate(cursor.getDate() + 1)
      }
      cols.push(col)
    }
    return cols
  }, [endDay, weeks, countByDate])

  const thresholds = useMemo(() => {
    const counts = columns.flat().filter((c) => c.inRange).map((c) => c.count)
    return computeThresholds(counts)
  }, [columns])

  const total = useMemo(
    () => columns.flat().reduce((sum, c) => sum + c.count, 0),
    [columns],
  )

  // Label the column that contains the 1st of a month (not the column whose
  // Sunday happens to fall in that month) so the label lines up with the
  // actual day, since a week can straddle a month boundary.
  const monthLabels = useMemo(() => {
    const labels: { index: number; label: string }[] = []
    let lastLabeled = -Infinity
    columns.forEach((col, w) => {
      const firstOfMonth = col.find((day) => day.date.getDate() === 1)
      if (firstOfMonth && w - lastLabeled >= 2) {
        labels.push({ index: w, label: MONTH_NAMES[firstOfMonth.date.getMonth()] })
        lastLabeled = w
      }
    })
    return labels
  }, [columns])

  const rangeLabel = weeks >= 50 ? "the last year" : `the last ${weeks} weeks`

  return (
    <TooltipProvider delayDuration={150}>
      <div className={cn("inline-flex flex-col gap-2", className)}>
        <p className="text-xs text-muted-foreground">
          <span className="font-medium text-text">{total}</span> {unit} in {rangeLabel}
        </p>

        <div className="flex gap-2">
          <div
            className="flex flex-col gap-0.75 pt-3.5"
            aria-hidden
          >
            {DAY_ROW_LABELS.map((label, i) => (
              <div
                key={i}
                className="flex h-2.5 items-center text-[10px] leading-none text-text-subtle"
              >
                {label}
              </div>
            ))}
          </div>

          <div className="overflow-x-auto">
            <div
              className="relative"
              style={{ width: columns.length * COL_W - GAP }}
            >
              <div className="relative h-3.5">
                {monthLabels.map(({ index, label }) => (
                  <span
                    key={index}
                    className="absolute top-0 text-[10px] leading-none text-text-subtle"
                    style={{ left: index * COL_W }}
                  >
                    {label}
                  </span>
                ))}
              </div>

              <div className="flex gap-0.75">
                {columns.map((col) => (
                  <div key={col[0].key} className="flex flex-col gap-0.75">
                    {col.map((day) => (
                      <HeatCell key={day.key} day={day} thresholds={thresholds} unit={unit} />
                    ))}
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>

        <div className="flex items-center gap-1 text-[10px] leading-none text-text-subtle">
          <span>Less</span>
          {([0, 1, 2, 3, 4] as const).map((level) => (
            <div key={level} className={cn("size-2.5 rounded-xs", HEAT_BG[level])} />
          ))}
          <span>More</span>
        </div>
      </div>
    </TooltipProvider>
  )
}

function HeatCell({
  day,
  thresholds,
  unit,
}: Readonly<{
  day: CellData
  thresholds: [number, number, number]
  unit: string
}>) {
  if (!day.inRange) {
    return <div className="size-2.5" aria-hidden />
  }

  const level = levelFor(day.count, thresholds)

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={`${day.count} ${unit} on ${formatFull(day.date)}`}
          className={cn(
            "block size-2.5 rounded-xs border-0 p-0 outline-1 outline-offset-1 outline-transparent transition-colors hover:outline-text-subtle focus-visible:outline-text-subtle",
            HEAT_BG[level],
          )}
        />
      </TooltipTrigger>
      <TooltipContent side="top">
        <span className="font-medium">{day.count} {unit}</span>
        <span className="opacity-70"> · {formatFull(day.date)}</span>
      </TooltipContent>
    </Tooltip>
  )
}
