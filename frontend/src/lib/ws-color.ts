export const WS_COLORS = ["ember", "blue", "violet", "teal", "rose", "amber"] as const
export type WsColor = (typeof WS_COLORS)[number]

// Deterministic color from a workspace identifier (djb2-style hash).
export function wsColorFromName(name: string): WsColor {
  let hash = 0
  for (let i = 0; i < name.length; i++) {
    hash = Math.trunc(hash * 31 + (name.codePointAt(i) ?? 0)) // keep it a 32-bit int
  }
  return WS_COLORS[(hash >>> 0) % WS_COLORS.length]
}
