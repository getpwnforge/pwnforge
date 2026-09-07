// src/lib/auth.ts
import { createContext } from "react";
import type {
  EmailResendPayload,
  ForgotPasswordPayload,
  LoginPayload,
  RegisterPayload,
  ResetPasswordPayload,
  User,
} from "@/types/api";

/** Provider state, not something the API ever returns. */
export type AuthStatus = "loading" | "authenticated" | "anonymous";

export type AuthContextValue = {
  /** `null` while loading and when signed out — check `status` to tell them apart. */
  user: User | null;
  status: AuthStatus;
  /** Resolves with the signed-in user, rejects with an `ApiError`. */
  login: (payload: LoginPayload) => Promise<User>;
  /** Creates the account without signing in (email verification comes first). */
  register: (payload: RegisterPayload) => Promise<User>;
  logout: () => Promise<void>;
  /** Consumes an email verification token. Does not open a session. */
  confirmEmail: (token: string) => Promise<void>;
  resendVerification: (payload: EmailResendPayload) => Promise<void>;
  /** Sends a password reset email. Does not open a session. */
  forgotPassword: (payload: ForgotPasswordPayload) => Promise<void>;
  /** Consumes a password reset token and sets a new password. Does not open a session. */
  resetPassword: (payload: ResetPasswordPayload) => Promise<void>;
  /** Re-reads `/auth/me`, e.g. after the user verifies their email. */
  reload: () => Promise<void>;
};

/**
 * `null` outside a provider so `useAuth` can fail loudly rather than hand back
 * a default that silently reads as "signed out".
 */
export const AuthContext = createContext<AuthContextValue | null>(null);
