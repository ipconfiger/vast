# /vote 投票功能实现计划

> **Status**: COMPLETE — E2E verified, pending commit
> **Created**: 2026-07-03
> **Intent**: CLEAR — user specified exact feature: `/vote <title>` opens modal to build options, submit creates a card message with horizontal bar chart + vote buttons, one locked vote per user, anonymous counts only.

## Confirmed Design Decisions (user-approved)

1. **投票即锁定** — 投了不可更改，后端已投返回 409 Conflict
2. **匿名投票** — 只显示票数和百分比，不暴露投票者身份
3. **Modal 构建选项** — `/vote 中午吃啥` 不直接发送，而是弹出 modal 让用户添加选项（最少 2 个，最多 10 个），确认后才发送
4. **横向柱状图** — 每个选项一行：选项文字 + 柱状条（宽度=得票百分比）+ 票数 + 投票按钮
5. **与 /train 类似的数据流** — 独立 `votes` 表 + WS `VoteUpdated` 事件 + `_vote` 消息 payload

## Architecture

### Data Model

New `votes` table:
```sql
CREATE TABLE votes (
    id          TEXT PRIMARY KEY,             -- UUID v4
    channel_id  TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    creator_id  TEXT NOT NULL REFERENCES users(id),
    title       TEXT NOT NULL,
    options     TEXT NOT NULL DEFAULT '[]',   -- JSON: [{"id":"opt_uuid","text":"麦当劳","voter_ids":["user1","user2"]}]
    created_at  INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX idx_votes_channel ON votes(channel_id, created_at);
```

`options` JSON 内部结构（后端独有，前端永远看不到 `voter_ids`）:
```json
[
  { "id": "opt-uuid-1", "text": "麦当劳", "voter_ids": ["user-1", "user-3"] },
  { "id": "opt-uuid-2", "text": "肯德基", "voter_ids": ["user-2"] }
]
```

API 响应中的 options（匿名化，只有 count）:
```json
[
  { "id": "opt-uuid-1", "text": "麦当劳", "count": 2 },
  { "id": "opt-uuid-2", "text": "肯德基", "count": 1 }
]
```

Message payload（msg_type 保持 `"text"`，与 train 一致）:
```json
{ "_vote": true, "vote_id": "<uuid>", "title": "中午吃啥" }
```

### API Endpoints

| Method | Path | Body | Returns | Purpose |
|--------|------|------|---------|---------|
| `GET` | `/api/votes/{vote_id}` | — | `VoteResponse` | 获取投票数据（匿名化的 options + my_vote） |
| `POST` | `/api/votes/{vote_id}/vote` | `{ option_id: string }` | `{ message, vote }` 201 / 409 / 403 | 投票：追加 voter，创建消息，广播 |

Vote 创建（不同于 train）不在单独的 API endpoint 中，而是由 `send_message` 中的 `_vote_request` payload 触发。

### `/vote` Command Flow（与 train 关键区别）

1. 用户在 MessageInput 输入 `/vote 中午吃啥`
2. MessageInput 检测到 `cmd === 'vote'` → **不发送消息** → 打开 `VoteBuilderModal`（title 预填为 "中午吃啥"）→ 清空输入框
3. 在 modal 中：用户点击 "+ 添加选项" 添加选项输入框（最少 2 个，最多 10 个），填入选项文字
4. 用户点击 "确认" → modal 调用 `onConfirm(title, optionsTextArray)`
5. MessageInput 发送消息 payload: `{ _vote_request: true, title: "中午吃啥", options: ["麦当劳", "肯德基", "其他"] }`
6. 后端 `send_message` 检测 `_vote_request`:
   - 生成 vote UUID + 每个选项的 opt UUID
   - INSERT into `votes` 表（options JSON 中每个选项 voter_ids 初始为空数组）
   - 存储消息 payload 改为 `{ _vote: true, vote_id: uuid, title: "中午吃啥" }`（丢弃 `_vote_request` 和原始 options 文字）
   - 广播 `NewMsg`
   - 返回 201

### Vote Cast Flow

1. 用户点击某个选项的 "投票" 按钮
2. 前端调用 `POST /api/votes/{vote_id}/vote` body `{ option_id }`
3. 后端:
   - 加载 vote 记录
   - 解析 options JSON
   - 检查 user_id 是否在**任何**选项的 voter_ids 中 → 409 Conflict "您已投票"
   - 检查频道成员资格 → 403 Forbidden
   - 找到 option_id 对应的选项 → 追加 user_id 到 voter_ids
   - 序列化 → UPDATE votes SET options
   - INSERT 消息 payload `{ _vote: true, vote_id, title }`（与创建时相同的 card）
   - 广播 `NewMsg` + `VoteUpdated { vote_id, channel_id }`
   - 返回 201 `{ message, vote }`
