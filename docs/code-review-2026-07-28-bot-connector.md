# 代码审查报告 — Bot Connector & 消息引用功能

| 项目 | 值 |
|------|-----|
| **审查日期** | 2026-07-28 |
| **提交范围** | `cb6060f..HEAD`（共 10 个提交，作者 ipconfiger） |
| **变更规模** | 27 文件，+1810 / -258 行 |
| **主要特性** | Bot Connector（WebSocket 模式）、消息引用（inline preview）、消息框布局重构、Hermes Agent 网关插件 |
| **审查方式** | 三通道并行专家审查（后端 Rust/SQL、Python/部署基础设施、前端 React） |
| **总体结论** | ⚠️ **不建议直接合并/部署**。存在 4 个 CRITICAL 与 1 个隐藏 panic，其中 BotMention 广播模型存在根本性隐私泄露缺陷，需在部署前修复。 |

---

## 审查范围

### 后端 Rust / SQL
| 文件 | 改动 | 说明 |
|------|------|------|
| `src/ws/mod.rs` | +205 全新 | WebSocket 端点，含用户 WS 与 bot WS（`/ws/bot`） |
| `src/ws/protocol.rs` | +16 | ws 协议定义（ServerEvent / ClientEvent） |
| `src/api/messages.rs` | +207 | 消息引用功能、bot mention 分发、ack 占位符 |
| `src/api/admin/bots.rs` | +54 | bot 连接模式管理 API |
| `src/api/trains.rs` / `src/api/votes.rs` | 各 +3 | schema 同步（`quoted_message_id`） |
| `src/lib.rs` | +1 | `/ws/bot` 路由注册 |
| `db/migrations/011_add_quote_reference.{up,down}.sql` | 新增 | 引用外键 |
| `db/migrations/012_add_bot_connection.{up,down}.sql` | 新增 | bot 连接表 |

### 前端 React / TypeScript
| 文件 | 改动 |
|------|------|
| `frontend/src/components/MessageBubble.tsx` | +93 |
| `frontend/src/components/MessageInput.tsx` | +42 |
| `frontend/src/components/MessageList.tsx` | +5 |
| `frontend/src/components/TextMessage.tsx` | +5 |
| `frontend/src/pages/admin/AdminBotsPage.tsx` | +57 |
| `frontend/src/pages/ChannelListPage.tsx` | +6 |
| `frontend/src/types/index.ts` | +1 |
| `frontend/src/api/admin.ts` | +7 |
| `frontend/src/api/channels.ts` | +2 |

### Python 工具 / 部署
| 文件 | 改动 |
|------|------|
| `tools/bot-connector.py` | +171 全新 |
| `tools/bot-connector.service` | +55 |
| `tools/bot-connector.yaml` | +69 |
| `tools/hermes-vast-plugin.py` | +256 全新 |

---

## 问题统计

| 级别 | 后端 | Python/部署 | 前端 | 合计 |
|------|------|-------------|------|------|
| 🔴 CRITICAL | 3 | 1 | 0 | **4** |
| ⚡ 隐藏 panic | 1 | 0 | 0 | **1** |
| 🟠 HIGH | 5 | 4 | 4 | **13** |
| 🟡 MEDIUM | 7 | 8 | 7 | **22** |
| 🟢 LOW | 5 | 7 | 5 | **17** |

---

# 🔴 CRITICAL（部署前必修）

## C1. BotMention 事件广播到所有 WebSocket 连接，泄露 system_prompt / 模型名 / 整个频道历史

| 项 | 内容 |
|----|------|
| **位置** | `src/api/messages.rs:891-900` + `src/ws/mod.rs:142-145, 549-575` |
| **类型** | 隐私泄露 / 权限越界 |

**问题**

当用户 @mention 一个 connector 模式的 bot 时：

```rust
let _ = ws_pool.global_tx.send(ServerEvent::BotMention {
    mention_id, channel_id, messages, model, system_prompt,
});
```

该事件进入 `global_tx`，而**每个**用户连接都在 `register()` 中订阅了 `global_tx`（`src/ws/mod.rs:110`）。`handle_socket` 仅过滤 self-typing（`src/ws/mod.rs:555-563`），对 `BotMention` 无任何过滤——直接序列化下发给客户端。

**根因**

