import { useTranslation } from "react-i18next";
import type { AccentColor } from "@/lib/api/types";
import { cn } from "@/lib/utils";

// Full class names: Tailwind only generates classes it finds written out in the code.
const SWATCH: Record<AccentColor, string> = {
  ember: "bg-ws-ember",
  blue: "bg-ws-blue",
  violet: "bg-ws-violet",
  teal: "bg-ws-teal",
  rose: "bg-ws-rose",
  amber: "bg-ws-amber",
};
const COLORS = Object.keys(SWATCH) as AccentColor[];

export function ColorPicker({ value, onChange, label, name }: Readonly<{
  value: AccentColor;
  onChange: (color: AccentColor) => void;
  label: string;
  name: string;
}>) {
  const { t } = useTranslation(["common"]);
  return (
    <fieldset className="flex flex-wrap gap-2">
      <legend className="sr-only">{label}</legend>
      {COLORS.map((color) => (
        <label
          key={color}
          className={cn(
            "grid size-8 cursor-pointer place-items-center rounded-full transition",
            value === color && "ring-2 ring-text",
          )}
        >
          <input
            type="radio"
            className="sr-only"
            name={name}
            value={color}
            checked={value === color}
            onChange={() => onChange(color)}
          />
          <span aria-hidden className={cn("size-6 rounded-full", SWATCH[color])} />
          <span className="sr-only">{t(`common:colors.${color}`)}</span>
        </label>
      ))}
    </fieldset>
  );
}
