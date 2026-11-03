import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { ConfirmDialog } from "@/components/common/ConfirmDialog";
import i18n from "@/lib/i18n";

function renderDialog(props: { confirmText?: string; onConfirm?: () => void } = {}) {
  const onConfirm = props.onConfirm ?? vi.fn();
  render(
    <ConfirmDialog trigger={<button type="button">Delete</button>} title="Delete the team?" description="Its workspaces go too."
      confirmLabel="Delete for good" danger confirmText={props.confirmText} onConfirm={onConfirm} />,
  );
  return onConfirm;
}

describe("ConfirmDialog", () => {
  it("opens on its trigger and confirms", async () => {
    const onConfirm = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(screen.getByRole("alertdialog", { name: "Delete the team?" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Delete for good" }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it("does nothing on cancel", async () => {
    const onConfirm = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Delete" }));
    await userEvent.click(screen.getByRole("button", { name: i18n.t("common:actions.cancel") }));
    expect(onConfirm).not.toHaveBeenCalled();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("stays locked until the exact text is typed", async () => {
    const onConfirm = renderDialog({ confirmText: "Team Alpha" });
    await userEvent.click(screen.getByRole("button", { name: "Delete" }));
    const action = screen.getByRole("button", { name: "Delete for good" });
    const input = screen.getByRole("textbox");

    expect(action).toBeDisabled();
    await userEvent.type(input, "team alpha");
    expect(action).toBeDisabled(); // case matters
    await userEvent.clear(input);
    await userEvent.type(input, "Team Alpha");
    expect(action).toBeEnabled();
    await userEvent.click(action);
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it("forgets what was typed when closed", async () => {
    renderDialog({ confirmText: "Team Alpha" });
    await userEvent.click(screen.getByRole("button", { name: "Delete" }));
    await userEvent.type(screen.getByRole("textbox"), "Team Alpha");
    await userEvent.keyboard("{Escape}");
    await userEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(screen.getByRole("textbox")).toHaveValue("");
  });
});
