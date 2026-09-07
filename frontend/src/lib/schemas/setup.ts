// src/lib/schemas/setup.ts
import { z } from "zod";
import { emailRule, passwordRule, usernameRule } from "@/lib/schemas/auth";

// The administrator the wizard creates is an ordinary account with a flag, so
// it is held to the same rules — imported rather than restated.
export const tokenSchema = z.object({
  token: z.string().trim().min(1, { message: "setup:validation.tokenRequired" }),
});

export const adminSchema = z.object({
  admin_username: usernameRule,
  admin_email: emailRule,
  admin_password: passwordRule,
});

export const settingsSchema = z.object({
  default_locale: z
    .string()
    .min(2, { message: "setup:validation.localeRequired" })
    .max(10, { message: "setup:validation.localeRequired" }),
  default_timezone: z
    .string()
    .min(1, { message: "setup:validation.timezoneRequired" })
    .max(64, { message: "setup:validation.timezoneRequired" }),
  allow_public_signup: z.boolean(),
  hide_landing_page: z.boolean(),
});

export const testEmailSchema = z.object({
  to: emailRule,
});

export type TokenValues = z.infer<typeof tokenSchema>;
export type AdminValues = z.infer<typeof adminSchema>;
export type SettingsValues = z.infer<typeof settingsSchema>;
export type TestEmailValues = z.infer<typeof testEmailSchema>;
