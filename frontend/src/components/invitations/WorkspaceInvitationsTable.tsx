import { AtSignIcon, MailIcon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import { FormError } from "@/components/common/FormError";
import { RoleBadge } from "@/components/members/RoleBadge";
import { Button } from "@/components/ui/button";
import { RelativeTime } from "@/components/ui/relative-time";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { useCancelWorkspaceInvitation, useWorkspaceInvitations } from "@/hooks/useInvitations";
import { apiErrorKey } from "@/lib/api-error";
import type { WorkspaceInvitationResponse, WorkspaceResponse } from "@/lib/api/types";
import { canCancelWorkspaceInvitation, satisfies } from "@/lib/permissions";

/** Stands for "every author" in the filter: Radix Select has no empty item. */
const ALL = "all";

/** Whom an invitation names: an address while it is pending, or an account. */
function recipientOf(invitation: WorkspaceInvitationResponse): { byEmail: boolean; label: string } {
  if (invitation.invited_email) return { byEmail: true, label: invitation.invited_email };
  return { byEmail: false, label: `@${invitation.invited_username ?? "—"}` };
}

type Props = Readonly<{ workspace: WorkspaceResponse; me: string }>;

/**
 * The invitations still waiting for an answer. The API lists all of them to the Captain and the
 * Admins, who can narrow them to one author, and only their own to a Member.
 */
export function WorkspaceInvitationsTable({ workspace, me }: Props) {
  const { t } = useTranslation(["workspaces", "common", "errors"]);
  const invitations = useWorkspaceInvitations(workspace.slug);
  const cancel = useCancelWorkspaceInvitation();
  const [author, setAuthor] = useState<string>(ALL);
  const seesAll = satisfies(workspace.my_role, "admin");

    // The API lists the history too: accepted, declined, cancelled, and expired once past their date.
  const list = (invitations.data ?? []).filter((invitation) => invitation.status === "pending");
  const authors = new Map<string, string>();
  for (const invitation of list) {
    if (invitation.invited_by) authors.set(invitation.invited_by, invitation.invited_by_username ?? "—");
  }
  const shown = author === ALL ? list : list.filter((invitation) => invitation.invited_by === author);

  return (
    <section aria-labelledby="ws-invitations-title" className="rounded-lg border border-border bg-bg-elev">
      <div className="flex flex-wrap items-center gap-2 border-b border-border px-3.5 py-2.5">
        <div className="min-w-0 flex-1">
          <h2 id="ws-invitations-title" className="text-sm font-semibold text-text">
            {t("workspaces:members.invitations.title")}
            {invitations.isSuccess ? <span className="ml-1.5 font-normal tabular-nums text-text-muted">{list.length}</span> : null}
          </h2>
          <p className="mt-0.5 text-xs text-text-muted">
            {t(seesAll ? "workspaces:members.invitations.hintAll" : "workspaces:members.invitations.hintMine")}
          </p>
        </div>
        {seesAll && authors.size > 1 ? (
          <Select value={author} onValueChange={setAuthor}>
            <SelectTrigger className="w-48" aria-label={t("workspaces:members.invitations.authorLabel")}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={ALL}>{t("workspaces:members.invitations.allAuthors")}</SelectItem>
              {[...authors].map(([id, username]) => (
                <SelectItem key={id} value={id}>
                  {id === me ? t("workspaces:members.invitations.you") : `@${username}`}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        ) : null}
      </div>

      {invitations.isError ? (
        <div className="p-4">
          <FormError error={invitations.error} />
        </div>
      ) : null}

      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{t("workspaces:members.invitations.columns.recipient")}</TableHead>
            <TableHead>{t("workspaces:members.columns.role")}</TableHead>
            <TableHead>{t("workspaces:members.invitations.columns.invitedBy")}</TableHead>
            <TableHead>{t("workspaces:members.invitations.columns.expires")}</TableHead>
            <TableHead className="w-28">
              <span className="sr-only">{t("workspaces:members.columns.actions")}</span>
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {invitations.isPending ? (
            <TableRow>
              <TableCell colSpan={5}>
                <Skeleton className="h-8" />
              </TableCell>
            </TableRow>
          ) : null}
          {shown.map((invitation) => {
            const { byEmail, label } = recipientOf(invitation);
            return (
              <TableRow key={invitation.id}>
                <TableCell>
                  <span className="flex items-center gap-2">
                    {byEmail ? <MailIcon aria-hidden className="size-4 text-text-muted" /> : <AtSignIcon aria-hidden className="size-4 text-text-muted" />}
                    <span className={byEmail ? "text-sm text-text" : "font-mono text-sm text-text"}>{label}</span>
                  </span>
                </TableCell>
                <TableCell>
                  <RoleBadge role={invitation.role} />
                </TableCell>
                <TableCell className="text-xs text-text-muted">
                  {invitation.invited_by === me
                    ? t("workspaces:members.invitations.you")
                    : `@${invitation.invited_by_username ?? "—"}`}
                </TableCell>
                <TableCell className="text-xs text-text-muted">
                  <RelativeTime timestamp={invitation.expires_at} />
                </TableCell>
                <TableCell className="text-right">
                  {canCancelWorkspaceInvitation(workspace.my_role, me, invitation.invited_by) ? (
                    <ConfirmDialog
                      trigger={
                        <Button variant="ghost" size="sm" className="text-danger-text">
                          {t("workspaces:members.invitations.cancel")}
                        </Button>
                      }
                      title={t("workspaces:members.invitations.cancelTitle")}
                      description={t("workspaces:members.invitations.cancelText", { who: label })}
                      confirmLabel={t("workspaces:members.invitations.cancel")}
                      danger
                      pending={cancel.isPending}
                      onConfirm={() =>
                        cancel.mutate(
                          { slug: workspace.slug, invitationId: invitation.id },
                          {
                            onSuccess: () => toast.success(t("workspaces:members.invitations.cancelled")),
                            onError: (error) => toast.error(t(apiErrorKey(error))),
                          },
                        )
                      }
                    />
                  ) : null}
                </TableCell>
              </TableRow>
            );
          })}
          {invitations.isSuccess && shown.length === 0 ? (
            <TableRow>
              <TableCell colSpan={5} className="py-8 text-center text-sm text-text-muted">
                {t("workspaces:members.invitations.empty")}
              </TableCell>
            </TableRow>
          ) : null}
        </TableBody>
      </Table>
    </section>
  );
}
