// hooks/use-workspaces.ts
import type { WsColor } from "@/lib/ws-color"
import { useMatch } from "react-router"
import { WORKSPACE_ROUTE_MATCH } from "@/lib/routes";

export type Workspace = {
  id: string
  name: string
  color: WsColor
  type: string
  platform: string
  tags: string[]
}

// NOTE: This is a mock implementation of the useWorkspaces hook. In a real application, you would fetch the workspaces from an API or a database.
const MOCK: Workspace[] = [
  { id: "htb", name: "HackTheBox", color: "blue", type: "solo", platform: "HackTheBox", "tags": ["web", "pwn", "jwt", "sqli"] },
  { id: "hex", name: "CTF Hexagon 2026", color: "rose", type: "team", platform: "CTF Online", tags: ["web", "pwn", "crypto"] },
  { id: "rm", name: "Root-Me", color: "violet", type: "solo", platform: "Root-Me", tags: ["web", "pwn", "crypto"] },
  { id: "long", name: "Qualifs DEF CON 2026 — Team Alpha Bravo", color: "amber", type: "team", platform: "CTF Online", tags: ["web", "pwn", "crypto"] },
]

export function useWorkspaces() {
  return { data: MOCK, isLoading: false, isError: false }
}

export function useWorkspaceId(): string | null {
  const match = useMatch(WORKSPACE_ROUTE_MATCH)
  return match?.params.workspaceId ?? null
}
