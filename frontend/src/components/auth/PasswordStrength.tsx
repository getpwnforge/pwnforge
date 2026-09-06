// src/components/auth/PasswordStrength.tsx
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/utils";
import { loadEstimator, scorePassword, type StrengthScore } from "@/lib/password-strength";

/** Bars lit, colour and label, per zxcvbn score. */
const LEVELS = [
  { bars: 1, color: "bg-danger", label: "weak" },
  { bars: 1, color: "bg-danger", label: "weak" },
  { bars: 2, color: "bg-warning", label: "medium" },
  { bars: 3, color: "bg-success", label: "good" },
  { bars: 4, color: "bg-success", label: "strong" },
] as const;

// Long enough that a typist does not trigger an estimate per keystroke, short
// enough that the bar has moved by the time they look down at it.
const DEBOUNCE_MS = 150;

export function PasswordStrength({ password }: Readonly<{ password: string }>) {
  const { t } = useTranslation("auth");

  /**
   * Last computed score, deliberately kept across keystrokes.
   *
   * Tying it to the value it came from would blank the indicator between every
   * character, while the next estimate is still debouncing — the bars would
   * read as resetting rather than moving. The cost is that for ~150ms after an
   * edit the score shown is the previous one.
   */
  const [score, setScore] = useState<StrengthScore | null>(null);

  useEffect(() => {
    if (!password) return;

    let cancelled = false;

    const timer = setTimeout(() => {
      void loadEstimator().then((ready) => {
        if (!ready || cancelled) return;

        const next = scorePassword(password);
        if (next !== null) setScore(next);
      });
    }, DEBOUNCE_MS);

    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [password]);

  // Only the emptied-field case is derived: nothing to clear, so no
  // synchronous setState in the effect body.
  const level = password && score !== null ? LEVELS[score] : null;

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex gap-1" aria-hidden>
        {[0, 1, 2, 3].map((index) => (
          <span
            key={index}
            className={cn(
              "h-0.75 flex-1 rounded-full transition-colors",
              level && index < level.bars ? level.color : "bg-surface-2",
            )}
          />
        ))}
      </div>
      {/* Height reserved even when empty, so the label appearing does not push
          the rest of the form down. Announced rather than shown only in colour:
          the bars alone carry the information for everyone else. */}
      <span
        role="status"
        aria-live="polite"
        className="h-4 text-xs leading-4 text-muted-foreground"
      >
        {level ? t(`password.strength.${level.label}`) : null}
      </span>
    </div>
  );
}