`broadcast_to_channel` / `notify_channel` 名字暗示按频道分发，但实际上 `_channel_id` 参数被完全忽略（`src/ws/mod.rs:142`），所有 ServerEvent 都通过单条全局 broadcast 扇出。`BotMention` 是本次新增的、含高敏感字段的事件，直接被旧的"全局广播"模型污染。

**影响**

- **隐私泄露**：任意已登录用户（哪怕不在该频道）都能收到频道完整消息历史（`messages: Vec<Value>`，由 `src/api/messages.rs:846-871` 拼装，**无 LIMIT**）。
- **Prompt 泄露**：bot 的 `system_prompt` 和 `model` 暴露给所有客户端——通常含 persona、安全规则、商业机密。
- 体积放大：每次 mention 都把整个频道历史重发 N 份（N = 在线连接数）。

**修复建议**

1. **立即修复（隔离，1 行）**：在 `handle_socket` 的广播分支显式跳过 `BotMention`，让该事件只走 bot 通道：
   ```rust
   if matches!(event, ServerEvent::BotMention { .. }) { continue; }
   ```
2. **正确修复**：`BotMention` 应携带 `bot_user_id`（见 C2），由 `handle_bot_socket` 校验后处理；分发不应走 `global_tx`，而应通过 `user_connections` map 直接找到目标 bot 的连接，向其专属通道发送（参考已有的 `user_connections: DashMap<UserId, DashSet<ConnectionId>>`，新增 `bot_tx: DashMap<UserId, mpsc::Sender<ServerEvent>>`）。
3. 历史拼装加 `LIMIT 50 ORDER BY created_at DESC`（见 H3）。

---

## C2. 多 connector bot 共存时，每个 bot 都会回复每次 mention（回复风暴）

| 项 | 内容 |
|----|------|
| **位置** | `src/ws/protocol.rs:96-102` + `src/api/messages.rs:893` + `tools/bot-connector.py:57-59` |
| **类型** | 协议设计缺陷 / 正确性 |

**问题**

`BotMention` 不携带"目标 bot 是谁"的字段。所有 connector bot 的 WS 连接都订阅同一条 `global_tx`，因此每个 connector 都会收到每个 mention。`bot-connector.py` 的处理：

```python
if etype == "bot_mention":
    await on_mention(ws, event, llm, system_prompt)   # 不校验目标 bot
```

没有过滤——每个 connector 都会调自己的 LLM，发回 `BotReply`，导致 N 个回复。

**根因**

`ServerEvent::BotMention`（`src/ws/protocol.rs:96-102`）缺少 `bot_id` / `bot_user_id` 字段，所以即便 connector 想过滤也没办法。

**影响**

N 个 connector bot 配置时，每次 mention 产生 N 条回复；同一 mention_id 触发多次消息插入。

**修复建议**

1. 在 `ServerEvent::BotMention` 增加 `bot_user_id: String`（或 `bot_id`）。
2. 在 `handle_bot_socket` 收到 BotMention 时校验 `event.bot_user_id == self.bot_user_id`，不匹配则丢弃。
3. 在 `tools/bot-connector.py` 也加一道防御性检查（针对 mention_id 记录是否已处理）。

---

## C3. 用户 WebSocket 接收并下发所有频道的事件（跨频道隐私泄露，BotMention 放大）

| 项 | 内容 |
|----|------|
| **位置** | `src/ws/mod.rs:142-145, 163-165` |
| **类型** | 隐私泄露 / 权限越界（既有缺陷，本次加重） |

**问题**

`broadcast_to_channel(_channel_id, event)` 把 `_channel_id` 标为 `_` 完全忽略。任何用户加入 `register()` 后都收到**所有频道**的 `NewMsg`/`MsgDeleted`/`ThreadReply` 等，包括自己没加入的私密频道。

**根因**

历史架构选择——单 global broadcast channel，所有过滤推给客户端。`get_channel_members` 存在但只用于查询，未用于路由。

**影响**

严格说这是 PR 之前就存在的问题，但**本次新增 `BotMention` 后，该缺陷升级为隐私事件**（见 C1）。本次未修复、反而加重。

**修复建议**

- 中长期应把 `global_tx` 改成 `per_connection mpsc` 或 `per_channel broadcast`，按 `subscribed_channels` 路由。
- 短期至少把 BotMention 隔离（见 C1 修复）。

---

## C4. connection_key 在日志中被完整泄露

