import { zodResolver } from "@hookform/resolvers/zod";
import type { RefObject } from "react";
import { FormProvider, useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { FormError } from "@/components/common/FormError";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { DetailsSection } from "@/components/workspaces/create/DetailsSection";
import { ModeSection } from "@/components/workspaces/create/ModeSection";
import { TypeSection } from "@/components/workspaces/create/TypeSection";
import { useTeams } from "@/hooks/useTeams";
import { useCreateWorkspace } from "@/hooks/useWorkspaces";
import { apiErrorKey, fieldErrorKey, isApiError } from "@/lib/api-error";
import type { CreateWorkspaceInput } from "@/lib/api/types";
import { ROUTES } from "@/lib/routes";
import { createWorkspaceSchema, toInstant, type CreateWorkspaceValues } from "@/lib/schemas/workspace";

type Field = keyof CreateWorkspaceValues;

function initialValues(defaultTeamId?: string): CreateWorkspaceValues {
  return {
    name: "",
    description: "",
    color: "ember",
    type: "platform",
    platform: null,
    mode: defaultTeamId ? "team" : "solo",
    team_id: defaultTeamId ?? null,
    starts_at: null,
    ends_at: null,
    lock_after_end: false,
  };
}

function isField(name: string): name is Field {
  return Object.hasOwn(initialValues(), name);
}

/** API codes that belong under one field of the form. */
const FIELD_BY_CODE: Readonly<Record<string, Field>> = {
  workspace_name_not_sluggable: "name",
  workspace_name_reserved: "name",
  platform_required: "platform",
  ends_before_start: "ends_at",
  invalid_ownership: "team_id",
  insufficient_role: "team_id",
  team_not_found: "team_id",
};

/** Where an API error belongs: under the field its code names, or under each field a 422 lists. */
function fieldsOf(error: unknown): { field: Field; message: string }[] {
  if (!isApiError(error)) return [];
  const listed = Object.entries(error.fields ?? {})
    .filter((entry): entry is [Field, string[]] => isField(entry[0]))
    .map(([field, codes]) => ({ field, message: fieldErrorKey(codes[0] ?? "invalid") }));
  const byCode = FIELD_BY_CODE[error.code];
  return byCode ? [{ field: byCode, message: apiErrorKey(error) }, ...listed] : listed;
}

/** Local datetime-local values become UTC instants; a solo workspace sends no team. */
function toInput(v: CreateWorkspaceValues): CreateWorkspaceInput {
  const event = v.type === "event";
  return {
    name: v.name.trim(),
    description: v.description?.trim() || null,
    color: v.color,
    type: v.type,
    platform: v.platform,
    mode: v.mode,
    team_id: v.mode === "team" ? v.team_id : null,
    starts_at: event ? toInstant(v.starts_at) : null,
    ends_at: event ? toInstant(v.ends_at) : null,
    lock_after_end: event ? v.lock_after_end : false,
  };
}

type Props = Readonly<{
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Preselects team mode and this team: a team's Workspaces tab passes its id. */
  defaultTeamId?: string;
  /** Without a DialogTrigger, Radix gives focus back to nothing: this element gets it on close. */
  returnFocusTo?: RefObject<HTMLElement | null>;
}>;

/** One form in three numbered sections, as on the mockup's creation page. */
export function CreateWorkspaceDialog({ open, onOpenChange, defaultTeamId, returnFocusTo }: Props) {
  const { t } = useTranslation(["workspaces"]);
  const navigate = useNavigate();
  const teams = useTeams();
  const create = useCreateWorkspace();
  const form = useForm<CreateWorkspaceValues>({
    resolver: zodResolver(createWorkspaceSchema),
    defaultValues: initialValues(defaultTeamId),
  });

  // Closing forgets everything: the next opening starts from an empty form.
  const changeOpen = (next: boolean) => {
    if (!next) {
      form.reset(initialValues(defaultTeamId));
      create.reset();
    }
    onOpenChange(next);
  };

  const submit = (values: CreateWorkspaceValues) =>
    create.mutate(toInput(values), {
      onSuccess: (ws) => {
        changeOpen(false);
        void navigate(ROUTES.workspace(ws.slug));
      },
      onError: (error) => {
        fieldsOf(error).forEach(({ field, message }, index) =>
          form.setError(field, { message }, { shouldFocus: index === 0 }),
        );
      },
    });

  // An error already shown under its field is not repeated above the form.
  const unplaced = create.error && fieldsOf(create.error).length === 0 ? create.error : null;

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogContent
        className="flex max-h-[90vh] flex-col gap-0 p-0 sm:max-w-2xl"
        onCloseAutoFocus={(event) => {
          if (!returnFocusTo?.current) return;
          event.preventDefault();
          returnFocusTo.current.focus();
        }}
      >
        <DialogHeader className="border-b border-border px-6 pt-6 pb-4">
          <DialogTitle>{t("workspaces:create.title")}</DialogTitle>
          <DialogDescription>{t("workspaces:create.subtitle")}</DialogDescription>
        </DialogHeader>

        <FormProvider {...form}>
          <form noValidate className="flex min-h-0 flex-1 flex-col" onSubmit={(event) => void form.handleSubmit(submit)(event)}>
            {/* Only the sections scroll: the buttons stay in sight on a small screen. */}
            <div className="flex flex-col gap-6 overflow-y-auto px-6 py-5">
              {unplaced ? <FormError error={unplaced} /> : null}
              <TypeSection />
              <DetailsSection />
              <ModeSection teams={teams.data} />
            </div>

            <DialogFooter className="m-0 border-t border-border bg-transparent px-6 py-4">
              <DialogClose asChild>
                <Button type="button" variant="ghost">
                  {t("workspaces:create.cancel")}
                </Button>
              </DialogClose>
              <Button type="submit" disabled={create.isPending}>
                {t("workspaces:create.submit")}
              </Button>
            </DialogFooter>
          </form>
        </FormProvider>
      </DialogContent>
    </Dialog>
  );
}
