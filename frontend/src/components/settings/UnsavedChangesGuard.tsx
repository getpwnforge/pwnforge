import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useBlocker } from "react-router";

import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";

/** What the guard calls to save: it then goes on, or stays if the save failed. */
export type SaveThen = (outcome: { onSaved: () => void; onFailed: () => void }) => void;

type Props = Readonly<{ dirty: boolean; saving: boolean; onSave: SaveThen }>;

/**
 * Holds any move to another page or section while the form has unsaved changes, and asks:
 * stay, discard, or save and go. Closing the tab gets the browser's own prompt instead.
 * Needs a data router (createBrowserRouter in the app, createMemoryRouter in tests).
 */
export function UnsavedChangesGuard({ dirty, saving, onSave }: Props) {
  const { t } = useTranslation(["workspaces"]);
  const blocker = useBlocker(
    ({ currentLocation, nextLocation }) =>
      dirty && (currentLocation.pathname !== nextLocation.pathname || currentLocation.search !== nextLocation.search),
  );

  useEffect(() => {
    if (!dirty) return;
    const hold = (event: BeforeUnloadEvent) => event.preventDefault();
    window.addEventListener("beforeunload", hold);
    return () => window.removeEventListener("beforeunload", hold);
  }, [dirty]);

  const blocked = blocker.state === "blocked";
  const stay = () => blocker.reset?.();
  const leave = () => blocker.proceed?.();
  const saveAndLeave = () => onSave({ onSaved: leave, onFailed: stay });

  return (
    <AlertDialog
      open={blocked}
      onOpenChange={(open) => {
        if (!open && !saving) stay();
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("workspaces:settings.unsavedTitle")}</AlertDialogTitle>
          <AlertDialogDescription>{t("workspaces:settings.unsavedText")}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={saving}>{t("workspaces:settings.unsavedStay")}</AlertDialogCancel>
          <Button type="button" variant="ghost" disabled={saving} onClick={leave}>
            {t("workspaces:settings.unsavedDiscard")}
          </Button>
          <Button type="button" variant="primary" disabled={saving} onClick={saveAndLeave}>
            {t("workspaces:settings.unsavedSave")}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