| 项 | 内容 |
|----|------|
| **位置** | `tools/hermes-vast-plugin.py:78-79` |
| **类型** | 凭证泄露 |

**问题**

```python
url = f"{self._cfg.ws_url}?key={self._cfg.connection_key}"
log.info("Connecting to VAST: %s", url[:80])
```

截断阈值是 80 字符，但典型 URL 长度根本不到 80：
- `ws://localhost:3000/ws/bot?key=` = 33 字符
- UUID4 connection_key = 36 字符
- 合计 **69 字符**，**整个 URL 含 key 完整落入 INFO 日志**。

**根因**

开发者误以为"截断 80"能保护 key，没做长度核算；把凭证拼进 URL 后再用字符串截断来"打码"本身就是错误范式。

**影响**

任何能读 journal / 日志文件的人（含 `journalctl --user`、运维、日志聚合系统、S3 归档）都拿到 bot 的唯一长期凭证。服务端 `bots.rs:120` 显示该 key 只在创建时返回一次 → 泄露后只能重新建 bot 才能轮换。

**修复建议**

```python
log.info("Connecting to VAST: %s (key=%s...)", self._cfg.ws_url, self._cfg.connection_key[:8])
```

永远不要把含凭证的 URL 整体入参。同样问题需检查 `bot-connector.py`。

---

# ⚡ 隐藏 panic（紧急，建议提级到 HIGH）

## P1. `extract_preview` 字节切片在中文/emoji 边界 panic

| 项 | 内容 |
|----|------|
| **位置** | `src/api/messages.rs:166-170` |
| **类型** | 生产 panic / 可用性 |

**问题**

```rust
if s.len() > 100 {
    format!("{}...", &s[..100])   // 字节切片，非字符切片
}
```

对于含中文/emoji 的消息，若第 100 字节落在多字节字符中间，`&s[..100]` 直接 panic（`"byte index is not a char boundary"`）。中文用户发长消息时会直接 panic 整个请求 handler。

**修复建议**

```rust
format!("{}...", s.chars().take(100).collect::<String>())
```

与 `src/ws/mod.rs:392` 的 `text.chars().take(100).collect()` 风格保持一致。

---

# 🟠 HIGH（合并前应修）

## 后端 Rust

### H1. 生产代码 `unwrap()` / `expect()` —— 违反项目红线

| 项 | 内容 |
|----|------|
| **位置** | `src/ws/mod.rs:399, 493-494` |
| **红线** | 生产代码禁止 `unwrap()` |

```rust
// src/ws/mod.rs:399 (handle_bot_socket, bot ping 分支)
let pong = serde_json::to_string(&ServerEvent::Pong).unwrap();

// src/ws/mod.rs:493-494 (handle_socket, user ping 分支)
let pong = serde_json::to_string(&ServerEvent::Pong)
    .expect("Pong is always serializable");
```

Pong 是单元变体，序列化当前确实不会失败——但项目规约明确禁止。两处风格还不一致（bot 路径 `unwrap()`，用户路径 `expect()`）。

**修复建议**：统一用 `unwrap_or_else(|_| "{\"type\":\"pong\"}".to_string())` 或直接构造常量字符串。

---

### H2. `handle_bot_socket` 断开时双重广播 presence-offline

| 项 | 内容 |
|----|------|
| **位置** | `src/ws/mod.rs:455-461` |

```rust
let presence = ServerEvent::Presence { user_id: bot_user_id.clone(), status: "offline".into() };
let _ = pool.global_tx.send(presence);     // 第 1 次
pool.unregister_bot(&bot_user_id, &connection_id);  // 内部再发一次！
```

`unregister`（`src/ws/mod.rs:127-131`）已发送 `Presence { status: "offline" }`，`unregister_bot` 又委托给它，所以发两次。对比 `handle_socket` 没有此 bug（只调 `unregister`）——这是 bot 路径的回归。

**影响**：客户端收到两次 offline，可能引起 UI 闪烁；多余网络流量。

**修复建议**：删除 `handle_bot_socket` 第 455-460 行的手动 presence 广播，只保留 `unregister_bot`。

---

### H3. Connector 模式构建 context 时无 LIMIT，长频道 OOM / 卡死 WS

| 项 | 内容 |
|----|------|
| **位置** | `src/api/messages.rs:846-855`（同样问题 `941-950`） |

