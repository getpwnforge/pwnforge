import { CheckIcon, ChevronsUpDownIcon, UserIcon, UsersIcon } from "lucide-react";
import { useState } from "react";
import { useController, useFormContext, useFormState } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { WsIcon } from "@/components/brand/WsIcon";
import { Button } from "@/components/ui/button";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { OptionCard } from "@/components/ui/option-card";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import type { TeamResponse } from "@/lib/api/types";
import { avatarUrl } from "@/lib/avatar";
import { canCreateTeamWorkspace } from "@/lib/permissions";
import type { CreateWorkspaceValues } from "@/lib/schemas/workspace";

/**
 * 3 · Working mode: for me alone, or for one of the teams where I may create a workspace.
 * `teams` is undefined while the list loads.
 */
export function ModeSection({ teams }: Readonly<{ teams: readonly TeamResponse[] | undefined }>) {
  const { t } = useTranslation(["workspaces", "errors"]);
  const { control } = useFormContext<CreateWorkspaceValues>();
  const mode = useController({ control, name: "mode" });
  const teamId = useController({ control, name: "team_id" });
  const [picking, setPicking] = useState(false);

  const eligible = (teams ?? []).filter((team) => canCreateTeamWorkspace(team));
  // Loading is not "no team": the explanation only shows once the list is known.
  const noTeam = teams !== undefined && eligible.length === 0;
  const chosen = eligible.find((team) => team.id === teamId.field.value);
  const { errors } = useFormState({ control, name: "team_id" });
  const teamError = errors.team_id;

  return (
    <fieldset className="flex flex-col gap-3">
      <legend className="mb-3 text-sm font-semibold text-text">{t("workspaces:create.steps.mode")}</legend>
      <div className="grid gap-2.5 sm:grid-cols-2">
        <OptionCard
          name="mode"
          value="solo"
          checked={mode.field.value === "solo"}
          onChange={mode.field.onChange}
          icon={<UserIcon />}
          title={t("workspaces:create.mode.solo")}
          description={t("workspaces:create.mode.soloHelp")}
        />
        <OptionCard
          name="mode"
          value="team"
          checked={mode.field.value === "team"}
          onChange={mode.field.onChange}
          disabled={eligible.length === 0}
          icon={<UsersIcon />}
          title={t("workspaces:create.mode.team")}
          description={t(noTeam ? "workspaces:create.mode.noEligibleTeam" : "workspaces:create.mode.teamHelp")}
        />
      </div>

      {mode.field.value === "team" ? (
        <Field data-invalid={!!teamError}>
          <FieldLabel id="ws-team-label" htmlFor="ws-team">
            {t("workspaces:create.mode.pickTeam")}
          </FieldLabel>
          <Popover open={picking} onOpenChange={setPicking}>
            <PopoverTrigger asChild>
              <Button
                id="ws-team"
                type="button"
                variant="secondary"
                role="combobox"
                aria-labelledby="ws-team-label"
                aria-expanded={picking}
                aria-invalid={!!teamError}
                className="h-auto w-full justify-between px-2 py-1.5 font-normal"
              >
                {chosen ? <TeamLine team={chosen} /> : (
                  <span className="px-1 text-text-muted">{t("workspaces:create.mode.pickTeamPlaceholder")}</span>
                )}
                <ChevronsUpDownIcon className="text-text-muted" aria-hidden />
              </Button>
            </PopoverTrigger>
            <PopoverContent className="w-(--radix-popover-trigger-width) p-0" align="start">
              <Command>
                <CommandInput placeholder={t("workspaces:create.mode.searchTeam")} />
                <CommandList>
                  <CommandEmpty>{t("workspaces:create.mode.noTeamFound")}</CommandEmpty>
                  <CommandGroup>
                    {eligible.map((team) => (
                      <CommandItem
                        key={team.id}
                        // The id keeps two teams of the same name apart; the name is what the search matches.
                        value={team.id}
                        keywords={[team.name]}
                        onSelect={() => {
                          teamId.field.onChange(team.id);
                          setPicking(false);
                        }}
                      >
                        <TeamLine team={team} />
                        {team.id === teamId.field.value ? <CheckIcon className="ml-auto" aria-hidden /> : null}
                      </CommandItem>
                    ))}
                  </CommandGroup>
                </CommandList>
              </Command>
            </PopoverContent>
          </Popover>
          <FieldError errors={teamError ? [{ message: t(teamError.message ?? "errors:field.invalid") }] : undefined} />
        </Field>
      ) : null}
    </fieldset>
  );
}

/** A team as the mockup lists it: icon, name, then its size and my role in it. */
function TeamLine({ team }: Readonly<{ team: TeamResponse }>) {
  const { t } = useTranslation(["workspaces", "common"]);
  return (
    <span className="flex min-w-0 items-center gap-2 text-left">
      <span aria-hidden>
        <WsIcon name={team.name} color={team.color} size="sm" src={avatarUrl("teams", team.id, team.avatar_version, 64)} />
      </span>
      <span className="flex min-w-0 flex-col">
        <span className="truncate text-sm font-medium text-text">{team.name}</span>
        <span className="truncate text-xs text-text-muted">
          {t("workspaces:create.mode.teamMeta", { count: team.member_count, role: t(`common:roles.${team.my_role}`) })}
        </span>
      </span>
    </span>
  );
}
