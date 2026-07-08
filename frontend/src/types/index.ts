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
  is_archived: boolean
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
  sender_avatar_url?: string
  is_bot?: boolean
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

export interface TrainReply {
  user_id: string
  username: string
  display_name?: string | null
  avatar_url?: string | null
  content: string
  created_at: number
}

export interface Train {
  id: string
  channel_id: string
  creator_id: string
  title: string
  replies: TrainReply[]
  created_at: number
}

// Vote JSON keys are camelCase (backend VoteResponse uses
// #[serde(rename_all = "camelCase")], unlike Train). voter_ids is
// never serialized to clients — only count + myVote.
export interface VoteOption {
  id: string
  text: string
  count: number
}

export interface Vote {
  id: string
  channelId: string
  creatorId: string
  title: string
  options: VoteOption[]
  myVote: string | null
  createdAt: number
}

export interface FileRecord {
  id: string
  uploader_id: string
  uploader_name: string
  uploader_display_name: string
  uploader_avatar_url: string
  channel_id: string | null
  channel_name?: string
  original_name: string
  size: number
  mime_type: string
  extension: string
  is_deleted: boolean
  deleted_at: number | null
  deleted_by: string | null
  created_at: number
}

export interface FileListResponse {
  files: FileRecord[]
  next_cursor: string
  has_more: boolean
}

export interface FileFilters {
  channel_id?: string
  uploader_id?: string
  mime_type?: string
  mime_prefix?: string
  size_min?: number
  size_max?: number
  created_after?: number
  created_before?: number
  search?: string
  sort_by?: 'created_at' | 'size' | 'name'
  sort_order?: 'asc' | 'desc'
  cursor?: string
  limit?: number
}
