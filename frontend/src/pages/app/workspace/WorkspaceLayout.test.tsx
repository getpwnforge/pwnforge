import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import i18n from "@/lib/i18n";
import { deferred, type FakeReply } from "@/test/fake-api";
import { HEXAGON, renderWorkspace } from "@/test/render-workspace";

const t = (key: string, options: Record<string, unknown> = {}) => i18n.t(key, options);
const DETAIL = `GET /workspaces/${HEXAGON.slug}`;

describe("WorkspaceLayout", () => {
  it("waits for the workspace before showing a page", async () => {
    const reply = deferred<FakeReply>();
    const { container } = renderWorkspace("", { [DETAIL]: () => reply.promise });
    expect(container.querySelector("[aria-busy='true']")).toBeInTheDocument();

    reply.resolve({ status: 200, data: HEXAGON });
    expect(await screen.findByRole("heading", { name: "CTF Hexagon" })).toBeInTheDocument();
  });

  it("gives one message for a missing workspace, one out of reach and an exclusion", async () => {
    renderWorkspace("", { [DETAIL]: { status: 404, data: { error: "not_found" } } });
    expect(await screen.findByText(t("workspaces:notFound.title"))).toBeInTheDocument();
  });

  it("shows any other error as it came", async () => {
    renderWorkspace("", { [DETAIL]: { status: 503, data: { error: "service_unavailable" } } });
    expect(await screen.findByText(t("errors:api.service_unavailable"))).toBeInTheDocument();
  });

  it("says an archived workspace is read-only", async () => {
    renderWorkspace("", {}, { workspace: { ...HEXAGON, archived: true } });
    expect(await screen.findByText(t("workspaces:archived.banner"))).toBeInTheDocument();
  });

  it("shows no banner on a live workspace", async () => {
    renderWorkspace("");
    await screen.findByRole("heading", { name: "CTF Hexagon" });
    expect(screen.queryByText(t("workspaces:archived.banner"))).not.toBeInTheDocument();
  });
});
