import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

type Props<V extends string> = Readonly<{
  name: string;
  value: V;
  checked: boolean;
  onChange: (value: V) => void;
  title: string;
  description?: string;
  icon?: ReactNode;
  disabled?: boolean;
}>;

/** A selectable card backed by a real radio input, so arrows and screen readers work. */
export function OptionCard<V extends string>({ name, value, checked, onChange, title, description, icon, disabled }: Props<V>) {
  return (
    <label
      className={cn(
        "flex cursor-pointer flex-col gap-1.5 rounded-lg border border-border p-3.5 transition-colors",
        "hover:border-border-strong has-focus-visible:ring-[3px] has-focus-visible:ring-ring/50",
        checked && "border-primary bg-ember-soft hover:border-primary",
        disabled && "cursor-not-allowed opacity-50 hover:border-border",
      )}
    >
      <input
        type="radio"
        className="sr-only"
        name={name}
        value={value}
        checked={checked}
        disabled={disabled}
        onChange={() => onChange(value)}
      />
      <span className="flex items-center gap-2">
        {icon ? (
          <span aria-hidden className={cn("[&_svg]:size-4", checked ? "text-primary" : "text-text-muted")}>
            {icon}
          </span>
        ) : null}
        <span className="text-sm font-semibold text-text">{title}</span>
      </span>
      {description ? <span className="text-xs text-text-muted">{description}</span> : null}
    </label>
  );
}
