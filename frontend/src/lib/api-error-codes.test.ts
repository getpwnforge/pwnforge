// src/lib/api-error-codes.test.ts
import { describe, expect, it } from "vitest";

import {
  API_ERROR_CODES,
  API_FIELD_CODES,
  ApiError,
  apiErrorKey,
  fieldErrorKey,
} from "@/lib/api-error";
import i18n from "@/lib/i18n";

describe("every error code the frontend knows", () => {
  // Read from each language's own resources: `i18n.exists` would fall back
  // on English and find a key French lacks.
  const has = (lng: string, key: string) => typeof i18n.getResource(lng, "errors", key) === "string";

  it.each(["en", "fr"])("has its message in %s", (lng) => {
    const missing = API_ERROR_CODES.filter((code) => !has(lng, `api.${code}`));
    expect(missing).toEqual([]);
  });

  it.each(["en", "fr"])("has its field message in %s, the generic one included", (lng) => {
    const missing = [...API_FIELD_CODES, "invalid"].filter((code) => !has(lng, `field.${code}`));
    expect(missing).toEqual([]);
  });
});

describe("the message keys", () => {
  it("name the code when it is known, the generic message otherwise", () => {
    expect(apiErrorKey(new ApiError("workspace_is_solo", 409))).toBe("errors:api.workspace_is_solo");
    expect(apiErrorKey(new ApiError("not_a_code_yet", 409))).toBe("errors:api.internal_error");
    expect(fieldErrorKey("invitee_named_twice")).toBe("errors:field.invitee_named_twice");
    expect(fieldErrorKey("not_a_code_yet")).toBe("errors:field.invalid");
  });
});
