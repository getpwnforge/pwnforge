import { useRef, useState } from "react";
import type { TurnstileInstance } from "@marsidev/react-turnstile";

// Turnstile tokens are single-use: any failed submission burns the token, and
// retrying without resetting the widget fails with turnstile_failed rather
// than the real error. Centralised here so no page can forget the reset.
export function useTurnstile() {
  const [token, setToken] = useState<string | null>(null);
  const ref = useRef<TurnstileInstance>(null);
  const siteKey = import.meta.env.VITE_TURNSTILE_SITE_KEY as string | undefined;

  function reset() {
    setToken(null);
    ref.current?.reset();
  }

  return {
    token,
    ref,
    reset,
    siteKey,
    // No key configured: the backend skips verification too, so the form
    // must stay submittable rather than blocking on a widget that will
    // never mount.
    isReady: !siteKey || token !== null,
    setToken,
  };
}
