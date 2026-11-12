import { zodResolver } from "@hookform/resolvers/zod";
import { MailIcon } from "lucide-react";
import { useState, type ReactNode } from "react";
import { Controller, useForm, useWatch } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { z } from "zod";

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
  DialogTrigger,
} from "@/components/ui/dialog";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Tabs, TabsList, TabsTab } from "@/components/ui/tabs";
import { useCreateWorkspaceInvitation } from "@/hooks/useInvitations";
import { apiErrorKey, fieldErrorKey, isApiError } from "@/lib/api-error";
import type { CreateWorkspaceInvitationRequest, GuestRole, WorkspaceResponse } from "@/lib/api/types";

const TARGETS = ["username", "email"] as const;
type Target = (typeof TARGETS)[number];
const isTarget = (value: unknown): value is Target => TARGETS.includes(value as Target);
/** A guest holds at most Member: the roles above come from the team. */
const GUEST_ROLES = ["member", "viewer"] as const satisfies readonly GuestRole[];
/** Enough to catch a typo; the API decides what an address is. Linear: no two parts can match the same character. */
const EMAIL = /^[^\s@]+@[^\s@.]+(?:\.[^\s@.]+)+$/;

const schema = z
  .object({
    target: z.enum(TARGETS),
    who: z.string().trim(),
    role: z.enum(GUEST_ROLES),
  })
  .superRefine((v, ctx) => {
    const who = v.who.replace(/^@/, "");
    if (!who) {
      ctx.addIssue({ code: "custom", path: ["who"], message: `workspaces:members.invite.${v.target}Required` });
    } else if (v.target === "email" && !EMAIL.test(who)) {
      ctx.addIssue({ code: "custom", path: ["who"], message: "workspaces:members.invite.emailInvalid" });
    }
  });
type Values = z.infer<typeof schema>;

const DEFAULTS: Values = { target: "username", who: "", role: "member" };

/** Codes about the person named, shown under the field rather than above the form. */
const WHO_CODES = new Set(["already_member"]);

function whoErrorOf(error: unknown): string | null {
  if (!isApiError(error)) return null;
  if (WHO_CODES.has(error.code)) return apiErrorKey(error);
  const listed = error.fields?.username?.[0] ?? error.fields?.invited_email?.[0];
  return listed ? fieldErrorKey(listed) : null;
}

function toRequest(values: Values): CreateWorkspaceInvitationRequest {
  const who = values.who.trim().replace(/^@/, "");
  return values.target === "email" ? { invited_email: who, role: values.role } : { username: who, role: values.role };
}

/**
 * One person outside the team, as a guest, by username or by address. The answer is the same
 * whether an account holds the address or not, and the invitation waits 7 days for theirs.
 */
export function InviteToWorkspaceDialog({ workspace, trigger }: Readonly<{ workspace: WorkspaceResponse; trigger: ReactNode }>) {
  const { t } = useTranslation(["workspaces", "common", "errors"]);
  const create = useCreateWorkspaceInvitation();
  const [open, setOpen] = useState(false);
  const form = useForm<Values>({ resolver: zodResolver(schema), defaultValues: DEFAULTS });
  const target = useWatch({ control: form.control, name: "target" });
  const { errors } = form.formState;
  const byEmail = target === "email";

  const changeOpen = (next: boolean) => {
    setOpen(next);
    if (!next) {
      form.reset(DEFAULTS);
      create.reset();
    }
  };

  const switchTarget = (value: unknown) => {
    if (!isTarget(value)) return;
    form.setValue("target", value);
    form.setValue("who", "");
    form.clearErrors("who");
    create.reset();
  };

  const submit = (values: Values) => {
    const body = toRequest(values);
    const shown = body.invited_email ?? `@${body.username}`;
    create.mutate(
      { slug: workspace.slug, body },
      {
        onSuccess: () => {
          toast.success(t("workspaces:members.invite.sent", { who: shown }));
          changeOpen(false);
        },
        onError: (error) => {
          const message = whoErrorOf(error);
          if (message) form.setError("who", { message }, { shouldFocus: true });
        },
      },
    );
  };
  const unplaced = create.error && !whoErrorOf(create.error) ? create.error : null;

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogTrigger asChild>{trigger}</DialogTrigger>
      <DialogContent className="sm:max-w-md">
        <form noValidate className="flex flex-col gap-4" onSubmit={(event) => void form.handleSubmit(submit)(event)}>
          <DialogHeader>
            <DialogTitle>{t("workspaces:members.invite.title", { name: workspace.name })}</DialogTitle>
            <DialogDescription>{t("workspaces:members.invite.description")}</DialogDescription>
          </DialogHeader>

          {unplaced ? <FormError error={unplaced} /> : null}

          <Tabs value={target} onValueChange={switchTarget}>
            <TabsList aria-label={t("workspaces:members.invite.by")}>
              <TabsTab value="username">{t("workspaces:members.invite.byUsername")}</TabsTab>
              <TabsTab value="email">{t("workspaces:members.invite.byEmail")}</TabsTab>
            </TabsList>
          </Tabs>

          <Field data-invalid={!!errors.who}>
            <FieldLabel htmlFor="ws-invite-who">
              {t(byEmail ? "workspaces:members.invite.email" : "workspaces:members.invite.username")}
            </FieldLabel>
            <InputGroup>
              {byEmail ? null : (
                <InputGroupAddon>
                  <span className="font-mono text-text-subtle">@</span>
                </InputGroupAddon>
              )}
              <InputGroupInput
                id="ws-invite-who"
                type={byEmail ? "email" : "text"}
                autoComplete="off"
                spellCheck={false}
                className={byEmail ? undefined : "font-mono"}
                placeholder={t(byEmail ? "workspaces:members.invite.emailPlaceholder" : "workspaces:members.invite.usernamePlaceholder")}
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
              <Field>
                <FieldLabel id="ws-invite-role-label" htmlFor="ws-invite-role">
                  {t("workspaces:members.invite.role")}
                </FieldLabel>
                <Select value={field.value} onValueChange={field.onChange}>
                  <SelectTrigger id="ws-invite-role" aria-labelledby="ws-invite-role-label" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {GUEST_ROLES.map((role) => (
                      <SelectItem key={role} value={role}>
                        {t(`common:roles.${role}`)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <FieldDescription>{t("workspaces:members.invite.roleHint")}</FieldDescription>
              </Field>
            )}
          />

          <p className="flex gap-2 rounded-md bg-info/10 px-3 py-2 text-xs text-text-muted">
            <MailIcon aria-hidden className="mt-0.5 size-3.5 shrink-0" />
            {t("workspaces:members.invite.mailHint")}
          </p>

          <DialogFooter>
            <DialogClose asChild>
              <Button type="button" variant="ghost">
                {t("common:actions.cancel")}
              </Button>
            </DialogClose>
            <Button type="submit" variant="primary" disabled={create.isPending}>
              {t("workspaces:members.invite.submit")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
