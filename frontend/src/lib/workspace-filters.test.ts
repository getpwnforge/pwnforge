import { describe, expect, it } from "vitest";

import { countFacets, filterWorkspaces, NO_FACETS, toggleFacet } from "@/lib/workspace-filters";
import { teamSummary, workspaceSummary } from "@/test/api-fixtures";

const team = teamSummary({ slug: "alpha-k2m4", name: "Team Alpha", color: "blue" });
const rootme = workspaceSummary({ slug: "root-me-cd34", name: "Root-Me", type: "platform", platform: "root_me", mode: "solo", team: null, favorite: true });
const hexagon = workspaceSummary({ slug: "hexagon-ab12", name: "CTF Hexagon", type: "event", platform: "ctftime", team });
const old = workspaceSummary({ slug: "old-ctf-ef56", name: "Old CTF", type: "event", team, archived: true });
const list = [rootme, hexagon, old];
const names = (l: readonly { name: string }[]) => l.map((ws) => ws.name);

describe("filterWorkspaces", () => {
  it("splits by tab and keeps the API order", () => {
    expect(names(filterWorkspaces(list, { tab: "all", query: "" }))).toEqual(["Root-Me", "CTF Hexagon", "Old CTF"]);
    expect(names(filterWorkspaces(list, { tab: "active", query: "" }))).toEqual(["Root-Me", "CTF Hexagon"]);
    expect(names(filterWorkspaces(list, { tab: "archived", query: "" }))).toEqual(["Old CTF"]);
  });

  it("searches the name, the slug and the team name, ignoring case and spaces", () => {
    expect(names(filterWorkspaces(list, { tab: "all", query: "  HEXA " }))).toEqual(["CTF Hexagon"]);
    expect(names(filterWorkspaces(list, { tab: "all", query: "cd34" }))).toEqual(["Root-Me"]);
    expect(names(filterWorkspaces(list, { tab: "active", query: "alpha" }))).toEqual(["CTF Hexagon"]);
  });

  it("returns nothing when nothing matches", () => {
    expect(filterWorkspaces(list, { tab: "all", query: "zzz" })).toEqual([]);
  });

  it("keeps everything when no facet is checked", () => {
    expect(names(filterWorkspaces(list, { tab: "all", query: "", facets: NO_FACETS }))).toEqual(["Root-Me", "CTF Hexagon", "Old CTF"]);
  });

  it("filters by type, mode and platform, each facet narrowing the others", () => {
    const events = toggleFacet(NO_FACETS, "types", "event");
    expect(names(filterWorkspaces(list, { tab: "all", query: "", facets: events }))).toEqual(["CTF Hexagon", "Old CTF"]);

    const solo = toggleFacet(NO_FACETS, "modes", "solo");
    expect(names(filterWorkspaces(list, { tab: "all", query: "", facets: solo }))).toEqual(["Root-Me"]);

    const ctftimeEvents = toggleFacet(events, "platforms", "ctftime");
    expect(names(filterWorkspaces(list, { tab: "all", query: "", facets: ctftimeEvents }))).toEqual(["CTF Hexagon"]);
  });

  it("drops a workspace with no platform once a platform is checked", () => {
    const ctftime = toggleFacet(NO_FACETS, "platforms", "ctftime");
    expect(names(filterWorkspaces(list, { tab: "all", query: "", facets: ctftime }))).not.toContain("Old CTF");
  });
});

describe("toggleFacet and countFacets", () => {
  it("checks, then unchecks, in a new object each time", () => {
    const once = toggleFacet(NO_FACETS, "types", "event");
    expect(once).not.toBe(NO_FACETS);
    expect(once.types.has("event")).toBe(true);
    expect(NO_FACETS.types.size).toBe(0);

    const twice = toggleFacet(once, "types", "event");
    expect(twice.types.has("event")).toBe(false);
  });

  it("counts the checked boxes across facets", () => {
    const facets = toggleFacet(toggleFacet(NO_FACETS, "types", "event"), "platforms", "htb");
    expect(countFacets(NO_FACETS)).toBe(0);
    expect(countFacets(facets)).toBe(2);
  });
});
