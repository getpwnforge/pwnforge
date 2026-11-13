import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { OptionCard } from "@/components/ui/option-card";

function renderPair(onChange = vi.fn(), disabled = false) {
  render(
    <fieldset>
      <OptionCard name="mode" value="solo" checked onChange={onChange} title="Solo" description="Just you" />
      <OptionCard name="mode" value="team" checked={false} onChange={onChange} title="Team" disabled={disabled} />
    </fieldset>,
  );
  return onChange;
}

describe("OptionCard", () => {
  it("is a radio named by its title, with its description", () => {
    renderPair();
    expect(screen.getByRole("radio", { name: /Solo/ })).toBeChecked();
    expect(screen.getByText("Just you")).toBeInTheDocument();
  });

  it("reports its value when chosen", async () => {
    const onChange = renderPair();
    await userEvent.click(screen.getByText("Team"));
    expect(onChange).toHaveBeenCalledWith("team");
  });

  it("cannot be chosen when disabled", async () => {
    const onChange = renderPair(vi.fn(), true);
    expect(screen.getByRole("radio", { name: /Team/ })).toBeDisabled();
    await userEvent.click(screen.getByText("Team"));
    expect(onChange).not.toHaveBeenCalled();
  });
});
