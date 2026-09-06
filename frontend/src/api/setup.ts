// src/api/setup.ts
import { api } from "@/lib/api";
import type { EmailConfig, SetupPayload, SetupStatus, User } from "@/types/api";

export const SETUP_STATUS_QUERY_KEY = ["setup", "status"] as const;

/** Header carrying the boot token. A header, so GET routes are covered too. */
const SETUP_TOKEN_HEADER = "x-setup-token";

/**
 * The one setup route answered without a token: the app has to know whether to
 * show the wizard before it has anything to authenticate with.
 */
export async function fetchSetupStatus(): Promise<SetupStatus> {
  const { data } = await api.get<SetupStatus>("/setup/status", { skipAuthRefresh: true });
  return data;
}

/**
 * Doubles as the token check: the backend answers 404 on a wrong token, and
 * the wizard has nothing else to validate step 1 against.
 */
export async function fetchEmailConfig(token: string): Promise<EmailConfig> {
  const { data } = await api.get<EmailConfig>("/setup/email-config", {
    headers: { [SETUP_TOKEN_HEADER]: token },
    skipAuthRefresh: true,
  });

  return data;
}

/**
 * Dry run of the administrator rules — reserved username, blocked email domain,
 * breached password. Resolves when the account would be accepted.
 *
 * Exists so those rejections land on the step that owns the field instead of at
 * the end of the wizard. The final POST re-runs them regardless.
 */
export async function validateAdmin(
  token: string,
  payload: {
    admin_username: string;
    admin_email: string;
    admin_password: string;
  },
): Promise<void> {
  await api.post("/setup/validate-admin", payload, {
    headers: { [SETUP_TOKEN_HEADER]: token },
    skipAuthRefresh: true,
  });
}

export async function sendTestEmail(token: string, to: string): Promise<void> {
  await api.post(
    "/setup/test-email",
    { to },
    { headers: { [SETUP_TOKEN_HEADER]: token }, skipAuthRefresh: true },
  );
}

/** Creates the first administrator. Returns the account; opens no session. */
export async function completeSetup(token: string, payload: SetupPayload): Promise<User> {
  const { data } = await api.post<User>("/setup", payload, {
    headers: { [SETUP_TOKEN_HEADER]: token },
    skipAuthRefresh: true,
  });

  return data;
}
