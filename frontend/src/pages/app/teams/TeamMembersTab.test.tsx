import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import i18n from "@/lib/i18n";
import { ME_ID, teamMember } from "@/test/api-fixtures";
import { lastBody } from "@/test/fake-api";
import { ALPHA, renderTeam } from "@/test/render-team";

const t = (key: string, options: Record<string, unknown> = {}) => i18n.t(key, options);
const NORA_ID = "0000000f-0000-7000-8000-000000000009";
const me = (role: "captain" | "member") => teamMember({ role });
const nora = teamMember({ user_id: NORA_ID, username: "nora", first_name: "Nora", last_name: "", role: "member", invited_by: ME_ID });

const actionsFor = (name: string) => screen.queryByRole("button", { name: t("teams:members.actionsFor", { name }) });
const rowOf = async (handle: string) => {
  const row = (await screen.findByText(`@${handle}`)).closest("tr");
  if (!row) throw new Error(`no row for @${handle}`);
  return row;
};

describe("TeamMembersTab", () => {
  it("lists the members with their name, handle, role and who invited them", async () => {
    renderTeam("", { "GET /teams/alpha-k2m4/members": { status: 200, data: [me("captain"), nora] } });
    const row = await rowOf("nora");
    expect(within(row).getByText(/Nora/)).toBeInTheDocument();
    expect(within(row).getByText(t("common:roles.member"))).toBeInTheDocument();
    expect(within(row).getByText("@enzo")).toBeInTheDocument();
  });

  it("filters the members by name or handle", async () => {
    const user = userEvent.setup();
    renderTeam("", { "GET /teams/alpha-k2m4/members": { status: 200, data: [me("captain"), nora] } });
    await rowOf("nora");

    await user.type(screen.getByRole("searchbox", { name: t("teams:members.filter") }), "nor");
    expect(screen.queryByText("@enzo", { selector: "div" })).not.toBeInTheDocument();
    expect(screen.getByText("@nora")).toBeInTheDocument();
  });

  it("lets the Captain change a role", async () => {
    const user = userEvent.setup();
    const { server } = renderTeam("", {
      "GET /teams/alpha-k2m4/members": { status: 200, data: [me("captain"), nora] },
      [`PATCH /teams/alpha-k2m4/members/${NORA_ID}`]: { status: 200, data: { ...nora, role: "admin" } },
    });
    await rowOf("nora");

    await user.click(actionsFor("Nora")!);
    await user.click(screen.getByRole("menuitem", { name: t("teams:members.setRole", { role: t("common:roles.admin") }) }));

    await waitFor(() => expect(lastBody(server, `PATCH /teams/alpha-k2m4/members/${NORA_ID}`)).toEqual({ role: "admin" }));
  });

  it("lets the Captain remove a member after confirming", async () => {
    const user = userEvent.setup();
    const { server } = renderTeam("", {
      "GET /teams/alpha-k2m4/members": { status: 200, data: [me("captain"), nora] },
      [`DELETE /teams/alpha-k2m4/members/${NORA_ID}`]: { status: 204 },
    });
    await rowOf("nora");

    await user.click(actionsFor("Nora")!);
    await user.click(screen.getByRole("menuitem", { name: t("teams:members.remove") }));
    expect(server.count(`DELETE /teams/alpha-k2m4/members/${NORA_ID}`)).toBe(0);
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: t("teams:members.remove") }));

    await waitFor(() => expect(server.count(`DELETE /teams/alpha-k2m4/members/${NORA_ID}`)).toBe(1));
  });

  it("offers nothing on the Captain's own row", async () => {
    renderTeam("", { "GET /teams/alpha-k2m4/members": { status: 200, data: [me("captain"), nora] } });
    await rowOf("nora");
    expect(actionsFor("Enzo P")).toBeNull();
  });

  it("gives a Member nothing on others, and Leave on their own row", async () => {
    const user = userEvent.setup();
    const { server } = renderTeam("", {
      "GET /teams/alpha-k2m4/members": { status: 200, data: [me("member"), { ...nora, role: "captain" }] },
      [`DELETE /teams/alpha-k2m4/members/${ME_ID}`]: { status: 204 },
    }, { team: { ...ALPHA, my_role: "member" } });
    await rowOf("nora");
    expect(actionsFor("Nora")).toBeNull();

    await user.click(actionsFor("Enzo P")!);
    await user.click(screen.getByRole("menuitem", { name: t("teams:members.leave") }));
    await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: t("teams:members.leave") }));

    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent(/^\/teams$/));
    expect(server.count(`DELETE /teams/alpha-k2m4/members/${ME_ID}`)).toBe(1);
  });

  it("links the roles card to the Roles tab", async () => {
    renderTeam("", { "GET /teams/alpha-k2m4/members": { status: 200, data: [me("captain"), nora] } });
    await rowOf("nora");
    const card = within(screen.getByRole("region", { name: t("teams:roles.title") }));
    expect(card.getByRole("link", { name: t("teams:roles.seeDetail") })).toHaveAttribute("href", "/teams/alpha-k2m4?tab=roles");
  });
});
