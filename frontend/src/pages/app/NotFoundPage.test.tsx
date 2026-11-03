import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route } from "react-router";
import { describe, expect, it } from "vitest";

import i18n from "@/lib/i18n";
import { NotFoundPage } from "@/pages/app/NotFoundPage";
import { renderRoute } from "@/test/render-route";

const t = (key: string) => i18n.t(key);

describe("NotFoundPage", () => {
  it("answers an unknown URL and leads to the workspaces", async () => {
    renderRoute("/nowhere", <Route path="*" element={<NotFoundPage />} />);
    expect(screen.getByText(t("common:notFound.title"))).toBeInTheDocument();

    await userEvent.click(screen.getByRole("link", { name: t("common:notFound.home") }));
    expect(screen.getByTestId("location")).toHaveTextContent("/workspaces");
  });

  it("offers no Back when the app has no previous page, so it never leaves the site", () => {
    renderRoute("/nowhere", <Route path="*" element={<NotFoundPage />} />);
    expect(screen.queryByRole("button", { name: t("common:notFound.back") })).not.toBeInTheDocument();
  });

  it("goes back when the previous page is in the app", async () => {
    // React Router numbers its entries in history.state.idx; above 0, the previous one is ours.
    window.history.replaceState({ idx: 1 }, "");
    renderRoute("/nowhere", <Route path="*" element={<NotFoundPage />} />);
    expect(screen.getByRole("button", { name: t("common:notFound.back") })).toBeInTheDocument();
  });

  it("takes the words a page gives it", () => {
    renderRoute("/x", <Route path="*" element={<NotFoundPage title="No such team" description="Or not yours" />} />);
    expect(screen.getByText("No such team")).toBeInTheDocument();
    expect(screen.getByText("Or not yours")).toBeInTheDocument();
  });
});
