import { zodResolver } from "@hookform/resolvers/zod";
import { SendIcon } from "lucide-react";
import { Controller, useForm, useWatch } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { FormError } from "@/components/common/FormError";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Tabs, TabsList, TabsTab } from "@/components/ui/tabs";
import { useCreateTeamInvitation } from "@/hooks/useInvitations";
import { apiErrorKey, fieldErrorKey, isApiError } from "@/lib/api-error";
import type { TeamResponse } from "@/lib/api/types";
import {
  INVITE_TARGETS,
  invitableRolesFor,
  inviteToTeamSchema,
  toInviteRequest,
  type InviteTarget,
  type InviteToTeamValues,
} from "@/lib/schemas/invitation";

/** Codes that are about the person named, shown under the field rather than above the form. */
const WHO_CODES = new Set(["already_member"]);

const isTarget = (value: unknown): value is InviteTarget => INVITE_TARGETS.includes(value as InviteTarget);

/** The message for the "who" field, when the API's refusal is about it. */
function whoErrorOf(error: unknown): string | null {
  if (!isApiError(error)) return null;
  if (WHO_CODES.has(error.code)) return apiErrorKey(error);
  const listed = error.fields?.username?.[0] ?? error.fields?.invited_email?.[0];
  return listed ? fieldErrorKey(listed) : null;
}

/**
 * One person at a time, by username or by address, with the role they get. The answer is the
 * same whether an account holds the address or not: the form never says which.
 */
export function InviteToTeamForm({ team }: Readonly<{ team: TeamResponse }>) {
  const { t } = useTranslation(["teams", "common", "errors"]);
  const create = useCreateTeamInvitation();
  const roles = invitableRolesFor(team.my_role);
  const form = useForm<InviteToTeamValues>({
    resolver: zodResolver(inviteToTeamSchema),
    defaultValues: { target: "username", who: "", role: "member" },
  });
  const target = useWatch({ control: form.control, name: "target" });
  const { errors } = form.formState;
  const byEmail = target === "email";

  const submit = (values: InviteToTeamValues) => {
    const shown = byEmail ? values.who.trim() : `@${values.who.trim().replace(/^@/, "")}`;
    create.mutate(
      { slug: team.slug, body: toInviteRequest(values) },
      {
        onSuccess: () => {
          toast.success(t("teams:invitations.invite.sent", { who: shown }));
          form.reset({ target: values.target, who: "", role: values.role });
        },
        onError: (error) => {
          const message = whoErrorOf(error);
          if (message) form.setError("who", { message }, { shouldFocus: true });
        },
      },
    );
  };
  const unplaced = create.error && !whoErrorOf(create.error) ? create.error : null;

  const switchTarget = (value: unknown) => {
    if (!isTarget(value)) return;
    form.setValue("target", value);
    form.setValue("who", "");
    form.clearErrors("who");
    create.reset();
  };

  return (
    <form noValidate className="flex flex-col gap-3" onSubmit={(event) => void form.handleSubmit(submit)(event)}>
      {unplaced ? <FormError error={unplaced} /> : null}

      <Tabs value={target} onValueChange={switchTarget}>
        <TabsList aria-label={t("teams:invitations.invite.by")}>
          <TabsTab value="username">{t("teams:invitations.invite.byUsername")}</TabsTab>
          <TabsTab value="email">{t("teams:invitations.invite.byEmail")}</TabsTab>
        </TabsList>
      </Tabs>

      <div className="flex flex-wrap items-start gap-2">
        <Field data-invalid={!!errors.who} className="min-w-48 flex-1">
          <FieldLabel htmlFor="invite-who" className="sr-only">
            {t(byEmail ? "teams:invitations.invite.email" : "teams:invitations.invite.username")}
          </FieldLabel>
          <InputGroup>
            {byEmail ? null : (
              <InputGroupAddon>
                <span className="font-mono text-text-subtle">@</span>
              </InputGroupAddon>
            )}
            <InputGroupInput
              id="invite-who"
              type={byEmail ? "email" : "text"}
              autoComplete="off"
              spellCheck={false}
              className={byEmail ? undefined : "font-mono"}
              placeholder={t(byEmail ? "teams:invitations.invite.emailPlaceholder" : "teams:invitations.invite.usernamePlaceholder")}
              aria-invalid={!!errors.who}
              {...form.register("who")}
            />
          </InputGroup>
          <FieldError errors={errors.who ? [{ message: t(errors.who.message ?? "errors:field.invalid") }] : undefined} />
        </Field>

        <Controller
          control={form.control}
          name="role"
          render={({ field }) => (
            <Select value={field.value} onValueChange={field.onChange}>
              <SelectTrigger className="w-32" aria-label={t("teams:invitations.invite.role")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {roles.map((role) => (
                  <SelectItem key={role} value={role}>
                    {t(`common:roles.${role}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
        />

        <Button type="submit" variant="primary" disabled={create.isPending}>
          <SendIcon aria-hidden />
          {t("teams:invitations.invite.submit")}
        </Button>
      </div>

      <FieldDescription>{t("teams:invitations.invite.hintInvitation")}</FieldDescription>
    </form>
  );
}
