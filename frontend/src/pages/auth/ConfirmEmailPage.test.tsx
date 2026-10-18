// src/pages/auth/ConfirmEmailPage.test.tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { describe, expect, it, vi } from "vitest";

import * as authApi from "@/api/auth";
import i18n from "@/lib/i18n";
import { AuthContext, type AuthContextValue } from "@/lib/auth";
import { ConfirmEmailPage } from "@/pages/auth/ConfirmEmailPage";
import { fakeApi } from "@/test/fake-api";

/** The page, with the real API calls for what it uses and nothing else. */
function renderPage() {
  const auth: AuthContextValue = {
    user: null,
    status: "anonymous",
    confirmEmail: authApi.confirmEmail,
    resendVerification: authApi.resendVerification,
    login: vi.fn(),
    register: vi.fn(),
    logout: vi.fn(),
    forgotPassword: vi.fn(),
    resetPassword: vi.fn(),
    reload: vi.fn(),
  };

  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { mutations: { retry: false } } })}>
      <AuthContext value={auth}>
        <MemoryRouter>
          <ConfirmEmailPage />
        </MemoryRouter>
      </AuthContext>
    </QueryClientProvider>,
  );
}

describe("ConfirmEmailPage", () => {
  it("sends nothing on load, then verifies once on the click", async () => {
    window.history.replaceState(null, "", "/auth/email/verify#token=abc123");
    const server = fakeApi({ "POST /auth/email/verify": { status: 204 } });
    const user = userEvent.setup();

    renderPage();
    const confirm = await screen.findByRole("button", {
      name: i18n.t("auth:confirmEmail.prompt.confirm"),
    });

    // A link scanner that runs the page's script stops here.
    expect(server.count("POST /auth/email/verify")).toBe(0);

    await user.dblClick(confirm);

    expect(await screen.findByText(i18n.t("auth:confirmEmail.success.title"))).toBeInTheDocument();
    expect(server.requests).toEqual([
      { method: "POST", url: "/auth/email/verify", body: { token: "abc123" } },
    ]);
  });

  it("offers no verification without a token", async () => {
    window.history.replaceState(null, "", "/auth/email/verify");
    const server = fakeApi({});

    renderPage();

    expect(
      await screen.findByText(i18n.t("auth:confirmEmail.error.missingToken")),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: i18n.t("auth:confirmEmail.prompt.confirm") }),
    ).not.toBeInTheDocument();
    expect(server.requests).toEqual([]);
  });
});
