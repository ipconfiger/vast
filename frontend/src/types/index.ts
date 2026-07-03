export interface User {
  id: string
  username: string
  display_name: string
  avatar_url?: string
  status?: string
  created_at: string
}

export interface Channel {
  id: string
  name: string
  description?: string
  type: 'public' | 'private' | 'dm'
  created_by: string
  created_at: string
  member_count?: number
  owner_id?: string
  role?: string
}

export interface ChannelMember {
  id: string
  channel_id: string
  user_id: string
  role: 'owner' | 'admin' | 'member'
  joined_at: string
}

export interface Message {
  id: string
  msg_id: string
  channel_id: string
  sender_id: string
  sender_name?: string
  sender_display_name?: string
  msg_type: string
  payload: any
  thread_parent_id?: string | null
  deleted_at?: string | null
  created_at: string
}

export interface Reaction {
  id: string
  message_id: string
  user_id: string
  emoji: string
  created_at: string
}

export interface JoinRequest {
  id: string
  channel_id: string
  user_id: string
  status: 'pending' | 'approved' | 'rejected'
  created_at: string
}

export interface Invitation {
  id: string
  channel_id: string
  inviter_id: string
  invitee_id: string
  status: 'pending' | 'accepted' | 'declined'
  created_at: string
}

export interface ApiError {
  code: string
  message: string
  status: number
}
export interface JoinRequestWithUser extends JoinRequest {
  user?: User
}

export interface InvitationWithChannel extends Invitation {
  channel?: Channel
  inviter?: User
}

export interface ChannelMemberWithUser extends ChannelMember {
  user?: User
}
