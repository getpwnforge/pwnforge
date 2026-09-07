// src/lib/api-error.ts
import axios from "axios";

/** Field name -> validator codes, as returned by `AppError::Validation`. */
export type ValidationFields = Record<string, string[]>;

/**
 * Error codes the API can return, mirrored from `crates/api/src/error.rs`.
 *
 * The backend never sends a human sentence: every failure is a stable code the
 * frontend translates. Keeping the list here rather than deriving it lets us
 * fall back to a generic message for a code this build does not know yet — an
 * older frontend against a newer backend must not render a raw code.
 */
export const API_ERROR_CODES = [
  "validation_failed",
  "username_taken",
  "username_reserved",
  "email_taken",
  "invalid_email",
  "email_domain_not_allowed",
  "public_signup_disabled",
  "invalid_credentials",
  "email_not_verified",
  "invalid_refresh_token",
  "password_compromised",
  "password_unchanged",
  "token_invalid",
  "token_expired",
  "token_consumed",
  "invalid_timezone",
  "email_send_failed",
  "rate_limited",
  "unauthorized",
  "not_found",
  "service_unavailable",
  "internal_error",
  "turnstile_failed",
  "invalid_body",
  "malformed_json",
  "unsupported_media_type",
  "legal_version_stale",
  "legal_acceptance_required",
  // Frontend-only: the request never reached the API.
  "network",
] as const;

export type ApiErrorCode = (typeof API_ERROR_CODES)[number];

const KNOWN_CODES: ReadonlySet<string> = new Set(API_ERROR_CODES);

/**
 * A failed API call, reduced to what a caller can act on.
 *
 * Every rejection coming out of the axios instance is one of these, including
 * network failures and non-JSON responses (a proxy returning HTML, for
 * instance). Callers therefore never have to inspect an `AxiosError`.
 */
export class ApiError extends Error {
  /** Stable backend code, or `"network"` / `"internal_error"` when there is none. */
  readonly code: string;
  /** HTTP status, or `null` when the request never got a response. */
  readonly status: number | null;
  /** Only set for `validation_failed`: field name -> validator codes. */
  readonly fields: ValidationFields | null;
  /** Only set for `rate_limited`. */
  readonly retryAfterSecs: number | null;
  /**
   * Raw upstream message, when the backend passes one through — currently only
   * the setup test-email route. Never translated, never shown to an end user:
   * it exists so an operator can read what their SMTP server actually said.
   */
  readonly detail: string | null;
  /** Returned only when login identifies an unverified account. */
  readonly resendToken: string | null;

  constructor(
    code: string,
    status: number | null,
    options?: {
      fields?: ValidationFields | null;
      retryAfterSecs?: number | null;
      detail?: string | null;
      resendToken?: string | null;
      cause?: unknown;
    },
  ) {
    super(code, options?.cause === undefined ? undefined : { cause: options.cause });
    this.name = "ApiError";
    this.code = code;
    this.status = status;
    this.fields = options?.fields ?? null;
    this.retryAfterSecs = options?.retryAfterSecs ?? null;
    this.detail = options?.detail ?? null;
    this.resendToken = options?.resendToken ?? null;
  }
}

export function isApiError(error: unknown): error is ApiError {
  return error instanceof ApiError;
}

/** Narrow helper for the common "was this a 401?" check. */
export function isUnauthorized(error: unknown): boolean {
  return isApiError(error) && error.status === 401;
}

type ErrorBody = {
  error?: unknown;
  fields?: unknown;
  retry_after_secs?: unknown;
  resend_token?: unknown;
  detail?: unknown;
};

function readErrorBody(data: unknown): ErrorBody {
  return typeof data === "object" && data !== null ? (data as ErrorBody) : {};
}

function readFields(value: unknown): ValidationFields | null {
  if (typeof value !== "object" || value === null) return null;

  const fields: ValidationFields = {};
  for (const [field, codes] of Object.entries(value)) {
    if (Array.isArray(codes)) {
      fields[field] = codes.filter((code): code is string => typeof code === "string");
    }
  }

  return Object.keys(fields).length > 0 ? fields : null;
}

/** Normalises anything axios can reject with into an {@link ApiError}. */
export function toApiError(error: unknown): ApiError {
  if (isApiError(error)) return error;

  if (!axios.isAxiosError(error)) {
    return new ApiError("internal_error", null, { cause: error });
  }

  const response = error.response;

  // No response at all: DNS failure, connection refused, CORS, timeout.
  if (!response) {
    return new ApiError("network", null, { cause: error });
  }

  const body = readErrorBody(response.data);
  // A 5xx from a reverse proxy has no JSON body — fall back on the status
  // rather than surfacing an empty code.
  const code =
    typeof body.error === "string" && body.error.length > 0
      ? body.error
      : codeFromStatus(response.status);

  return new ApiError(code, response.status, {
    fields: readFields(body.fields),
    retryAfterSecs: typeof body.retry_after_secs === "number" ? body.retry_after_secs : null,
    detail: typeof body.detail === "string" ? body.detail : null,
    resendToken: typeof body.resend_token === "string" ? body.resend_token : null,
    cause: error,
  });
}

function codeFromStatus(status: number): string {
  if (status === 401) return "unauthorized";
  if (status === 404) return "not_found";
  if (status === 429) return "rate_limited";
  if (status === 503) return "service_unavailable";
  return "internal_error";
}

/**
 * i18next key for an error, ready for `t()`.
 *
 * Unknown codes collapse to the generic message: a code this build has never
 * heard of is not something the user can act on anyway.
 */
export function apiErrorKey(error: unknown): string {
  const code = isApiError(error) ? error.code : "internal_error";
  return KNOWN_CODES.has(code) ? `errors:api.${code}` : "errors:api.internal_error";
}
