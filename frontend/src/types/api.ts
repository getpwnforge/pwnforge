// src/types/api.ts
import type {
  EmailConfigResponse,
  EmailResendRequest,
  LoginRequest,
  PasswordForgotRequest,
  PasswordResetRequest,
  RegisterRequest,
  SetupRequest,
  SetupStatusResponse,
  UserResponse,
} from "@/lib/api/types";

// The names the auth and setup code has always used, kept so that none of
// it has to change, as aliases of the types generated from the backend's
// OpenAPI spec (`lib/api/types.ts`): nothing here is written by hand any
// more, so a DTO and its mirror cannot drift apart.

/** The account holding the session (`GET /auth/me`, login, refresh). */
export type User = UserResponse;

/** The backend accepts either a username or an email in `username_or_email`. */
export type LoginPayload = LoginRequest;

export type RegisterPayload = RegisterRequest;

export type SetupStatus = SetupStatusResponse;

/** Never carries a secret, by design: whether credentials exist, not what they are. */
export type EmailConfig = EmailConfigResponse;

/** `default_timezone` is an IANA identifier, never a UTC offset. */
export type SetupPayload = SetupRequest;

export type ForgotPasswordPayload = PasswordForgotRequest;

export type EmailResendPayload = EmailResendRequest;

export type ResetPasswordPayload = PasswordResetRequest;
