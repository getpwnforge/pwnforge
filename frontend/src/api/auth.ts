// src/api/auth.ts
import { api } from "@/lib/api";
import { isUnauthorized, toApiError } from "@/lib/api-error";
import type {
  ForgotPasswordPayload,
  EmailResendPayload,
  LoginPayload,
  RegisterPayload,
  ResetPasswordPayload,
  User,
} from "@/types/api";

export const AUTH_ME_QUERY_KEY = ["auth", "me"] as const;

/**
 * Current session holder, or `null` when there is none.
 *
 * A 401 here is an answer, not a failure: the app boots by asking this
 * question, and "nobody is signed in" must not surface as an error state.
 */
export async function fetchCurrentUser(): Promise<User | null> {
  try {
    const { data } = await api.get<User>("/auth/me");
    return data;
  } catch (error) {
    if (isUnauthorized(error)) return null;
    throw toApiError(error);
  }
}

/** Sets the session cookies as a side effect; the returned user is the payload. */
export async function login(payload: LoginPayload): Promise<User> {
  const { data } = await api.post<User>("/auth/login", payload);
  return data;
}

/**
 * Creates the account. Deliberately does **not** open a session: the backend
 * returns 201 without cookies, the user must verify their email first.
 */
export async function register(payload: RegisterPayload): Promise<User> {
  const { data } = await api.post<User>("/auth/register", payload);
  return data;
}

/** Revokes the refresh token server-side and clears the cookies. */
export async function logout(): Promise<void> {
  await api.post("/auth/logout", null, { skipAuthRefresh: true });
}

/**
 * Consumes an email verification token. Idempotent server-side, but does not
 * open a session — the account is verified, sign-in still happens separately.
 */
export async function confirmEmail(token: string): Promise<void> {
  await api.post("/auth/email/verify", { token });
}

/** Requests a reset email without revealing whether the address exists. */
export async function forgotPassword(payload: ForgotPasswordPayload): Promise<void> {
  await api.post("/auth/password/forgot", payload);
}

/** Requests a verification email without opening a session. */
export async function resendVerification(payload: EmailResendPayload): Promise<void> {
  await api.post("/auth/email/resend", payload);
}

/** Consumes a reset token and changes the account password. */
export async function resetPassword(payload: ResetPasswordPayload): Promise<void> {
  await api.post("/auth/password/reset", payload);
}
