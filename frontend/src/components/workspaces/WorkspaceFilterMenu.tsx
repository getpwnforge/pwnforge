import { FilterIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { PLATFORMS, WORKSPACE_MODES, WORKSPACE_TYPES } from "@/lib/schemas/workspace";
import { countFacets, NO_FACETS, toggleFacet, type WorkspaceFacets } from "@/lib/workspace-filters";

// Checking a box keeps the menu open, so several can be checked in a row.
const stayOpen = (event: Event) => event.preventDefault();

export function WorkspaceFiltersMenu({ value, onChange }: Readonly<{
  value: WorkspaceFacets;
  onChange: (next: WorkspaceFacets) => void;
}>) {
  const { t } = useTranslation(["workspaces", "nav"]);
  const active = countFacets(value);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm">
          <FilterIcon aria-hidden />
          {t("workspaces:list.filters")}
          <Badge variant="default">{active}</Badge>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-56">
        <DropdownMenuLabel>{t("workspaces:list.facets.type")}</DropdownMenuLabel>
        {WORKSPACE_TYPES.map((type) => (
          <DropdownMenuCheckboxItem
            key={type}
            checked={value.types.has(type)}
            onCheckedChange={() => onChange(toggleFacet(value, "types", type))}
            onSelect={stayOpen}
          >
            {t(`workspaces:create.type.${type}`)}
          </DropdownMenuCheckboxItem>
        ))}

        <DropdownMenuSeparator />
        <DropdownMenuLabel>{t("workspaces:list.facets.mode")}</DropdownMenuLabel>
        {WORKSPACE_MODES.map((mode) => (
          <DropdownMenuCheckboxItem
            key={mode}
            checked={value.modes.has(mode)}
            onCheckedChange={() => onChange(toggleFacet(value, "modes", mode))}
            onSelect={stayOpen}
          >
            {t(`workspaces:create.mode.${mode}`)}
          </DropdownMenuCheckboxItem>
        ))}

        <DropdownMenuSeparator />
        <DropdownMenuLabel>{t("workspaces:list.facets.platform")}</DropdownMenuLabel>
        {PLATFORMS.map((platform) => (
          <DropdownMenuCheckboxItem
            key={platform}
            checked={value.platforms.has(platform)}
            onCheckedChange={() => onChange(toggleFacet(value, "platforms", platform))}
            onSelect={stayOpen}
          >
            {t(`nav:platforms.${platform}`)}
          </DropdownMenuCheckboxItem>
        ))}

        <DropdownMenuSeparator />
        <DropdownMenuItem disabled={active === 0} onSelect={() => onChange(NO_FACETS)}>
          {t("workspaces:list.clearFilters")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
