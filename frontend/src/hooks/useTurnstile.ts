import { useCallback, useRef, useState } from "react";
import type { TurnstileInstance } from "@marsidev/react-turnstile";
import { useInstanceConfig } from "@/hooks/useInstanceConfig";

/**
 * The form a widget protects. The backend checks that a token carries the
 * action of the route it reaches, with the same strings.
 */
export type TurnstileAction = "login" | "register" | "forgot_password" | "contact";

/** Why the widget holds no token while it is on screen. */
export type TurnstileIssue = "error" | "expired";

// Turnstile tokens are single-use: any failed submission burns the token, and
// retrying without resetting the widget fails with turnstile_failed rather
// than the real error. Centralised here so no page can forget the reset.
export function useTurnstile(action: TurnstileAction) {
  const [token, setToken] = useState<string | null>(null);
  const [issue, setIssue] = useState<TurnstileIssue | null>(null);
  const ref = useRef<TurnstileInstance>(null);
  // The site key comes from the backend at startup, not from the build: the
  // published image carries none, and the operator turns Turnstile on with
  // the backend's environment alone.
  const config = useInstanceConfig();
  const siteKey = config.data?.turnstile_site_key ?? undefined;

  function reset() {
    setToken(null);
    setIssue(null);
    ref.current?.reset();
  }

  // Stable identities: the widget reads its callbacks through a ref, but the
  // script loader's options are an effect dependency.
  const onSuccess = useCallback((solved: string) => {
    setToken(solved);
    setIssue(null);
  }, []);

  // The widget retries and refreshes on its own. These only drop the token it
  // no longer vouches for and say why, so the form does not sit disabled with
  // no explanation.
  const onError = useCallback(() => {
    setToken(null);
    setIssue("error");
  }, []);

  const onExpire = useCallback(() => {
    setToken(null);
    setIssue("expired");
  }, []);

  return {
    action,
    token,
    ref,
    reset,
    siteKey,
    issue,
    onSuccess,
    onError,
    onExpire,
    // The configuration did not load: whether Turnstile is on is unknown, so
    // the form stays shut and says why.
    configError: config.isError,
    // Not before the configuration is known, so a form cannot leave with an
    // empty token while the widget is still on its way. With no key
    // configured the backend skips verification too, and the form opens as
    // soon as the configuration says so.
    isReady: config.isSuccess && (!siteKey || token !== null),
  };
}
