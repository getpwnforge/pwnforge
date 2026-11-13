import { zodResolver } from "@hookform/resolvers/zod";
import { LinkIcon, TriangleAlertIcon } from "lucide-react";
import { useState } from "react";
import { Controller, useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { FormError } from "@/components/common/FormError";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useCreateTeamInvitation } from "@/hooks/useInvitations";
import type { TeamResponse } from "@/lib/api/types";
import { invitationLink } from "@/lib/invitation-link";
import {
  createTeamLinkSchema,
  invitableRolesFor,
  LINK_EXPIRY_DAYS,
  toLinkRequest,
  type CreateTeamLinkValues,
} from "@/lib/schemas/invitation";

const DEFAULTS: CreateTeamLinkValues = { role: "member", days: "7", max_uses: "" };

/**
 * Creates a generic link. The token comes back once, in the answer: it is shown here, in React
 * state only, and forgotten when the dialog closes. Nothing logs it and nothing caches it.
 */
export function CreateTeamLinkDialog({ team }: Readonly<{ team: TeamResponse }>) {
  const { t } = useTranslation(["teams", "common", "errors"]);
  const create = useCreateTeamInvitation();
  const [open, setOpen] = useState(false);
  const [link, setLink] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const form = useForm<CreateTeamLinkValues>({ resolver: zodResolver(createTeamLinkSchema), defaultValues: DEFAULTS });
  const { errors } = form.formState;

  const onOpenChange = (next: boolean) => {
    setOpen(next);
    if (next) return;
    // Closed: the link is gone for good, and the next opening starts afresh.
    setLink(null);
    setCopied(false);
    form.reset(DEFAULTS);
    create.reset();
  };

  const submit = (values: CreateTeamLinkValues) => {
    create.mutate(
      { slug: team.slug, body: toLinkRequest(values) },
      {
        onSuccess: (created) => {
          if ("token" in created) setLink(invitationLink("team", created.token));
        },
      },
    );
  };

  const copy = async () => {
    if (!link) return;
    try {
      await navigator.clipboard.writeText(link);
      setCopied(true);
    } catch {
      toast.error(t("teams:invitations.link.copyFailed"));
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogTrigger asChild>
        <Button variant="primary">
          <LinkIcon aria-hidden />
          {t("teams:invitations.link.create")}
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-md">
        {link ? (
          <>
            <DialogHeader>
              <DialogTitle>{t("teams:invitations.link.doneTitle")}</DialogTitle>
              <DialogDescription>{t("teams:invitations.link.doneText", { team: team.name })}</DialogDescription>
            </DialogHeader>
            <div className="flex gap-2">
              <Input
                readOnly
                value={link}
                aria-label={t("teams:invitations.link.url")}
                className="font-mono text-xs text-text-muted"
                onFocus={(event) => event.currentTarget.select()}
              />
              <Button type="button" variant="secondary" onClick={() => void copy()}>
                {t(copied ? "teams:invitations.link.copied" : "teams:invitations.link.copy")}
              </Button>
            </div>
            <p role="note" className="flex gap-2.5 rounded-lg border border-border-strong bg-surface p-3 text-xs text-text">
              <TriangleAlertIcon aria-hidden className="mt-px size-4 shrink-0 text-primary" />
              {t("teams:invitations.link.once")}
            </p>
            <DialogFooter>
              <Button type="button" variant="primary" onClick={() => onOpenChange(false)}>
                {t("teams:invitations.link.done")}
              </Button>
            </DialogFooter>
          </>
        ) : (
          <form noValidate className="flex flex-col gap-4" onSubmit={(event) => void form.handleSubmit(submit)(event)}>
            <DialogHeader>
              <DialogTitle>{t("teams:invitations.link.dialogTitle")}</DialogTitle>
              <DialogDescription>{t("teams:invitations.link.dialogText", { team: team.name })}</DialogDescription>
            </DialogHeader>

            {create.error ? <FormError error={create.error} /> : null}

            <Field>
              <FieldLabel htmlFor="link-role">{t("teams:invitations.link.role")}</FieldLabel>
              <Controller
                control={form.control}
                name="role"
                render={({ field }) => (
                  <Select value={field.value} onValueChange={field.onChange}>
                    <SelectTrigger id="link-role" className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {invitableRolesFor(team.my_role).map((role) => (
                        <SelectItem key={role} value={role}>
                          {t(`common:roles.${role}`)}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}
              />
            </Field>

            <div className="grid gap-3 sm:grid-cols-2">
              <Field>
                <FieldLabel htmlFor="link-days">{t("teams:invitations.link.expiry")}</FieldLabel>
                <Controller
                  control={form.control}
                  name="days"
                  render={({ field }) => (
                    <Select value={field.value} onValueChange={field.onChange}>
                      <SelectTrigger id="link-days" className="w-full">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {LINK_EXPIRY_DAYS.map((days) => (
                          <SelectItem key={days} value={days}>
                            {t("teams:invitations.link.days", { count: Number(days) })}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  )}
                />
              </Field>
              <Field data-invalid={!!errors.max_uses}>
                <FieldLabel htmlFor="link-max-uses">{t("teams:invitations.link.maxUses")}</FieldLabel>
                <Input
                  id="link-max-uses"
                  inputMode="numeric"
                  autoComplete="off"
                  className="tabular-nums"
                  placeholder={t("teams:invitations.link.unlimited")}
                  aria-invalid={!!errors.max_uses}
                  {...form.register("max_uses")}
                />
                <FieldError
                  errors={errors.max_uses ? [{ message: t(errors.max_uses.message ?? "errors:field.invalid") }] : undefined}
                />
              </Field>
            </div>
            <FieldDescription>{t("teams:invitations.link.maxUsesHint")}</FieldDescription>

            <DialogFooter>
              <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
                {t("common:actions.cancel")}
              </Button>
              <Button type="submit" variant="primary" disabled={create.isPending}>
                {t("teams:invitations.link.submit")}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}
