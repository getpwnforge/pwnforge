// src/types/api.ts

/**
 * Mirrors `UserResponse` in `crates/domain/src/dto/auth.rs`.
 *
 * Field names are kept exactly as they come off the wire: a mapping layer here
 * would be one more place to forget when the DTO gains a field.
 */
export type User = {
  id: string;
  username: string;
  email: string;
  email_verified: boolean;
  /** RFC 3339, UTC. */
  created_at: string;
};

/** Mirrors `LoginRequest`. The backend accepts either a username or an email. */
export type LoginPayload = {
  username_or_email: string;
  password: string;
  turnstile_token: string;
};

/** Mirrors `RegisterRequest`. */
export type RegisterPayload = {
  email: string;
  username: string;
  password: string;
  legal: { terms_version: string; privacy_version: string };
  turnstile_token: string;
};

export type SetupStatus = {
  completed: boolean;
};

/** Mirrors `EmailConfigResponse`. Never carries a secret, by design. */
export type EmailConfig = {
  backend: string;
  from: string;
  smtp_host: string | null;
  smtp_port: number | null;
  smtp_tls: string | null;
  /** Whether credentials are configured, not what they are. */
  smtp_auth: boolean;
  resend_key_hint: string | null;
  public_url: string;
};

/** Mirrors `SetupRequest`. */
export type SetupPayload = {
  admin_username: string;
  admin_email: string;
  admin_password: string;
  default_locale: string;
  /** IANA identifier, never a UTC offset. */
  default_timezone: string;
  allow_public_signup: boolean;
};

export type ForgotPasswordPayload = {
  email: string;
  turnstile_token: string;
};

export type EmailResendPayload = {
  token: string;
};

export type ResetPasswordPayload = {
  token: string;
  new_password: string;
};
