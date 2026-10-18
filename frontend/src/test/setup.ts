// src/test/setup.ts
import "@testing-library/jest-dom/vitest";
import { act, cleanup } from "@testing-library/react";
import { afterEach, beforeAll, vi } from "vitest";
import type * as Zustand from "zustand";

import i18n from "@/lib/i18n";
import { queryClient } from "@/lib/query-client";

/** One per store created while a test file runs, to put it back after each test. */
const storeResets = vi.hoisted(() => new Set<() => void>());

// Every zustand store, wrapped so that it starts each test from its initial
// state, the way zustand's own testing guide does it for Vitest.
vi.mock("zustand", async (importOriginal) => {
  const actual = await importOriginal<typeof Zustand>();

  const tracked = <S extends { getInitialState: () => unknown; setState: (...args: never[]) => void }>(
    store: S,
  ): S => {
    const initial = store.getInitialState();
    storeResets.add(() => (store.setState as (state: unknown, replace: true) => void)(initial, true));
    return store;
  };

  const create = ((initializer?: Zustand.StateCreator<unknown>) => {
    const build = (init: Zustand.StateCreator<unknown>) => tracked(actual.create(init));
    return initializer ? build(initializer) : build;
  }) as typeof actual.create;

  const createStore = ((initializer?: Zustand.StateCreator<unknown>) => {
    const build = (init: Zustand.StateCreator<unknown>) => tracked(actual.createStore(init));
    return initializer ? build(initializer) : build;
  }) as typeof actual.createStore;

  return { ...actual, create, createStore };
});

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

// What one test leaves behind must not reach the next: the rendered tree, the
// stores, the shared query cache, both storages and the address bar.
afterEach(() => {
  cleanup();
  act(() => {
    for (const reset of storeResets) reset();
  });
  queryClient.clear();
  localStorage.clear();
  sessionStorage.clear();
  window.history.replaceState(null, "", "/");
});
