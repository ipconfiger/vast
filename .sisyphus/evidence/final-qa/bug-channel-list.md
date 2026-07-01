# BUG: ChannelSidebar crashes with "channels.map is not a function"

**Severity**: High (blocks main app view)
**Component**: `frontend/src/components/ChannelSidebar.tsx:118`
**API layer**: `frontend/src/api/channels.ts:13`

**Root Cause**: 
The `/api/channels` endpoint returns `{"channels": [...]}` (an object wrapping the array).
But `apiClient<Channel[]>('/channels')` assigns the whole response object directly to the store.
The store's `channels` field becomes the object `{channels: [...]}` instead of the array `[...]`.
When ChannelSidebar calls `channels.map()`, it fails because an object has no `.map()` method.

**Fix**:
```typescript
// In frontend/src/api/channels.ts, line 12-14, change:
const channels = await apiClient<Channel[]>('/channels')
setChannels(channels)

// To:
const data = await apiClient<{channels: Channel[]}>('/channels')
setChannels(data.channels)
```
