import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { WsIcon } from "@/components/brand/WsIcon";

describe("WsIcon", () => {
  it("shows the initials when there is no image", () => {
    const { container } = render(<WsIcon name="CTF Hexagon" color="rose" />);
    expect(screen.getByText("CH")).toBeInTheDocument();
    expect(container.querySelector("img")).toBeNull();
  });

  it("keeps the initials until an image has loaded", () => {
    // jsdom never loads images: Radix keeps the fallback, as it would on a 404.
    render(<WsIcon name="CTF Hexagon" color="rose" src="/api/v1/avatars/workspaces/x/1/64.webp" />);
    expect(screen.getByText("CH")).toBeInTheDocument();
  });

  it("takes two initials at most, across spaces and slashes", () => {
    render(<WsIcon name="team/alpha bravo" />);
    expect(screen.getByText("TA")).toBeInTheDocument();
  });
});
