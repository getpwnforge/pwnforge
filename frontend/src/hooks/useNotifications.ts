// hooks/use-notifications.ts
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import type { WsColor } from "@/lib/ws-color"

export type NotificationSource =
  | { kind: "workspace"; name: string; color: WsColor }
  | { kind: "user"; name: string }

export type Notification = {
  id: string
  source: NotificationSource
  actor?: string       // emphasized subject
  text: string         // plain middle part
  highlight?: string   // emphasized object
  context: string
  createdAt: string    // ISO
  read: boolean
}

const KEY = ["notifications"] as const

// NOTE: This is a mock implementation of the useNotifications hook. In a real application, you would fetch the notifications from an API or a database.
const MOCK: Notification[] = [
  { id: "1", source: { kind: "workspace", name: "CTF Hexagon 2026", color: "rose" }, actor: "al1z", text: "a résolu", highlight: "SQL Injection 401", context: "CTF Hexagon 2026", createdAt: new Date(Date.now() - 12 * 60000).toISOString(), read: false },
  { id: "2", source: { kind: "user", name: "Nora" }, actor: "Nora", text: "t'a invité dans", highlight: "Team Alpha Bravo", context: "Qualifs DEF CON 2026", createdAt: new Date(Date.now() - 40 * 60000).toISOString(), read: false },
  { id: "3", source: { kind: "workspace", name: "Root-Me", color: "violet" }, text: "Nouveau writeup sur", highlight: "Buffer Overflow 101", context: "Root-Me", createdAt: new Date(Date.now() - 26 * 3600000).toISOString(), read: true },
  { id: "4", source: { kind: "user", name: "Marc Dubois" }, text: "Ton rang est passé", highlight: "#14 → #9", context: "HackTheBox", createdAt: new Date(Date.now() - 30 * 3600000).toISOString(), read: true },
  { id: "5", source: { kind: "workspace", name: "PwnLab", color: "amber" }, text: "Nouveau challenge disponible :", highlight: "Reverse Engineering 2026", context: "PwnLab", createdAt: new Date(Date.now() - 100 * 86400000).toISOString(), read: true },
]

export function useNotifications() {
  return useQuery({ queryKey: KEY, queryFn: async () => MOCK })
}

export function useMarkNotificationRead() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async (id: string) => {
      // await api.post(`/notifications/${id}/read`)
      MOCK.forEach((n) => { if (n.id === id) n.read = true })
    },
    onMutate: async (id) => {
      await qc.cancelQueries({ queryKey: KEY })
      const prev = qc.getQueryData<Notification[]>(KEY)
      qc.setQueryData<Notification[]>(KEY, (old) =>
        old?.map((n) => (n.id === id ? { ...n, read: true } : n)),
      )
      return { prev }
    },
    onError: (_e, _id, ctx) => {
      if (ctx?.prev) qc.setQueryData(KEY, ctx.prev)
    },
    onSettled: () => qc.invalidateQueries({ queryKey: KEY }),
  })
}

export function useMarkAllNotificationsRead() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async () => {
      // await api.post("/notifications/read-all")
      MOCK.forEach((n) => { n.read = true })
    },
    onMutate: async () => {
      await qc.cancelQueries({ queryKey: KEY })
      const prev = qc.getQueryData<Notification[]>(KEY)
      qc.setQueryData<Notification[]>(KEY, (old) =>
        old?.map((n) => ({ ...n, read: true })),
      )
      return { prev }
    },
    onError: (_e, _v, ctx) => {
      if (ctx?.prev) qc.setQueryData(KEY, ctx.prev)
    },
    onSettled: () => qc.invalidateQueries({ queryKey: KEY }),
  })
}
