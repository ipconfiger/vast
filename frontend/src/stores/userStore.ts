import { create } from 'zustand'

interface UserInfo {
  id: string
  display_name: string
}

interface UserMapState {
  users: Map<string, string>
  setUser: (id: string, name: string) => void
  setUsers: (users: UserInfo[]) => void
  getName: (id: string) => string | undefined
}

export const useUserStore = create<UserMapState>()((set, get) => ({
  users: new Map(),

  setUser: (id, name) =>
    set((state) => {
      const next = new Map(state.users)
      next.set(id, name)
      return { users: next }
    }),

  setUsers: (infos) =>
    set((state) => {
      const next = new Map(state.users)
      for (const info of infos) {
        next.set(info.id, info.display_name)
      }
      return { users: next }
    }),

  getName: (id) => get().users.get(id),
}))
