import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { WorkspaceStatusBadge } from "@/components/workspaces/WorkspaceStatusBadge";
import i18n from "@/lib/i18n";

const ws = (over: { archived?: boolean; starts_at?: string | null; ends_at?: string | null }) =>
  ({ archived: false, starts_at: null, ends_at: null, ...over });

describe("WorkspaceStatusBadge", () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date("2026-10-28T12:00:00Z"));
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it.each([
    ["archived", ws({ archived: true, starts_at: "2026-10-01T00:00:00Z" })],
    ["permanent", ws({})],
    ["upcoming", ws({ starts_at: "2026-11-01T00:00:00Z" })],
    ["live", ws({ starts_at: "2026-10-27T00:00:00Z", ends_at: "2026-10-29T00:00:00Z" })],
    ["ended", ws({ starts_at: "2026-10-01T00:00:00Z", ends_at: "2026-10-02T00:00:00Z" })],
  ])("says %s", (status, workspace) => {
    render(<WorkspaceStatusBadge workspace={workspace} />);
    expect(screen.getByText(i18n.t(`workspaces:status.${status}`))).toBeInTheDocument();
  });
});
