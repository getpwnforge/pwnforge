// src/pages/public/ContactPage.test.tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import i18n from "@/lib/i18n";
import { ContactPage } from "@/pages/public/ContactPage";
import { fakeApi } from "@/test/fake-api";

/** The page, Turnstile off, its submission answered by `answer`. */
function renderPage(answer: () => Promise<Response>) {
  fakeApi({
    "GET /instance/config": { status: 200, data: { hide_landing_page: false, turnstile_site_key: null } },
  });
  const sent = vi.fn(answer);
  vi.stubGlobal("fetch", sent);

  render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <ContactPage />
    </QueryClientProvider>,
  );
  return sent;
}

/** A JSON answer, as the API sends its errors. */
function answer(status: number, body: unknown) {
  return () => Promise.resolve(new Response(JSON.stringify(body), { status }));
}

async function send() {
  const user = userEvent.setup();
  await user.type(screen.getByLabelText(i18n.t("public:contact.name")), "A visitor");
  await user.type(screen.getByLabelText(i18n.t("public:contact.email")), "visitor@example.test");
  await user.type(screen.getByLabelText(i18n.t("public:contact.message")), "A message long enough.");
  const submit = screen.getByRole("button", { name: i18n.t("public:contact.submit") });
  await waitFor(() => expect(submit).toBeEnabled());
  await user.click(submit);
}

describe("ContactPage", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it.each([
    ["a mail service that did not take the message", 502, "email_send_failed", "public:contact.sendFailed"],
    ["a failed bot check", 400, "turnstile_failed", "errors:api.turnstile_failed"],
    ["a failure of the server's own", 500, "internal_error", "public:contact.error"],
    ["a code this page does not know", 409, "constructor", "public:contact.error"],
  ])("says so for %s", async (_case, status, code, message) => {
    const sent = renderPage(answer(status, { error: code }));

    await send();

    expect(await screen.findByText(i18n.t(message))).toBeInTheDocument();
    expect(sent).toHaveBeenCalledOnce();
  });

  it("keeps the generic message when the request never reaches the API", async () => {
    renderPage(() => Promise.reject(new TypeError("Failed to fetch")));

    await send();

    expect(await screen.findByText(i18n.t("public:contact.error"))).toBeInTheDocument();
  });

  it("thanks the visitor once the message is sent", async () => {
    renderPage(() => Promise.resolve(new Response(null, { status: 204 })));

    await send();

    expect(await screen.findByText(i18n.t("public:contact.successTitle"))).toBeInTheDocument();
  });
});
