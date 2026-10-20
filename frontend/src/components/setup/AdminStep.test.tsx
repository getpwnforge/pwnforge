// src/components/setup/AdminStep.test.tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { AdminStep } from "@/components/setup/AdminStep";
import i18n from "@/lib/i18n";
import { fakeApi, type FakeRoute } from "@/test/fake-api";

/** The step, with the real dry-run call answered by `answer`. */
function renderStep(answer: FakeRoute) {
  const server = fakeApi({ "POST /setup/validate-admin": answer });
  const onNext = vi.fn();

  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })}
    >
      <AdminStep token="a-setup-token" defaultValues={null} onBack={vi.fn()} onNext={onNext} />
    </QueryClientProvider>,
  );
  return { server, onNext };
}

async function submit() {
  const user = userEvent.setup();
  await user.type(screen.getByLabelText(i18n.t("common:fields.handle")), "operator");
  await user.type(screen.getByLabelText(i18n.t("common:fields.email")), "operator@example.test");
  await user.type(screen.getByLabelText(i18n.t("common:fields.password")), "a-long-test-password-1!");
  await user.click(screen.getByRole("button", { name: i18n.t("common:actions.next") }));
}

describe("AdminStep", () => {
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

  it("puts a taken address under the email field and stays on the step", async () => {
    const { server, onNext } = renderStep({ status: 409, data: { error: "admin_email_taken" } });

    await submit();

    expect(await screen.findByText(i18n.t("errors:api.admin_email_taken"))).toBeInTheDocument();
    expect(screen.getByLabelText(i18n.t("common:fields.email"))).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByLabelText(i18n.t("common:fields.handle"))).toHaveAttribute("aria-invalid", "false");
    expect(server.count("POST /setup/validate-admin")).toBe(1);
    expect(onNext).not.toHaveBeenCalled();
  });

  it("has a message for a taken address in every language", () => {
    for (const lng of ["en", "fr"]) {
      expect(i18n.exists("errors:api.admin_email_taken", { lng })).toBe(true);
    }
  });

  it("goes on when the administrator would be accepted", async () => {
    const { onNext } = renderStep({ status: 204 });

    await submit();

    await waitFor(() => expect(onNext).toHaveBeenCalledOnce());
  });
});