```rust
let context_rows = sqlx::query_as(
    "SELECT ... WHERE m.channel_id = ? AND m.deleted_at IS NULL
     ORDER BY m.created_at ASC",   // 没有 LIMIT
)
```

频道积累数千条消息后，每次 mention 都 SELECT 全表、序列化为 JSON、塞进 `BotMention` 经 WS 发出。该 payload 还会广播给所有连接（见 C1），放大效应。单帧可能超 axum/tungstenite 默认 `max_message_size`（64 MiB）。

**修复建议**：`ORDER BY m.created_at DESC LIMIT 50`，然后在 Rust 里 reverse，保留最新 50 条 + 含 mention 上下文。或限制总字符数。

---

### H4. ack 发出后无超时/无补救 —— bot 断线时用户被承诺响应却永远收不到

| 项 | 内容 |
|----|------|
| **位置** | `src/api/messages.rs:814-900` |

Connector 模式流程：
1. 检查 `is_user_online` → true（line 827）
2. 插入 ack 文本"收到，X 正在处理..."（line 874-889）
3. `global_tx.send(BotMention)`（line 893）

如果在 1 和 3 之间 bot 连接断开（网络抖动），`global_tx.send` 仍返回 Ok（broadcast 没有订阅者也 Ok），但**没有 bot 接收者**——`broadcast::Receiver` 在 `handle_bot_socket` 退出时被 drop。Ack 已写入数据库、客户端已显示，永远没有后续 reply。

**根因**：broadcast channel 的"无订阅者"是静默的；ack 文本承诺"正在处理"，但没有 cancellation/超时机制。

**修复建议**：
- 在 `global_tx.send` 之前检查 `receiver_count()` 或更精确地查 `user_connections[bot_user_id]`；
- 或在 `BotMention` 中加 `deadline`，由 `handle_bot_socket` 在 deadline 过期后自己插入一条"⚠️ X 响应超时"的消息。

---

### H5. `register_bot` 丢弃 receiver 后又重新 subscribe

| 项 | 内容 |
|----|------|
| **位置** | `src/ws/mod.rs:348, 358` |

```rust
let _ = pool.register_bot(&bot_user_id, &connection_id);  // 丢弃 receiver
...
let mut broadcast_rx = pool.global_tx.subscribe();         // 重新订阅
```

对比 `handle_socket`（line 480）正确使用了 `register` 返回的 receiver。两处不一致。register 与 subscribe 之间有微秒级窗口可能丢失事件。

**修复建议**：改为 `let mut broadcast_rx = pool.register_bot(&bot_user_id, &connection_id);`，删除 line 358。

---

## 前端 React

### H6. 引用状态在切换频道时不重置 → 跨频道误发 + 已输入文本丢失

| 项 | 内容 |
|----|------|
| **位置** | `ChannelListPage.tsx:45` + `MessageInput.tsx:66-94` |

`quotingMessage` 存放在 `ChannelListPage` 的 state 中，但 `channelId` 变化时它被保留（effect 只同步 `currentChannel`，不清除 quote）。

**复现路径**：
1. 在 #general 对消息 A 点引用 → quote bar 显示 A
2. 切换到 #random（quote bar 仍显示 #general 的消息 A）
3. 用户输入文字按回车 → `handleSend` 同步执行 `setText('')`（line 93）和 `onCancelQuote?.()`（line 94）
4. 请求携带 `quoted_message_id` 指向 #general 的消息，被后端"同频道校验"拒绝
5. **用户的文字和引用都已清空，mutation 报错，消息彻底丢失，无法重试**

**修复建议**：
```tsx
// ChannelListPage.tsx
useEffect(() => { setQuotingMessage(null) }, [channelId])
```
并在发送失败时恢复输入文本（见 H7）。

---

### H7. 发送为"乐观清空"——失败即丢引用与文本

| 项 | 内容 |
|----|------|
| **位置** | `MessageInput.tsx:92-98` |

```ts
sendMessage.mutate({ ..., quoted_message_id: quoteId })
setText('')          // line 93
onCancelQuote?.()    // line 94 —— 本次新增
```

`mutate` 是 fire-and-forget，立即清空文本和引用。任何发送失败（网络、后端校验、H6 跨频道拒绝）都会让用户的输入和引用同时消失且无重试入口。本次提交把 `onCancelQuote?.()` 加进了这个"乐观清空"链路，放大了既有问题。

