import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
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
import { Field, FieldLabel } from "@/components/ui/field";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useAuth } from "@/hooks/useAuth";
import { useTeamMembers, useTransferCaptaincy } from "@/hooks/useTeams";
import type { TeamResponse } from "@/lib/api/types";
import { displayName } from "@/lib/display-name";

/** Hands the Captain role to another member; I become Admin. */
export function TransferCaptaincyDialog({ team }: Readonly<{ team: TeamResponse }>) {
  const { t } = useTranslation(["teams"]);
  const { user } = useAuth();
  const members = useTeamMembers(team.slug);
  const transfer = useTransferCaptaincy();
  const [open, setOpen] = useState(false);
  const [target, setTarget] = useState("");
  const candidates = (members.data ?? []).filter((m) => m.user_id !== user?.id);

  const changeOpen = (next: boolean) => {
    if (!next) {
      setTarget("");
      transfer.reset();
    }
    setOpen(next);
  };

  const submit = () =>
    transfer.mutate(
      { slug: team.slug, userId: target },
      {
        onSuccess: () => {
          changeOpen(false);
          toast.success(t("teams:settings.transferred"));
        },
      },
    );

  return (
    <Dialog open={open} onOpenChange={changeOpen}>
      <DialogTrigger asChild>
        <Button variant="secondary" size="sm">
          {t("teams:settings.transfer")}
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("teams:settings.transferTitle")}</DialogTitle>
          <DialogDescription>{t("teams:settings.transferText")}</DialogDescription>
        </DialogHeader>

        {/* target_pending_deletion, target_already_captain and the rest, translated. */}
        {transfer.error ? <FormError error={transfer.error} /> : null}

        <Field>
          <FieldLabel id="transfer-target-label" htmlFor="transfer-target">
            {t("teams:settings.transferTarget")}
          </FieldLabel>
          <Select value={target} onValueChange={setTarget}>
            <SelectTrigger id="transfer-target" aria-labelledby="transfer-target-label" className="w-full">
              <SelectValue placeholder={t("teams:settings.transferPlaceholder")} />
            </SelectTrigger>
            <SelectContent>
              {candidates.map((m) => (
                <SelectItem key={m.user_id} value={m.user_id}>
                  {displayName(m)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>

        <DialogFooter>
          <DialogClose asChild>
            <Button type="button" variant="ghost">
              {t("teams:settings.cancel")}
            </Button>
          </DialogClose>
          <Button disabled={!target || transfer.isPending} onClick={submit}>
            {t("teams:settings.transferConfirm")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
