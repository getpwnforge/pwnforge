import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { ColorPicker } from "@/components/ui/color-picker";
import i18n from "@/lib/i18n";

const renderPicker = (onChange = vi.fn()) => {
  render(<ColorPicker name="color" label="Colour" value="ember" onChange={onChange} />);
  return onChange;
};

describe("ColorPicker", () => {
  it("offers the six colours, the current one checked", () => {
    renderPicker();
    expect(screen.getAllByRole("radio")).toHaveLength(6);
    expect(screen.getByRole("radio", { checked: true })).toHaveAttribute("value", "ember");
  });

  it("names each colour for screen readers", () => {
    renderPicker();
    expect(screen.getByRole("radio", { name: i18n.t("common:colors.blue") })).toBeInTheDocument();
  });

  it("reports a click", async () => {
    const onChange = renderPicker();
    await userEvent.click(screen.getByRole("radio", { name: i18n.t("common:colors.teal") }));
    expect(onChange).toHaveBeenCalledWith("teal");
  });

  it("moves with the arrow keys", async () => {
    const onChange = renderPicker();
    screen.getByRole("radio", { checked: true }).focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(onChange).toHaveBeenCalledWith("blue");
  });
});
