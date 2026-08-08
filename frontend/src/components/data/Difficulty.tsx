
const DIFFICULTY_MAP = { easy: 1, medium: 2, hard: 3, insane: 4 } as const;
type DifficultyLevel = keyof typeof DIFFICULTY_MAP;

const difficultyColors: Record<DifficultyLevel, string> = {
  easy: "bg-diff-easy",
  medium: "bg-diff-medium",
  hard: "bg-diff-hard",
  insane: "bg-diff-insane",
};

export function Difficulty({ level }: Readonly<{ level: DifficultyLevel }>) {
  const n = DIFFICULTY_MAP[level] || 0;
  return (
    <div className="flex items-center gap-1">
      {Array.from({ length: 4 }, (_, i) => (
        <div
          key={i}
          className={`h-3 w-2 rounded-xs ${i < n ? difficultyColors[level] : "bg-surface-2"}`}
        />
      ))}
    </div>
  )
}
