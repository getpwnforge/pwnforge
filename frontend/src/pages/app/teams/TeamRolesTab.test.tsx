import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import i18n from "@/lib/i18n";
import { teamMember } from "@/test/api-fixtures";
import { ALPHA, renderTeam } from "@/test/render-team";

const t = (key: string, options?: Record<string, unknown>) => i18n.t(key, options);
const MEMBERS = {
  "GET /teams/alpha-k2m4/members": {
    status: 200,
    data: [teamMember(), teamMember({ user_id: "0000000f-0000-7000-8000-000000000009", username: "nora", first_name: "Nora", last_name: "", role: "member" })],
  },
};

/** The marks of one matrix row, Captain to Viewer: "allowed" or "denied". */
async function marks(rowKey: string): Promise<string[]> {
  const row = await screen.findByRole("row", { name: (name) => name.startsWith(t(`teams:roles.rows.${rowKey}`)) });
  return [...row.querySelectorAll("svg[aria-label]")].map((svg) =>
    svg.getAttribute("aria-label") === t("teams:roles.allowed") ? "allowed" : "denied",
  );
}

describe("TeamRolesTab", () => {
  it("counts the members of each role", async () => {
    renderTeam("?tab=roles", MEMBERS);
    // Captain and Member hold one each; Admin and Viewer none.
    expect(await screen.findAllByText(t("teams:card.members", { count: 1 }))).toHaveLength(2);
    expect(screen.getAllByText(t("teams:card.members", { count: 0 }))).toHaveLength(2);
  });

  it("reads the matrix from the permission rules", async () => {
    renderTeam("?tab=roles", MEMBERS);
    expect(await marks("deleteTeam")).toEqual(["allowed", "denied", "denied", "denied"]);
    expect(await marks("invite")).toEqual(["allowed", "allowed", "denied", "denied"]);
    expect(await marks("leave")).toEqual(["denied", "allowed", "allowed", "allowed"]);
    expect(await marks("view")).toEqual(["allowed", "allowed", "allowed", "allowed"]);
  });

  it("keeps workspace creation to the Captain and Admins by default", async () => {
    renderTeam("?tab=roles", MEMBERS);
    expect(await marks("createWorkspace")).toEqual(["allowed", "allowed", "denied", "denied"]);
  });

  it("opens workspace creation to Members when the team allows it", async () => {
    renderTeam("?tab=roles", MEMBERS, { team: { ...ALPHA, members_can_create_workspaces: true } });
    expect(await marks("createWorkspace")).toEqual(["allowed", "allowed", "allowed", "denied"]);
  });
});