4. 所有客户端:
   - `NewMsg` → invalidate `['messages', channelId]`
   - `VoteUpdated` → invalidate `['vote', voteId]` → 所有 vote card 更新柱状图

### WS Protocol Addition

New `ServerEvent` variant in `src/ws/protocol.rs`:
```rust
VoteUpdated {
    vote_id: String,
    channel_id: String,
}
```

### VoteResponse Structure（后端匿名化）

```rust
// Internal (DB row, never serialized to client)
struct VoteOption {
    id: String,
    text: String,
    voter_ids: Vec<String>,
}

// API response (anonymous, no voter_ids)
struct VoteOptionResponse {
    id: String,
    text: String,
    count: usize,           // voter_ids.len()
}

struct VoteResponse {
    id: String,
    channel_id: String,
    creator_id: String,
    title: String,
    options: Vec<VoteOptionResponse>,
    my_vote: Option<String>, // option_id the current user voted for (None if not voted)
    created_at: i64,
}
```

`my_vote` 从 auth token 的 user_id 推导：遍历 options 找 voter_ids 包含当前 user_id 的选项。

### Frontend Components

1. **`VoteBuilderModal`** (`frontend/src/components/VoteBuilderModal.tsx`)
   - Props: `{ isOpen: boolean, onClose: () => void, onConfirm: (title: string, options: string[]) => void, initialTitle: string }`
   - 标题输入框（预填 initialTitle）
   - 选项列表：每行一个输入框 + 删除按钮（×）
   - "+ 添加选项" 按钮（最多 10 个，达到上限时按钮隐藏或禁用）
   - "确认" 按钮：disabled 当 options.length < 2 或任何选项文字为空
   - "取消" 按钮：关闭 modal
   - 遵循现有 modal 模式：`fixed inset-0 z-50` + `bg-black/60 backdrop-blur-sm` + centered panel

2. **`VoteMessage`** (`frontend/src/components/VoteMessage.tsx`)
   - Props: `{ voteId: string, title: string, channelId: string }`
   - 获取投票数据 `useVote(voteId)`
   - 渲染:
     - 标题 header（粗体，📋 + title）
     - 每个选项一行（flex 布局）:
       - 选项文字（左，固定宽度或 truncate）
       - 柱状条（中间，flex-1）：外层 div（灰色背景）+ 内层 div（蓝色/主题色背景，`width: percentage%`，CSS transition 动画）
       - 票数 + 百分比（柱状条内部或右侧）
       - 投票按钮（右）：未投票时显示 "投票" 并可点击；已投票时该选项高亮（绿色边框/背景），其他按钮禁用
     - 底部总投票数 "共 N 人投票"
   - 投票交互: 点击 "投票" → `useCastVote().mutateAsync({ voteId, optionId })` → 成功后柱状图自动更新（WS invalidate）

3. **MessageBubble integration** — detect `payload._vote === true` → render `<VoteMessage>`

4. **MessageInput** changes:
   - COMMANDS: 添加 `{ cmd: 'vote', desc: '发起投票', args: true, argHint: '<标题>' }`
   - Command 拦截: 在 handleSend 中检测 `cmd === 'vote'`，**不发送**，而是打开 VoteBuilderModal
   - 新增 state: `voteModalOpen`, `voteInitialTitle`
   - 渲染 `<VoteBuilderModal>` 组件

5. **API layer** (`frontend/src/api/votes.ts`)
   - `useVote(voteId)` → `GET /votes/{id}`
   - `useCastVote()` → mutation `POST /votes/{id}/vote { option_id }`

6. **WS subscription** (extend `useCursorSync.ts`)
   - Subscribe to `vote_updated` → `queryClient.invalidateQueries(['vote', voteId])`

## Todos

### Wave 1: Backend

- [x] 1. DB migration: create `votes` table (TDD)
  - **What**: Add `004_add_votes.up.sql` + `.down.sql`. Table: `votes(id TEXT PK, channel_id, creator_id, title, options TEXT DEFAULT '[]', created_at INTEGER)`. Index on `(channel_id, created_at)`.
  - **Must NOT do**: Do not modify existing tables. Do not change the messages CHECK constraint.
  - **Parallelization**: Wave 1 | Blocked by: nothing | Blocks: T2
  - **References**: `db/migrations/003_add_trains.up.sql` (exact same pattern, change table name + columns). DB path: `db/vast.db` or whatever the existing migration runner uses.
  - **Acceptance criteria**: `cargo test` passes; migration up/down round-trips; `votes` table exists with correct schema.
  - **QA scenarios**: Run migration up → verify table columns. Run down → table gone. Evidence `.omo/evidence/task-1-vote-command.txt`
  - **Commit**: Y | `feat(vote): DB migration for votes table`