**修复建议**：改用 `mutateAsync` 在 `onSuccess` 中清空，`onError` 保留文本与引用并 toast 提示；或在 `useSendMessage` 的 mutation options 里处理。

---

### H8. 连接模式 Key 一次性展示 + 静默吞剪贴板错误 → 凭据永久丢失

| 项 | 内容 |
|----|------|
| **位置** | `AdminBotsPage.tsx:57-67` |

```ts
if (created.connection_key) {
  navigator.clipboard.writeText(created.connection_key).catch(() => {})
}
```

- `connection_key` 是一次性凭据（toast 明确写 "won't be shown again"）。
- 该调用在 mutation 的 async `onSuccess` 回调里，**不在用户手势栈内**，浏览器（Safari/Firefox）常因 "Clipboard write was blocked due to lack of user activation" 拒绝。
- 非 HTTPS 上下文 `navigator.clipboard` 为 `undefined` → 抛 `TypeError` → 被 `.catch(() => {})` 静默吞掉。
- 结果：toast 误导用户"已复制"，但 key 既没复制、也不再展示 → **凭据不可恢复**，用户无法配置 connector bot。

**修复建议**：用结果模态展示 key + 手动"复制"按钮（按钮点击即用户手势），复制失败时降级为 `select` 文本；不要在 toast 里宣称已复制。

---

### H9. 引用 id 被附加到斜杠命令消息上（语义错误）

| 项 | 内容 |
|----|------|
| **位置** | `MessageInput.tsx:77-88` |

```ts
sendMessage.mutate({ msg_type: 'text', payload: { _command: true, command: cmd, args }, quoted_message_id: quoteId })
```

`/quit`、`/list`、`/kick`、`/train` 这类命令也携带 `quoted_message_id`。引用一条消息来执行 `/quit` 毫无意义，且会在消息记录里留下指向被引用消息的 quote 引用，污染数据。`/vote` 路径（line 77-85）更不一致——它 `return` 早退，**既不发送 quoteId 也不清除 quote**，导致 quote bar 残留。

**修复建议**：仅对普通文本/文件/代码消息附加 quoteId；命令分支不附加。并统一各分支的 quote 清除时机。

---

## Python / 部署

### H10. connection_key 通过 URL query string 传输（服务端共谋）

| 项 | 内容 |
|----|------|
| **位置** | `tools/bot-connector.py:45`、`tools/hermes-vast-plugin.py:78` + 服务端 `src/ws/mod.rs:287-290` |

key 走 `?key=...`，会被记录在：反向代理 access log、VAST 进程 stdout/tracing、中间防火墙/IDS 日志、任何中间跳板的 Referer。

**根因**：服务端用 `Query` 解析，逼着客户端把凭证放 URL；客户端未尝试用 `Sec-WebSocket-Protocol` 或自定义 header 兜底。

**修复建议**：
1. **服务端**：增加对 `Authorization: Bearer <key>` 或 `Sec-WebSocket-Protocol` 的支持，废弃 query 参数。
2. **过渡期客户端**：连接成功后立即在日志里回避；文档强警示部署侧不要在共享代理层记录 query。
3. key 一旦疑似泄露，提供 admin 端 "rotate connection_key" 接口（当前 `bots.rs` 没有这个能力，是一个缺口）。

---

### H11. 配置文件模板被 git 跟踪，引导用户写入明文凭证

| 项 | 内容 |
|----|------|
| **位置** | `tools/bot-connector.yaml`（已确认 `git ls-files` 命中），`.gitignore` 仅忽略 `tools/__pycache__/` |

- 文件被提交；模板里明确写出 `api_key: "sk-your-api-key"`、`api_key: ""`，引导用户直接编辑此文件填真值。
- `.gitignore` 没有 `bot-connector.local.yaml` / `*-secrets.yaml` 之类的逃生舱模式。
- 脚本本身不支持 `${VAST_KEY}` / `${OPENAI_API_KEY}` 之类的环境变量插值。

**修复建议**：
1. 仓库内只保留 `bot-connector.example.yaml`；在 `.gitignore` 加 `tools/bot-connector.yaml`、`tools/*.local.yaml`。
2. `bot-connector.py` 增加最小代价的 env 插值（`os.path.expandvars`，或对 `connection_key`/`api_key` 字段单独支持 `!env VAR` tag）。
3. systemd unit 加 `EnvironmentFile=/etc/vast/bot-connector.env`，把凭证挪出去。

