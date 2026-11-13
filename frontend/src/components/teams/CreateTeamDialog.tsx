import { zodResolver } from "@hookform/resolvers/zod";
import { Controller, useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { FormError } from "@/components/common/FormError";
import { Button } from "@/components/ui/button";
import { ColorPicker } from "@/components/ui/color-picker";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useCreateTeam } from "@/hooks/useTeams";
import { apiErrorKey, fieldErrorKey, isApiError } from "@/lib/api-error";
import { ROUTES } from "@/lib/routes";
import { teamSchema, type TeamValues } from "@/lib/schemas/team";

const DEFAULTS: TeamValues = { name: "", color: "ember" };

/** API codes that belong under the name; anything else (team_quota_reached...) goes above the form. */
const NAME_CODES = new Set(["team_name_not_sluggable", "team_name_reserved"]);

function nameErrorOf(error: unknown): string | null {
  if (!isApiError(error)) return null;
  if (NAME_CODES.has(error.code)) return apiErrorKey(error);
  const listed = error.fields?.name?.[0];
  return listed ? fieldErrorKey(listed) : null;
}

type Props = Readonly<{
  open: boolean;
  onOpenChange: (open: boolean) => void;
}>;

export function CreateTeamDialog({ open, onOpenChange }: Props) {
  const { t } = useTranslation(["teams", "errors"]);
  const navigate = useNavigate();
  const create = useCreateTeam();
  const form = useForm<TeamValues>({ resolver: zodResolver(teamSchema), defaultValues: DEFAULTS });
  const nameError = form.formState.errors.name;

  // Closing forgets everything: the next opening starts from an empty form.
  const changeOpen = (next: boolean) => {
    if (!next) {
      form.reset(DEFAULTS);
      create.reset();
    }
    onOpenChange(next);
  };

  const submit = (values: TeamValues) =>
    create.mutate(
      { name: values.name.trim(), color: values.color },
      {
        onSuccess: (team) => {
          changeOpen(false);
          void navigate(ROUTES.team(team.slug));
        },
        onError: (error) => {
          const message = nameErrorOf(error);
          if (message) form.setError("name", { message }, { shouldFocus: true });
        },
      },
    );

  // An error already shown under the name is not repeated above the form.
  const unplaced = create.error && !nameErrorOf(create.error) ? create.error : null;

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("teams:create.title")}</DialogTitle>
          <DialogDescription>{t("teams:create.subtitle")}</DialogDescription>
        </DialogHeader>

        <form noValidate className="flex flex-col gap-5" onSubmit={(event) => void form.handleSubmit(submit)(event)}>
          {unplaced ? <FormError error={unplaced} /> : null}

          <Field data-invalid={!!nameError}>
            <FieldLabel htmlFor="team-name">{t("teams:create.name")}</FieldLabel>
            <Input id="team-name" autoComplete="off" aria-invalid={!!nameError} {...form.register("name")} />
            <FieldError errors={nameError ? [{ message: t(nameError.message ?? "errors:field.invalid") }] : undefined} />
          </Field>

          <div className="flex flex-col gap-2">
            {/* The picker names itself for screen readers; this is the visible label. */}
            <span aria-hidden className="text-sm font-medium text-text">
              {t("teams:create.color")}
            </span>
            <Controller
              control={form.control}
              name="color"
              render={({ field }) => (
                <ColorPicker name="team-color" label={t("teams:create.color")} value={field.value} onChange={field.onChange} />
              )}
            />
          </div>

          <DialogFooter>
            <DialogClose asChild>
              <Button type="button" variant="ghost">
                {t("teams:create.cancel")}
              </Button>
            </DialogClose>
            <Button type="submit" disabled={create.isPending}>
              {t("teams:create.submit")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
