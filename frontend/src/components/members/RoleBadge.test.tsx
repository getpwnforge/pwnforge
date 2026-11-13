import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactElement } from "react";
import { describe, expect, it } from "vitest";

import { RoleBadge } from "@/components/members/RoleBadge";
import { TooltipProvider } from "@/components/ui/tooltip";
import i18n from "@/lib/i18n";

// A provider with no delay, so the tooltip shows on the first hover.
const renderBadge = (ui: ReactElement) => render(<TooltipProvider delayDuration={0}>{ui}</TooltipProvider>);

describe("RoleBadge", () => {
  it.each(["owner", "captain", "admin", "member", "viewer"] as const)("names the %s role", (role) => {
    renderBadge(<RoleBadge role={role} />);
    expect(screen.getByText(i18n.t(`common:roles.${role}`))).toBeInTheDocument();
  });

  it("says where an overridden role comes from", async () => {
    renderBadge(<RoleBadge role="viewer" source="override" />);
    await userEvent.hover(screen.getByText(i18n.t("common:roles.viewer")));
    expect(await screen.findByRole("tooltip")).toHaveTextContent(i18n.t("common:roleSources.override"));
  });

  it("adds nothing for a role inherited from the team", async () => {
    renderBadge(<RoleBadge role="member" source="team" />);
    await userEvent.hover(screen.getByText(i18n.t("common:roles.member")));
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
  });
});
