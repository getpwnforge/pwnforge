// src/components/common/AuthProvider.tsx
import { useCallback, useEffect, useMemo, useRef } from "react";
import type { ReactNode } from "react";
import { hashKey, useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { setUnauthenticatedHandler } from "@/lib/api";
import { AuthContext, type AuthContextValue, type AuthStatus } from "@/lib/auth";
import {
  AUTH_ME_QUERY_KEY,
  confirmEmail as confirmEmailRequest,
  fetchCurrentUser,
  forgotPassword as forgotPasswordRequest,
  login as loginRequest,
  logout as logoutRequest,
  register as registerRequest,
  resendVerification as resendVerificationRequest,
  resetPassword as resetPasswordRequest,
} from "@/api/auth";
import type {
  ForgotPasswordPayload,
  EmailResendPayload,
  LoginPayload,
  RegisterPayload,
  ResetPasswordPayload,
  User,
} from "@/types/api";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";

const AUTH_ME_QUERY_HASH = hashKey(AUTH_ME_QUERY_KEY);

/**
 * Forgets the session client-side: every cached answer goes, so that an
 * account signing in next in this tab is not served this one's, and the
 * session's own query is set to "nobody".
 *
 * Set, not removed. The provider's observer would stay on the removed query,
 * whose fetch the removal cancels without a word, and `status` would read
 * `loading`, or the old user, until something else happened to re-render the
 * provider: a visitor with no session would wait on the spinner for good.
 */
function forgetSession(queryClient: QueryClient) {
  // An /auth/me already on the wire must not land afterwards and put the
  // previous account back.
  void queryClient.cancelQueries({ queryKey: AUTH_ME_QUERY_KEY, exact: true });
  queryClient.removeQueries({ predicate: (query) => query.queryHash !== AUTH_ME_QUERY_HASH });
  queryClient.setQueryData(AUTH_ME_QUERY_KEY, null);
}

type StatusWaiter = { wanted: AuthStatus; resolve: () => void };

/**
 * Holds the session for the whole app.
 *
 * Mounted above the router and using no router hook of its own, so a redirect
 * decision stays where it belongs — in the route guard, not here.
 */
export function AuthProvider({ children }: Readonly<{ children: ReactNode }>) {
  const queryClient = useQueryClient();
  const { t } = useTranslation("auth");

  const {
    data: user,
    isPending,
    refetch,
  } = useQuery({
    queryKey: AUTH_ME_QUERY_KEY,
    queryFn: fetchCurrentUser,
    // The interceptor already handles the one recoverable failure (expired
    // access token); retrying past that only delays the login screen.
    retry: false,
    staleTime: 5 * 60 * 1000,
    refetchOnWindowFocus: true,
  });

  // Session lost outside of any component — an expired refresh token surfacing
  // on a background query, for instance.
  useEffect(
    () =>
      setUnauthenticatedHandler(() => {
        const hadSession = queryClient.getQueryData(AUTH_ME_QUERY_KEY) != null;

        forgetSession(queryClient);

        if (hadSession) {
          // Same sentence the inline error uses for this code — kept as one
          // string so the two cannot drift apart. Stable id: several requests
          // waiting on the same refused refresh each land here, one toast must
          // come out.
          toast.warning(t("errors:api.invalid_refresh_token"), { id: "session-expired" });
        }
      }),
    [queryClient, t],
  );

  // `mutateAsync` is the only stable reference a mutation result exposes; the
  // result object itself is rebuilt on every render and would invalidate the
  // memo below at each pass.
  const { mutateAsync: loginAsync } = useMutation({
    mutationFn: loginRequest,
    onSuccess: (loggedIn) => queryClient.setQueryData(AUTH_ME_QUERY_KEY, loggedIn),
  });

  // No cache write on purpose: registering does not open a session.
  const { mutateAsync: registerAsync } = useMutation({ mutationFn: registerRequest });

  // No cache write here either: verifying an address does not open a session.
  const { mutateAsync: confirmEmailAsync } = useMutation({ mutationFn: confirmEmailRequest });

  const { mutateAsync: forgotPasswordAsync } = useMutation({ mutationFn: forgotPasswordRequest });

  const { mutateAsync: resendVerificationAsync } = useMutation({
    mutationFn: resendVerificationRequest,
  });

  const { mutateAsync: resetPasswordAsync } = useMutation({ mutationFn: resetPasswordRequest });

  const { mutateAsync: logoutAsync } = useMutation({
    mutationFn: logoutRequest,
    // Even on failure: the cookies are gone client-side either way, keeping the
    // cache would only show stale data to the next user of this browser.
    onSettled: () => forgetSession(queryClient),
  });

  let status: AuthStatus;
  if (isPending) {
    status = "loading";
  } else if (user) {
    status = "authenticated";
  } else {
    status = "anonymous";
  }

  // The cache changes as soon as login or logout answers, but this provider
  // only hears of it on React Query's next batched notification. A caller
  // navigating right away would reach route guards still reading the old
  // status, and be bounced: back to the sign-in form after signing in, back
  // into the app after signing out. Both calls therefore resolve once the
  // new status has been rendered, which is when every guard has it too.
  const renderedStatus = useRef(status);
  const statusWaiters = useRef(new Set<StatusWaiter>());

  useEffect(() => {
    renderedStatus.current = status;
    for (const waiter of statusWaiters.current) {
      if (waiter.wanted === status) {
        statusWaiters.current.delete(waiter);
        waiter.resolve();
      }
    }
  }, [status]);

  const untilRendered = useCallback(
    (wanted: AuthStatus): Promise<void> =>
      renderedStatus.current === wanted
        ? Promise.resolve()
        : new Promise((resolve) => statusWaiters.current.add({ wanted, resolve })),
    [],
  );

  const login = useCallback(
    async (payload: LoginPayload): Promise<User> => {
      const loggedIn = await loginAsync(payload);
      await untilRendered("authenticated");
      return loggedIn;
    },
    [loginAsync, untilRendered],
  );

  const register = useCallback(
    (payload: RegisterPayload): Promise<void> => registerAsync(payload),
    [registerAsync],
  );

  // On failure as well: the session is forgotten locally either way, and the
  // caller still sends the user to the sign-in form.
  const logout = useCallback(async (): Promise<void> => {
    try {
      await logoutAsync();
    } finally {
      await untilRendered("anonymous");
    }
  }, [logoutAsync, untilRendered]);

  const confirmEmail = useCallback(
    (token: string): Promise<void> => confirmEmailAsync(token),
    [confirmEmailAsync],
  );

  const forgotPassword = useCallback(
    (payload: ForgotPasswordPayload): Promise<void> => forgotPasswordAsync(payload),
    [forgotPasswordAsync],
  );

  const resendVerification = useCallback(
    (payload: EmailResendPayload): Promise<void> => resendVerificationAsync(payload),
    [resendVerificationAsync],
  );

  const resetPassword = useCallback(
    (payload: ResetPasswordPayload): Promise<void> => resetPasswordAsync(payload),
    [resetPasswordAsync],
  );

  const reload = useCallback(async (): Promise<void> => {
    await refetch();
  }, [refetch]);

  const value = useMemo<AuthContextValue>(
    () => ({
      user: user ?? null,
      status,
      login,
      register,
      logout,
      reload,
      confirmEmail,
      resendVerification,
      forgotPassword,
      resetPassword,
    }),
    [user, status, login, register, logout, reload, confirmEmail, resendVerification, forgotPassword, resetPassword],
  );

  return <AuthContext value={value}>{children}</AuthContext>;
}
