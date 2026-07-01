import { create } from 'zustand'
import type { InvitationWithChannel } from '../types'

interface PermissionState {
  pendingInvitations: InvitationWithChannel[]
  pendingRequestsCount: number
  setPendingRequestsCount: (count: number) => void
  addInvitation: (invitation: InvitationWithChannel) => void
  removeInvitation: (invitationId: string) => void
  setInvitations: (invitations: InvitationWithChannel[]) => void
}

export const usePermissionStore = create<PermissionState>()((set) => ({
  pendingInvitations: [],
  pendingRequestsCount: 0,

  setPendingRequestsCount: (count) => set({ pendingRequestsCount: count }),

  addInvitation: (invitation) =>
    set((state) => ({
      pendingInvitations: [...state.pendingInvitations, invitation],
    })),

  removeInvitation: (invitationId) =>
    set((state) => ({
      pendingInvitations: state.pendingInvitations.filter(
        (inv) => inv.id !== invitationId,
      ),
    })),

  setInvitations: (invitations) => set({ pendingInvitations: invitations }),
}))
