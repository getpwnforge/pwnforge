import { create } from "zustand"

interface CommandPaletteState {
  open: boolean
  setOpen: (open: boolean) => void
  toggle: () => void
}

export const useCommandPalette = create<CommandPaletteState>((set) => ({
  open: false,
  setOpen: (open) => set({ open }),
  toggle: () => set((s) => ({ open: !s.open })),
}))

// Registered once at module load, not inside a component — avoids
// duplicate listeners if multiple components consume this hook.
if (typeof window !== "undefined") {
  window.addEventListener("keydown", (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
      e.preventDefault()
      useCommandPalette.getState().toggle()
    }
  })
}
