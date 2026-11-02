import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription,
  AlertDialogFooter, AlertDialogHeader, AlertDialogTitle, AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Input } from "@/components/ui/input";

type Props = Readonly<{
  trigger: ReactNode;
  title: string;
  description: ReactNode;
  confirmLabel: string;
  danger?: boolean;
  /** When set, the action stays disabled until this exact text is typed. */
  confirmText?: string;
  pending?: boolean;
  onConfirm: () => void;
}>;

export function ConfirmDialog({ trigger, title, description, confirmLabel, danger, confirmText, pending, onConfirm }: Props) {
  const { t } = useTranslation();
  const [typed, setTyped] = useState("");
  const locked = confirmText !== undefined && typed !== confirmText;

  return (
    <AlertDialog onOpenChange={(open) => { if (!open) setTyped(""); }}>
      <AlertDialogTrigger asChild>{trigger}</AlertDialogTrigger>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        {confirmText !== undefined ? (
          <Input value={typed} onChange={(e) => setTyped(e.target.value)}
            aria-label={t("common:confirm.typeToConfirm", { text: confirmText })} />
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel>{t("common:actions.cancel")}</AlertDialogCancel>
          <AlertDialogAction disabled={locked || pending} onClick={onConfirm}
            className={danger ? "bg-destructive text-destructive-foreground hover:bg-destructive/90" : ""}
            >{confirmLabel}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