- [x] 2. Backend vote API + `/vote_request` message handler + WS event (TDD)
  - **What**:
    - `src/api/votes.rs` (NEW file, model after `src/api/trains.rs`):
      - `VoteOption` (internal, with voter_ids), `VoteOptionResponse` (anonymous, with count), `VoteResponse` (with my_vote), `CastVoteRequest { option_id: String }`, `CastVoteResponse { message: MessageResponse, vote: VoteResponse }`
      - `get_vote` handler: GET `/api/votes/{vote_id}` → load vote → parse options → compute counts + my_vote → return `VoteResponse`
      - `cast_vote` handler: POST `/api/votes/{vote_id}/vote` → load vote → check already voted (409) → check membership (403) → append voter_id → UPDATE → INSERT `_vote` message → broadcast NewMsg + VoteUpdated → 201
      - `_vote_request` handler in `send_message` (`src/api/messages.rs`): when payload contains `_vote_request: true`, extract title + options array → create vote record (generate UUIDs for vote + each option) → replace message payload with `{ _vote: true, vote_id, title }`
    - Wire routes in `src/api/mod.rs`: `.route("/votes/{vote_id}", get(votes::get_vote)).route("/votes/{vote_id}/vote", post(votes::cast_vote))`
    - Also register in `src/lib.rs` if routes are nested there.
    - `src/ws/protocol.rs`: Add `VoteUpdated { vote_id: String, channel_id: String }` variant (next to `TrainUpdated`)
    - In `cast_vote`, broadcast both `NewMsg` and `VoteUpdated`
    - **my_vote computation**: after loading options, iterate `option.voter_ids` to find one containing `current_user_id`. Return `Some(option_id)` or `None`.
  - **Must NOT do**: Do NOT expose voter_ids in any API response. Do NOT allow vote changing (409 on second vote). Do NOT break existing `/train` handler in messages.rs.
  - **Parallelization**: Wave 1 | Blocked by: T1 | Blocks: T3, T6
  - **References**: `src/api/trains.rs` (exact same pattern — TrainRow FromRow, parse_replies→parse_options, into_response). `src/api/messages.rs` (the `/train` command handler block — model the `_vote_request` block after it, BEFORE the `match cmd` block). `src/ws/protocol.rs:65` (TrainUpdated variant — add VoteUpdated right after). `src/api/mod.rs` (route registration — add vote routes next to train routes).
  - **Acceptance criteria**: `cargo test` passes (all existing 194 + new vote tests); `cargo clippy -- -D warnings` clean.
  - **QA scenarios**:
    - Happy: create vote via `_vote_request` payload → GET returns correct options with count=0 + my_vote=None → cast vote → GET returns count=1 + my_vote=option_id
    - Failure: cast vote twice → 409. Cast vote as non-member → 403. GET non-existent vote → 404.
    - Anonymous: verify response JSON does NOT contain voter_ids anywhere.
    - Evidence `.omo/evidence/task-2-vote-command.txt`
  - **Commit**: Y | `feat(vote): backend vote API + command handler + WS event`

### Wave 2: Frontend

- [x] 3. Frontend API layer + types + WS subscription (TDD)
  - **What**:
    - `frontend/src/api/votes.ts` (NEW): `useVote(voteId)` query + `useCastVote()` mutation
    - `frontend/src/types/index.ts` (MODIFY): Add `Vote { id, channel_id, creator_id, title, options: VoteOption[], my_vote: string | null, created_at }` + `VoteOption { id, text, count }`
    - `frontend/src/hooks/useCursorSync.ts` (MODIFY): Subscribe to `vote_updated` → `queryClient.invalidateQueries(['vote', voteId])`
  - **Must NOT do**: Do NOT add voter_ids to the VoteOption TypeScript interface.
  - **Parallelization**: Wave 2 | Blocked by: T2 | Blocks: T4, T5, T6
  - **References**: `frontend/src/api/trains.ts` (exact same pattern). `frontend/src/types/index.ts` (Train + TrainReply interfaces — add Vote + VoteOption nearby). `frontend/src/hooks/useCursorSync.ts` (train_updated subscription block — add vote_updated right after).
  - **Acceptance criteria**: `bun vitest run` passes.
  - **QA scenarios**: `votes.test.ts` — mock apiClient, verify query/mutation call correct endpoints. Verify WS subscription calls invalidate.
  - **Commit**: Y | `feat(vote): frontend API layer + types + WS subscription`

