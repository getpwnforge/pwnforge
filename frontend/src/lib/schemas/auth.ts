// src/lib/schemas/auth.ts
import { z } from "zod";

/**
 * Mirrors `USERNAME_RE` in `crates/domain/src/dto/auth.rs`.
 *
 * Client-side validation is a courtesy, not a control: the backend re-runs
 * every rule below on the request itself.
 */
export const USERNAME_PATTERN = /^[a-zA-Z0-9][a-zA-Z0-9_-]*$/;

// Messages are i18n keys, resolved with `t()` at render time — a schema module
// has no business holding user-facing English.

/**
 * The three account rules, defined once.
 *
 * The wizard and the sign-up form both create an account against the same
 * backend DTOs; splitting these would mean raising the minimum length in one
 * place and shipping the other unchanged.
 */
export const usernameRule = z
  .string()
  .trim()
  .min(3, { message: "auth:validation.usernameLength" })
  .max(32, { message: "auth:validation.usernameLength" })
  .regex(USERNAME_PATTERN, { message: "auth:validation.usernamePattern" });

// Length on the raw string, format through `z.email()` — `.email()` as a
// method on ZodString is deprecated in zod 4.
export const emailRule = z
  .string()
  .trim()
  .max(254, { message: "auth:validation.emailInvalid" })
  .pipe(z.email({ message: "auth:validation.emailInvalid" }));

/**
 * Mirrors `RegisterRequest`: length, plus one digit and one symbol.
 *
 * "Symbol" is defined as the backend defines it — anything Rust's
 * `char::is_alphanumeric` rejects — hence `\p{L}\p{N}` rather than `[A-Za-z0-9]`.
 * Spelling it the naive way would accept `motdepassé1` here and see it refused
 * by the API, since `é` is a letter on both sides.
 *
 * The strength meter next to the field remains guidance, not a gate: these are
 * the only rules that actually block a submission.
 */
export const passwordRule = z
  .string()
  .min(12, { message: "auth:validation.passwordLength" })
  .max(128, { message: "auth:validation.passwordLength" })
  .refine((value) => /\d/.test(value) && /[^\p{L}\p{N}]/u.test(value), {
    message: "auth:validation.passwordComposition",
  });

/** Mirrors `LoginRequest`. The backend takes a username or an email here. */
export const loginSchema = z.object({
  username_or_email: z
    .string()
    .trim()
    .min(1, { message: "auth:validation.identifierRequired" })
    .max(254, { message: "auth:validation.identifierRequired" }),
  // Not `passwordRule`: an account created before a rule changed must still be
  // able to sign in, and the length of an existing password is not this form's
  // business.
  password: z.string().min(1, { message: "auth:validation.passwordRequired" }),
});

/** Mirrors `RegisterRequest`. */
export const registerSchema = z.object({
  username: usernameRule,
  email: emailRule,
  password: passwordRule,
});

export const forgotPasswordSchema = z.object({
  email: emailRule,
});

export const resetPasswordSchema = z
  .object({
    password: passwordRule,
    confirmPassword: z.string(),
  })
  .refine((values) => values.password === values.confirmPassword, {
    message: "auth:validation.passwordMismatch",
    path: ["confirmPassword"],
  });

export type LoginValues = z.infer<typeof loginSchema>;
export type RegisterValues = z.infer<typeof registerSchema>;
export type ForgotPasswordValues = z.infer<typeof forgotPasswordSchema>;
export type ResetPasswordValues = z.infer<typeof resetPasswordSchema>;
