// src/components/common/AuthProvider.tsx
import { useCallback, useEffect, useMemo } from "react";
import type { ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
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

        queryClient.setQueryData(AUTH_ME_QUERY_KEY, null);

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
    onSettled: () => {
      queryClient.removeQueries();
      queryClient.setQueryData(AUTH_ME_QUERY_KEY, null);
    },
  });

  const login = useCallback(
    (payload: LoginPayload): Promise<User> => loginAsync(payload),
    [loginAsync],
  );

  const register = useCallback(
    (payload: RegisterPayload): Promise<User> => registerAsync(payload),
    [registerAsync],
  );

  const logout = useCallback(async (): Promise<void> => {
    await logoutAsync();
  }, [logoutAsync]);

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

  let status: AuthStatus;
  if (isPending) {
    status = "loading";
  } else if (user) {
    status = "authenticated";
  } else {
    status = "anonymous";
  }

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
