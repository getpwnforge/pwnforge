import { describe, expect, it } from "vitest";

import { displayName } from "@/lib/display-name";

describe("displayName", () => {
  it("joins the first and last names", () => {
    expect(displayName({ first_name: "Marc", last_name: "Dubois", username: "marc" })).toBe("Marc Dubois");
  });

  it("keeps a single name when the other is missing or blank", () => {
    expect(displayName({ first_name: "Nora", last_name: null, username: "nora" })).toBe("Nora");
    expect(displayName({ first_name: "  ", last_name: "Dubois", username: "marc" })).toBe("Dubois");
  });

  it("falls back to the username, then the handle", () => {
    expect(displayName({ first_name: null, last_name: null, username: "al1z" })).toBe("al1z");
    expect(displayName({ first_name: null, last_name: null, handle: "nora" })).toBe("nora");
  });

  it("never prints null", () => {
    expect(displayName({ first_name: null, last_name: null })).toBe("");
  });
});