- [x] 4. VoteBuilderModal — option builder modal (TDD)
  - **What**:
    - `frontend/src/components/VoteBuilderModal.tsx` (NEW):
      - Props: `{ isOpen, onClose, onConfirm: (title, options) => void, initialTitle }`
      - Title input (pre-filled with initialTitle, editable)
      - Dynamic option inputs: starts with 2 empty inputs, "+ 添加选项" adds more (max 10), each has remove (×) button (min 2)
      - "确认" button: disabled when options < 2 or any option text empty or title empty
      - "取消" button
      - Modal pattern: same `fixed inset-0 z-50 bg-black/60 backdrop-blur-sm` + centered panel as TrainRepliesModal
    - `frontend/src/components/VoteBuilderModal.test.tsx` (NEW):
      - Renders with initial title
      - Can add/remove options
      - Confirm disabled when < 2 options or empty
      - Confirm calls onConfirm with title + options array
      - Cancel calls onClose
  - **Must NOT do**: Do NOT send any API request from the modal — it only calls onConfirm callback. Do NOT allow > 10 options.
  - **Parallelization**: Wave 2 | Blocked by: T3 | Blocks: T6
  - **References**: `frontend/src/components/TrainRepliesModal.tsx` (modal scaffolding pattern — same container, backdrop, close button). No existing option-builder component to reference.
  - **Acceptance criteria**: `bun vitest run` passes.
  - **QA scenarios**: Render → verify title pre-filled → click "+ 添加选项" → new input appears → type in inputs → click "确认" → onConfirm called. Click "取消" → onClose called.
  - **Commit**: Y | `feat(vote): VoteBuilderModal for building vote options`

- [x] 5. VoteMessage component — bar chart card with vote buttons (TDD)
  - **What**:
    - `frontend/src/components/VoteMessage.tsx` (NEW):
      - Props: `{ voteId: string, title: string, channelId: string }`
      - `useVote(voteId)` query, `useCastVote()` mutation, `useAuthStore` for currentUserId
      - Card layout:
        - Header: 📊 + title (bold)
        - Options list: each option is a row:
          - Option text (left)
          - Bar container (flex-1, h-6, bg-gray-200 rounded): inner fill div with `style={{ width: percentage + '%' }}`, bg-blue-500, CSS transition-all duration-300
          - Count text: "N 票 (XX%)" inside or next to bar
          - Vote button (right): if `my_vote === null` → "投票" button enabled; if `my_vote === option.id` → green highlight "✓ 已投"; if `my_vote === other_option` → disabled
        - Footer: "共 N 人投票"
      - Cast vote: `useCastVote().mutateAsync({ voteId, optionId })` — on success, WS VoteUpdated auto-invalidates the query
    - `frontend/src/components/VoteMessage.test.tsx` (NEW):
      - Renders card with title
      - Shows options with bars
      - Vote button enabled when my_vote=null
      - Vote button calls mutation
      - Already voted → highlight voted option, others disabled
  - **Must NOT do**: Do NOT show voter names. Do NOT allow vote changing (no un-vote button). Do NOT use a charting library — pure CSS bars.
  - **Parallelization**: Wave 2 | Blocked by: T3 | Blocks: T6
  - **References**: `frontend/src/components/TrainMessage.tsx` (card scaffolding pattern, query/mutation hooks, hasJoined check → model after it for my_vote check). No existing bar chart component.
  - **Acceptance criteria**: `bun vitest run` passes.
  - **QA scenarios**: Render with mock vote data → verify bars, counts, percentages → click vote → mutation called → re-render with updated data → voted option highlighted.
  - **Commit**: Y | `feat(vote): VoteMessage card with bar chart and vote buttons`