---

### H12. Hermes 插件 read loop 异常退出后状态不一致

| 项 | 内容 |
|----|------|
| **位置** | `tools/hermes-vast-plugin.py:98-111` |

```python
except Exception:
    log.exception("Error in VAST read loop")
    break
```

走 generic Exception 分支 break 后，`self._connected` 仍为 `True`、`self._ws` 仍非 None，但 reader 已死。Hermes 网关再调 `send()`（line 158）只检查 `self._ws and self._connected`，会向一个半死连接写，写入可能挂起或抛错，且无重连。

**修复建议**：
```python
except Exception:
    log.exception("Error in VAST read loop")
finally:
    self._connected = False
    self._ws = None
```
并由 Hermes 网关侧重连；或在插件内自身实现 `while self._connected: connect+read` 外层循环。

---

### H13. LLM api_key 仅以明文存于 YAML，无任何 env 兜底

| 项 | 内容 |
|----|------|
| **位置** | `tools/bot-connector.py:99` |

```python
headers={"Authorization": f"Bearer {llm.get('api_key', '')}"}
```

API key 落盘到 `bot-connector.yaml`，受 `ProtectSystem=strict` 保护但仍位于 `/opt/vast/tools`（systemd 中 `ReadWritePaths=/opt/vast/tools`，意味着 root 与同机其他能写此目录的服务都能读）。脚本不支持从环境读取。

**修复建议**：同 H11；并审查 systemd `ReadWritePaths` 范围（见 M-部署-1）。

---

# 🟡 MEDIUM（摘要）

## 后端 Rust

| ID | 问题 | 位置 |
|----|------|------|
| M-Rust-1 | `unwrap_or_default()` 吞关键 DB 查询错误（DB 抖动时静默返回空，用户 @mention 后无反应无日志） | `messages.rs:374,449,587,691,798,855,950,1073` |
| M-Rust-2 | `BotReply` 路径静默吞 DB 写错误，bot 以为成功但服务端没存，前端永不显示 | `ws/mod.rs:374-396` |
| M-Rust-3 | `pending_mention` 死变量（写但从不读），clippy 会报 unused | `ws/mod.rs:360,428` |
| M-Rust-4 | `serde_json::to_string(&event).unwrap_or_default()` 会发送空字符串（无效 JSON 帧，与 handle_socket 不一致） | `ws/mod.rs:430` |
| M-Rust-5 | `dashmap::DashSet` 嵌套持锁，`register` 与 `unregister` 锁序不一致，潜在 AB-BA 死锁 | `ws/mod.rs:95-124` |
| M-Rust-6 | `_membership` 命名误导（实际有用，`_` 前缀暗示未使用） | `messages.rs:220,310` |
| M-Rust-7 | `/ws/bot` 鉴权无审计、无频率限制（可暴力枚举 connection_key） | `ws/mod.rs:285-329` |

## Python / 部署

| ID | 问题 | 位置 |
|----|------|------|
| M-部署-1 | systemd `ReadWritePaths=/opt/vast/tools` 过宽，运行用户可改写自身脚本（RCE 后持久化）；服务无需写文件，**直接删除该行** | `bot-connector.service:47` |
| M-部署-2 | systemd 缺失多项标准硬化（`CapabilityBoundingSet=`、`MemoryDenyWriteExecute=yes`、`SystemCallFilter=@system-service` 等） | `bot-connector.service` |
| M-部署-3 | `Restart=always` 无重启频率上限，配置错误时 5s 一次无限重启刷爆日志；加 `StartLimitIntervalSec=300`/`Burst=10` | `bot-connector.service:39-40` |
| M-部署-4 | `reconnect_interval` 配置被静默忽略（硬编码 `sleep(5)`）——配置幻觉 | `bot-connector.yaml:14` + `bot-connector.py:71` |
| M-部署-5 | `bot-connector.py` 缺少输入校验，配置错误以 KeyError 暴露 | `bot-connector.py:39,127-128` |
| M-部署-6 | `except Exception` 捕获编程错误后无限重试，掩盖根因；缩窄到 `(WebSocketException, OSError, TimeoutError, HTTPError)` | `bot-connector.py:69-71` |
| M-部署-7 | `shutdown` 函数缺少类型注解（违红线） | `bot-connector.py:143` |
| M-部署-8 | `bot-connector.yaml` 默认 `ws://` 明文，应默认 `wss://` 并对非 localhost 的 `ws://` 发 WARNING | `bot-connector.yaml:11` |

