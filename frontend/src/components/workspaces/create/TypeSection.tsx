import { GlobeIcon, TrophyIcon } from "lucide-react";
import { useController, useFormContext } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { OptionCard } from "@/components/ui/option-card";
import type { CreateWorkspaceValues } from "@/lib/schemas/workspace";

const TYPES = [
  { value: "platform", icon: <GlobeIcon /> },
  { value: "event", icon: <TrophyIcon /> },
] as const;

/** 1 · Type: training on a platform, or a CTF event with dates. */
export function TypeSection() {
  const { t } = useTranslation(["workspaces"]);
  const { control } = useFormContext<CreateWorkspaceValues>();
  const { field } = useController({ control, name: "type" });

  return (
    <fieldset>
      <legend className="mb-3 text-sm font-semibold text-text">{t("workspaces:create.steps.type")}</legend>
      <div className="grid gap-2.5 sm:grid-cols-2">
        {TYPES.map(({ value, icon }) => (
          <OptionCard
            key={value}
            name="type"
            value={value}
            checked={field.value === value}
            onChange={field.onChange}
            icon={icon}
            title={t(`workspaces:create.type.${value}`)}
            description={t(`workspaces:create.type.${value}Help`)}
          />
        ))}
      </div>
    </fieldset>
  );
}