- [x] 6. MessageBubble + MessageInput integration
  - **What**:
    - `frontend/src/components/MessageBubble.tsx` (MODIFY): In `renderContent()`, check `payload._vote === true` → return `<VoteMessage voteId={payload.vote_id} title={payload.title} channelId={message.channel_id} />`
    - `frontend/src/components/MessageInput.tsx` (MODIFY):
      - Add `{ cmd: 'vote', desc: '发起投票', args: true, argHint: '<标题>' }` to COMMANDS array
      - In command handling: if `cmd === 'vote'` → do NOT send → set `voteInitialTitle = args`, `voteModalOpen = true`, clear input
      - Add state: `const [voteModalOpen, setVoteModalOpen] = useState(false)`, `const [voteInitialTitle, setVoteInitialTitle] = useState('')`
      - Render `<VoteBuilderModal isOpen={voteModalOpen} onClose={() => setVoteModalOpen(false)} onConfirm={handleVoteConfirm} initialTitle={voteInitialTitle} />`
      - `handleVoteConfirm(title, options)`: send message with payload `{ _vote_request: true, title, options }`, close modal
    - Get current user ID from `useAuthStore` in MessageBubble (same as TrainMessage does)
  - **Must NOT do**: Do NOT send a regular message when `/vote` is typed — it must open the modal. Do NOT break `/train` command handling.
  - **Parallelization**: Wave 2 | Blocked by: T3, T4, T5 | Blocks: T7
  - **References**: `frontend/src/components/MessageBubble.tsx` (the `_train` payload detection block — add `_vote` right after). `frontend/src/components/MessageInput.tsx` (COMMANDS array, handleSend slash command detection — intercept `/vote` BEFORE the existing `_command` send logic).
  - **Acceptance criteria**: `bun vitest run` passes; `bun run build` clean.
  - **QA scenarios**: Type `/vote 测试` → modal opens (no message sent). Add options in modal → confirm → message sent with `_vote_request` payload. Receive message with `_vote` payload → VoteMessage renders.
  - **Commit**: Y | `feat(vote): integrate VoteMessage into MessageBubble + MessageInput`

### Wave 3: Verification

- [x] 7. Playwright E2E: create vote, cast vote, verify bar chart updates
  - **What**: Start servers → login → navigate to channel → type `/vote 午餐吃啥` → modal opens with title pre-filled → add 3 options (麦当劳/肯德基/其他) → click "确认" → message sent → VoteMessage card renders with title + 3 bars (all 0%) → click "麦当劳" vote button → bar updates to 100% (1/1) → button changes to "✓ 已投" (green) → other buttons disabled → total shows "共 1 人投票" → WS event updates all clients.
  - **Must NOT do**: Do NOT skip this step. Do NOT declare done without browser evidence.
  - **Parallelization**: Wave 3 | Blocked by: T6 | Blocks: nothing
  - **References**: All vote components. Previous train E2E pattern in `.omo/evidence/task-7-train-command.txt`.
  - **Acceptance criteria**: Screenshots or DOM snapshots evidence; no console errors; vote data correct in DB.
  - **QA scenarios**: Full flow above + verify second user can also vote (different option) → bars update to show both votes.
  - **Commit**: N | (evidence only, no code changes)
  - **Evidence**: `.omo/evidence/task-7-vote-command.txt`

## Constraints

- **msg_type stays `"text"`** — vote messages use `_vote` / `_vote_request` sentinel in payload (no schema change to messages CHECK constraint)
- **No new npm dependencies** — use existing React Query + Zustand + Tailwind. Bar chart is pure CSS.
- **Follow existing patterns**: slash command handling, modal scaffolding, WS broadcast, message payload conventions — all modeled after `/train`
- **TDD**: failing test first for every todo, then green
- **Must NOT break**: existing message rendering, slash commands, WS events, `/train` feature, file uploads
- **Anonymous**: voter_ids never exposed in any API response or TypeScript interface

## Files to Create/Modify

**New files:**
- `db/migrations/004_add_votes.up.sql` + `.down.sql`
- `src/api/votes.rs`
- `frontend/src/api/votes.ts` + `votes.test.ts`
- `frontend/src/components/VoteMessage.tsx` + `.test.tsx`
- `frontend/src/components/VoteBuilderModal.tsx` + `.test.tsx`

**Modified files:**
- `src/api/messages.rs` — add `_vote_request` handler in `send_message`
- `src/api/mod.rs` — register `/votes` routes
- `src/lib.rs` — register `/votes` routes (if nested here)
- `src/ws/protocol.rs` — add `VoteUpdated` variant
- `frontend/src/types/index.ts` — add `Vote` + `VoteOption` interfaces
- `frontend/src/components/MessageBubble.tsx` — detect `_vote` payload → render VoteMessage
- `frontend/src/components/MessageInput.tsx` — add `vote` to COMMANDS, intercept `/vote` to open modal
- `frontend/src/hooks/useCursorSync.ts` — subscribe to `vote_updated`