## 前端 React

| ID | 问题 | 位置 |
|----|------|------|
| M-FE-1 | 引用预览仅依赖内存 store 无回填，被引用消息滚出窗口就永久显示"原消息不可用"；应按 id 懒加载 | `MessageBubble.tsx:280-285` |
| M-FE-2 | **布局隐性回归**：自己消息文本对齐由 `right` 改为 `left`（commit 未提及）——需产品确认是否有意 | `MessageBubble.tsx:425` |
| M-FE-3 | **布局隐性回归**：`RawContentPreview` 失去 `sm:w-1/2` 响应式半宽——需产品确认 | `TextMessage.tsx:95` |
| M-FE-4 | 引用预览卡片 `truncate` 在 flex 中不生效（缺 `min-w-0`/`flex-1`），长文本溢出 | `MessageBubble.tsx:344-358` |
| M-FE-5 | 引用按钮对所有消息类型都显示（含系统/命令/入群请求消息） | `MessageBubble.tsx:411-419` |
| M-FE-6 | 代码/文件/投票发送路径忽略引用却不清除 quote bar，状态与可见 UI 不一致 | `MessageInput.tsx:222-231,283-300` |
| M-FE-7 | 类型安全：`Message.id: string` 与后端 `i64` 类型谎言，靠 `Number(id)` 打补丁；建议后端序列化为字符串 | `types/index.ts:33,44` + `MessageBubble.tsx:284` |

---

# 🟢 LOW（摘要）

## 后端 Rust
| ID | 问题 | 位置 |
|----|------|------|
| L-Rust-1 | `bot-connector.py` 重连固定 5s，无指数退避/jitter（thundering herd） | `bot-connector.py:67-71` |
| L-Rust-2 | `011/012` down.sql 空注释，无法回滚（规约要求可回滚） | `db/migrations/011,012` |
| L-Rust-3 | `bot_user_id` 类型与 `UserId` 别名混用（一致性差） | `ws/mod.rs:209,215,342,461` |

## Python / 部署
| ID | 问题 | 位置 |
|----|------|------|
| L-部署-1 | Hermes 插件 `asyncio.create_task` 结果未持有，可能被 GC 且异常未消费 | `hermes-vast-plugin.py:90` |
| L-部署-2 | `error=str(exc)` 暴露内部异常文本给上层结果对象（可能含路径/主机名） | `hermes-vast-plugin.py:178-180` |
| L-部署-3 | `_stream_callback` 类型注解过松 `Optional[Callable]` | `hermes-vast-plugin.py:71` |
| L-部署-4 | 无消息大小上限（`recv()` 无限制），可被恶意推送耗尽内存 | `hermes-vast-plugin.py:102` |
| L-部署-5 | `event["mention_id"]` 直接索引访问，缺字段时 KeyError 触发 read_loop break | `hermes-vast-plugin.py:119` |
| L-部署-6 | `import argparse` 写在 `__main__` 内（违 PEP 8） | `bot-connector.py:162` |
| L-部署-7 | URL 规范化脆弱（`rstrip("/")` + `removesuffix` 大小写敏感） | `bot-connector.py:128` |

## 前端 React
| ID | 问题 | 位置 |
|----|------|------|
| L-FE-1 | `!= null` 违项目 JS 红线（禁 `==`/`== null` 判断对象），改 `!== null` | `MessageInput.tsx:87,90` + `MessageBubble.tsx:341` |
| L-FE-2 | a11y：引用按钮仅 hover 可见，键盘/触屏不可达（缺 `focus-within:opacity-100`） | `MessageBubble.tsx:410-411` |
| L-FE-3 | i18n 不一致：中文 UI 中混用英文 aria-label/toast | `MessageBubble.tsx:415` 等 |
| L-FE-4 | 引用卡片缺语义化结构（应用 `<blockquote cite>`） | `MessageBubble.tsx:344-361` |
| L-FE-5 | INFO：store 选择器在频道消息更新时对带 quote 的气泡触发重渲染（性能备案，影响可忽略） | `MessageBubble.tsx:281-285` |

