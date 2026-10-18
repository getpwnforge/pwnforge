// src/test/fake-api.ts
import { AxiosError, type AxiosAdapter, type AxiosResponse } from "axios";
import { onTestFinished } from "vitest";

import { api } from "@/lib/api";

/** What the fake server answers: a status, and a JSON body if any. */
export type FakeReply = { status: number; data?: unknown };

/** A reply, or a function computing it, possibly later. */
export type FakeRoute = FakeReply | (() => FakeReply | Promise<FakeReply>);

/** One request as the app sent it. */
export type FakeRequest = { method: string; url: string; body: unknown };

/**
 * Answers the app's axios instance from `routes`, keyed `"METHOD /path"`,
 * instead of the network, and records every request in the order it was
 * sent. The real interceptors run as they would in the browser: only the
 * transport is replaced, and it is put back when the test ends.
 *
 * A request with no route is answered 599, so that it shows up as a failure
 * instead of hanging.
 */
export function fakeApi(routes: Record<string, FakeRoute>) {
  const requests: FakeRequest[] = [];
  const previous = api.defaults.adapter;

  const adapter: AxiosAdapter = async (config) => {
    const method = (config.method ?? "get").toUpperCase();
    const url = config.url ?? "";
    const body = typeof config.data === "string" ? JSON.parse(config.data) : (config.data ?? null);
    requests.push({ method, url, body });

    const route = routes[`${method} ${url}`];
    const reply = route === undefined ? { status: 599 } : typeof route === "function" ? await route() : route;
    const response: AxiosResponse = {
      data: reply.data ?? null,
      status: reply.status,
      statusText: String(reply.status),
      headers: {},
      config,
      request: {},
    };

    if (reply.status >= 400) {
      throw new AxiosError(`fake ${reply.status}`, AxiosError.ERR_BAD_RESPONSE, config, {}, response);
    }
    return response;
  };

  api.defaults.adapter = adapter;
  onTestFinished(() => {
    api.defaults.adapter = previous;
  });

  return {
    requests,
    /** How many requests went to `"METHOD /path"`. */
    count: (key: string) => requests.filter(({ method, url }) => `${method} ${url}` === key).length,
  };
}

/** A promise resolved from outside, for an answer the test releases itself. */
export function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((settle) => {
    resolve = settle;
  });
  return { promise, resolve };
}
