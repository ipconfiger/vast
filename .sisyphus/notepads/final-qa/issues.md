
## F3: Real Manual QA - 2026-07-01

### BUG: ChannelSidebar crashes with "channels.map is not a function"
- **File**: `frontend/src/components/ChannelSidebar.tsx:118`
- **Source**: `frontend/src/api/channels.ts:13`
- **Root Cause**: API returns `{channels: [...]}`, apiClient stores entire object as array
- **Fix**: Unwrap `data.channels` from API response before storing
- **Severity**: High - blocks main app view after login
