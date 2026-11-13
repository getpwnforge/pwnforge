import type { ReactNode } from "react";

/** A titled block of a settings page: the heading underlined, then its rows. */
export function SettingsSection({ id, title, children }: Readonly<{ id: string; title: string; children: ReactNode }>) {
  return (
    <section aria-labelledby={id}>
      <h2 id={id} className="mb-3.5 border-b border-border pb-1.5 text-sm font-semibold text-text">
        {title}
      </h2>
      {children}
    </section>
  );
}

/** One setting: label and hint on the left, its control or value on the right. */
export function SettingRow({ label, hint, children }: Readonly<{ label: string; hint?: string; children: ReactNode }>) {
  return (
    <div className="flex flex-wrap items-center gap-4 border-b border-border py-2.5 last:border-b-0">
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-medium text-text">{label}</div>
        {hint ? <div className="mt-0.5 text-xs text-text-muted">{hint}</div> : null}
      </div>
      {children}
    </div>
  );
}