## INFO（依赖兼容性）
| ID | 问题 | 位置 |
|----|------|------|
| I-1 | `WebSocketClientProtocol` 类型注解在 `websockets >= 14` 将 ImportError（移至 `ClientConnection`）；建议 `TYPE_CHECKING` 块注解 | `bot-connector.py:75` + `hermes-vast-plugin.py:69` |
| I-2 | 服务端 connection_key 不可轮换，缺 `POST /api/admin/bots/:id/rotate-key` | `src/api/admin/bots.rs` |

---

# ✅ 已核实良好的方面

**后端**
- **SQL 注入**：所有用户输入都用 `.bind()` 参数化，包括动态 `IN (...)` 子句的 `format!("?{}", i+1)`（占位符非值，安全）。`bots.rs` 的 `format!("SELECT {SQL_COLUMNS} ...")` 中 `SQL_COLUMNS` 是代码常量，安全。
- **JWT 校验**：`/ws`（用户）正确调用 `validate_token` + `verify_user_epoch`，错误返回 401 JSON。
- **`011_add_quote_reference.up.sql`**：外键 `ON DELETE SET NULL` 正确；`send_message` 中 quoted_message 校验查 `deleted_at IS NULL` 且跨频道拒绝，逻辑完整。
- **`012_add_bot_connection.up.sql`**：`CHECK(connection_mode IN ('http','connector'))` + `WHERE connection_key != ''` 的 partial unique index 设计正确。
- **`src/api/trains.rs` / `src/api/votes.rs` (+3 行)**：仅在 RETURNING 列和 `ServerEvent::NewMsg` 字段加 `quoted_message_id: None`，纯 schema 同步，无回归。
- **`src/lib.rs` (+1 行)**：`/ws/bot` 路由正确注册，`TimeoutLayer(30s)` 仅作用于 upgrade 响应，不影响长连接。
- **Admin bot CRUD**：`api_key` 不在任何 BotView 中返回（仅 create 时单独返回 `connection_key`），审计日志齐全。
- **`send_message` 主流程**（消息插入、广播、push、引用校验）：核心逻辑正确。

**前端**
- **XSS 安全**：引用预览 `getQuotedPreview` 返回纯文本作为 React 子节点渲染（自动转义）；`ChatMarkdown` 经 `rehype-sanitize` + 协议白名单，引用功能未引入 `dangerouslySetInnerHTML`。**无 XSS 风险**。
- **无新增 `var` / 显式 `any`**（既有 `as any` 非本次引入）。
- **zustand 竞态**：`addMessage` 用 `msg_id` 去重，WS 重复事件安全。
- **AdminBotsPage 表单校验**：connector 模式下放宽 URL 必填且提交时显式置空，逻辑自洽。
- **deleted_at 处理**：引用卡片区分"已删除"与"不可用"两种态，体验细致。

**Python / 部署**
- `bot-connector.py:124` 使用 `with open(...)` ✓（符合红线）。
- 无 `import *`、无裸 `except:`（都是 `except Exception`）✓。
- 无 `subprocess`、无 `shell=True` ✓。
- 无 SQL 字符串拼接 ✓。
- systemd `User=vast` 非 root ✓、`NoNewPrivileges=yes` ✓、`PrivateTmp=yes` ✓。
- `bot-connector.py:96` 用 `async with httpx.AsyncClient(...)` ✓ 资源释放规范。

---

# 📋 建议修复顺序（按 ROI）

| 优先级 | 项 | 工作量 |
|--------|----|----|
| **立即** | C1（用户 WS 跳过 BotMention，1 行）+ P1（preview 字符切片）+ C4（删日志截断） | 各 5 分钟 |
| **本周** | C2（协议加 `bot_user_id` 双端过滤）· H6/H7（引用状态机）· H8（凭据展示改模态）· H3（context LIMIT）· H1（unwrap） | 各 5–30 分钟 |
| **本周** | H11（拆 example + env 插值）· H12（finally 重置 `_connected`）· M-部署-1 + M-部署-2（systemd 硬化） | 1–2 小时 |
| **跟进** | C3 / H10（协议层 header 鉴权，需服务端联动）· M-FE-7（id 类型契约对齐）· M-Rust-5（DashMap 锁序）· 跨频道路由重构 | 半天起 |

---

*报告由三通道并行专家审查（@oracle × 2 + @designer × 1）综合而成。如需某项的落地补丁草案，指明问题编号即可。*
