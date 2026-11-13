// src/test/setup.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { queryClient } from "@/lib/query-client";
import { useSidebarStore } from "@/stores/sidebar-store";

// Two tests that run in order: the first leaves something behind wherever a
// test can, the second checks that the setup took all of it away.
describe("the test setup", () => {
  it("lets a test leave things behind", () => {
    render(<p>left behind</p>);
    useSidebarStore.getState().toggle();
    queryClient.setQueryData(["left", "behind"], 1);
    localStorage.setItem("left", "behind");
    sessionStorage.setItem("left", "behind");
    window.history.replaceState(null, "", "/left?behind#here");

    expect(screen.getByText("left behind")).toBeInTheDocument();
    expect(useSidebarStore.getState().collapsed).toBe(true);
  });

  it("starts the next test from a clean slate", () => {
    expect(document.body).toBeEmptyDOMElement();
    expect(useSidebarStore.getState().collapsed).toBe(false);
    expect(queryClient.getQueryCache().getAll()).toEqual([]);
    expect(localStorage).toHaveLength(0);
    expect(sessionStorage).toHaveLength(0);
    expect(`${window.location.pathname}${window.location.search}${window.location.hash}`).toBe("/");
  });
});
