// src/pages/auth/RegisterPage.test.tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import * as authApi from "@/api/auth";
import { API_ERROR_CODES } from "@/lib/api-error";
import { AuthContext, type AuthContextValue } from "@/lib/auth";
import i18n from "@/lib/i18n";
import { RegisterPage } from "@/pages/auth/RegisterPage";
import { fakeApi, type FakeRoute } from "@/test/fake-api";

const ADDRESS = "player@example.test";

/** The page, with the real registration call and what it loads first. */
function renderPage(answer: FakeRoute) {
  const server = fakeApi({
    "GET /instance/config": { status: 200, data: { hide_landing_page: false, turnstile_site_key: null } },
    "GET /legal/versions": { status: 200, data: { terms_version: "1.0", privacy_version: "1.0" } },
    "POST /auth/register": answer,
  });
  const auth: AuthContextValue = {
    user: null,
    status: "anonymous",
    register: authApi.register,
    login: vi.fn(),
    logout: vi.fn(),
    confirmEmail: vi.fn(),
    resendVerification: vi.fn(),
    forgotPassword: vi.fn(),
    resetPassword: vi.fn(),
    reload: vi.fn(),
  };

  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })}
    >
      <AuthContext value={auth}>
        <MemoryRouter>
          <RegisterPage />
        </MemoryRouter>
      </AuthContext>
    </QueryClientProvider>,
  );
  return server;
}

async function register() {
  const user = userEvent.setup();
  await user.type(screen.getByLabelText(i18n.t("common:fields.handle")), "new-player");
  await user.type(screen.getByLabelText(i18n.t("common:fields.email")), ADDRESS);
  await user.type(screen.getByLabelText(i18n.t("common:fields.password")), "a-long-test-password-1!");
  await user.click(screen.getByRole("checkbox"));
  const submit = screen.getByRole("button", { name: i18n.t("auth:register.submit") });
  await waitFor(() => expect(submit).toBeEnabled());
  await user.click(submit);
}

describe("RegisterPage", () => {
  // jsdom has none, and a component of the form measures itself with it.
  beforeEach(() => {
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        unobserve() {}
        disconnect() {}
      },
    );
  });

  it("sends to the inbox, and tells an existing account it gets a mail to sign in", async () => {
    const server = renderPage({ status: 202 });

    await register();

    expect(await screen.findByText(i18n.t("auth:register.success.title"))).toBeInTheDocument();
    expect(
      screen.getByText(i18n.t("auth:register.success.description", { email: ADDRESS })),
    ).toBeInTheDocument();
    expect(screen.getByText(i18n.t("auth:register.success.existingAccount"))).toBeInTheDocument();
    expect(server.count("POST /auth/register")).toBe(1);
  });

  // A backend from before the change would still answer this: the page says
  // nothing of its own about the address, only what it says for any failure.
  it("says nothing about the address when a backend reports it taken", async () => {
    renderPage({ status: 409, data: { error: "email_taken" } });

    await register();

    expect(await screen.findByText(i18n.t("errors:api.internal_error"))).toBeInTheDocument();
    expect(screen.getByLabelText(i18n.t("common:fields.email"))).toHaveAttribute("aria-invalid", "false");
  });

  it("knows no code or message for a taken address", () => {
    expect(API_ERROR_CODES).not.toContain("email_taken");
    for (const lng of ["en", "fr"]) {
      expect(i18n.exists("errors:api.email_taken", { lng })).toBe(false);
    }
  });
});
