# VAST Bot Connector 设计方案

## 一、背景与目标

### 当前问题

VAST 的 Bot 集成采用 **HTTP 反向调用** 模式：当用户在频道中 `@mention` Bot 时，VAST 通过 `reqwest` 向 Bot 配置的 `api_url` 发起 `POST /v1/chat/completions` 请求。这要求 Bot 所在的机器 **具备公网可达的 IP**，而在自托管场景下（家庭宽带、内网服务器、NAT 后），Bot 无法被 VAST 访问。

### 目标

提供一种 **连接器（Connector）** 模式：Bot 以 WebSocket 客户端身份主动出站连接 VAST，VAST 通过这条已建立的持久连接推送 `@mention` 事件并接收回复。

### 核心思路

VAST 只需要暴露一个干净的 Bot WebSocket 端点。复杂度推到连接器端——连接器可以是独立脚本，也可以是 Hermes Agent Gateway Plugin。VAST 唯一的额外职责是：**当 Bot 离线时给出友好提示**。

### 架构概览

```
┌─────────────┐   WS /ws/bot?key=xxx      ┌──────────────────┐
│    VAST     │ ◄────────────────────────► │  Hermes Plugin    │
│             │   BotMention {context}     │  (Gateway 插件)  │
│   (轻量)    │ ◄────────────────────────► │                  │
│             │   BotReply {text}          │  复用 Hermes 的   │
│  只需处理   │                            │  skills/memory/  │
│  WS + 离线  │                            │  user-model 等   │
└─────────────┘                            └──────────────────┘

或降级为：

┌─────────────┐   WS /ws/bot?key=xxx      ┌──────────────────┐
│    VAST     │ ◄────────────────────────► │  standalone.py   │
│             │   BotMention {context}     │  (~200 lines)     │
│             │ ◄────────────────────────► │                  │
│             │   BotReply {text}          │  仅调用 LLM API  │
└─────────────┘                            └──────────────────┘
```

**关键特性**：

- Bot 仅需**出站**网络连接 —— 消除公网 IP 需求
- WS 连接是**持久**的 —— 断线自动重连
- 当前 HTTP 模式**向后兼容** —— 已有的 HTTP Bot 继续工作
- **离线友好提示** —— Bot 不在线时，用户能看到明确的状态提示

---

## 二、在 VAST 中添加 Bot

### 2.1 创建 Bot 流程

**入口**：管理控制台 -> Bots 页面 -> "Create Bot"

**创建表单字段**：

| 字段 | 必填 | 说明 |
|------|------|------|
| **Name** (`name`) | 是 | Bot 的唯一标识名，也是 `@mention` 关键词（如 `hermes`） |
| **Display Name** | 否 | Bot 的显示名称 |
| **Connection Mode** | 是 | `HTTP` 或 `Connector` |
| **API URL** (`api_url`) | HTTP 模式必填 | LLM 端点地址 |
| **API Key** (`api_key`) | 否 | API 访问密钥（服务端存储，界面不可见） |
| **Model** (`model`) | 否 | LLM 模型名称，默认 `hermes` |
| **System Prompt** | 否 | 系统提示词 |
| **Connector Key** | Connector 模式自动生成 | 连接器认证密钥，创建时一次性显示 |
| **Connector Type** | 否 | 标签（如 `hermes-plugin`、`standalone`），仅用于界面展示 |

**操作步骤**：

1. 管理员登录管理面板 -> Bots 页面
2. 点击 "Create Bot"
3. 填写 Name、Display Name、Model、System Prompt
4. 选择 Connection Mode：
   - **HTTP 模式**：填写 API URL 和 API Key（当前行为，不变）
   - **Connector 模式**：无需填写 API URL/Key（由连接器自行配置）
5. 提交创建
6. 若选择 Connector 模式，创建成功后**弹窗展示 Connector Key**（仅此一次，关闭后不可再查看）
7. Bot 创建后默认 `is_active = true`，自动生成对应的用户记录（`is_bot = 1`）

### 2.2 Bot 加入频道

与现有流程完全相同：频道 Owner 在频道设置中将 Bot 加为成员。

### 2.3 触发、回复与离线处理

```
用户 @mention Bot
  │
  ├── Bot 在线（有活跃 WS 连接）
  │     ├── 先插入占位消息："收到，{name} 正在处理..."
  │     ├── 通过 WS 发送 BotMention 事件
  │     ├── 等待 Connector 回复 BotReply
  │     └── 插入实际回复消息
  │
  ├── Bot 离线（无 WS 连接），connection_mode = 'connector'
  │     ├── 插入提示消息："{name} 当前离线，请在 Hermes 中检查连接状态"
  │     └── 不做其他操作
  │
  └── Bot 离线（无 WS 连接），connection_mode = 'http'
        ├── 保持现有行为：直接 POST 到 api_url
        └── POST 失败时插入错误消息："{name} 暂时不可用"
```

### 2.4 Bot 管理界面

新增列：

| 列 | 说明 |
|---|---|
| Name / Display Name | 同现有 |
| Connection Mode | HTTP / Connector |
| Status | Active / Inactive；Connector 模式额外显示**在线/离线** |
| Actions | Edit / Test / Toggle / Delete / Regenerate Key |

---

## 三、Bot 配置与连接密钥

### 3.1 数据库变更

新增迁移 `012_add_bot_connection.up.sql`：

```sql
ALTER TABLE bots ADD COLUMN connection_mode TEXT NOT NULL DEFAULT 'http'
    CHECK(connection_mode IN ('http', 'connector'));

ALTER TABLE bots ADD COLUMN connection_key TEXT NOT NULL DEFAULT '';

CREATE UNIQUE INDEX IF NOT EXISTS idx_bots_connection_key
    ON bots(connection_key) WHERE connection_key != '';
```

### 3.2 connection_key 生命周期

- **生成**：创建 Connector 模式 Bot 时，`Uuid::new_v4()` 生成
- **展示**：仅在创建成功的响应中返回完整 Key；列表接口返回脱敏形式 `ck_****`
- **重新生成**：Edit Bot 时可点击 "Regenerate Key" 重新生成（旧 Key 立即失效）
- **安全**：泄露影响单个 Bot，可通过 Regenerate 快速吊销

### 3.3 Connector 认证流程

```
1. Connector/Plugin 启动，读取 connection_key
2. 连接 ws://vast-host:3000/ws/bot?key={connection_key}
3. VAST 查询 bots 表，匹配 connection_key
4. 若匹配 && is_active：
   - 注册为 bot 连接
   - 自动订阅 Bot 所在的所有频道
   - 广播 Presence(online)
5. 否则返回 401
```

---

## 四、连接器方案对比

### 4.1 Hermes Agent Gateway Plugin（推荐）

**Pros**：
- 单一进程（Hermes Agent 管理 WS 连接生命周期）
- 复用 Hermes 的 **skills**、**memory**、**user model**、**personas**
- 原生流式输出（逐字渲染，而非等待全部完成后一次性返回）
- Hermes 的 tool/system prompt 配置统一管理

**Cons**：
- 必须实现 Hermes Gateway Adapter 接口（~15 个方法）
- 绑定 Hermes Agent，无法用于其他 LLM
- 依赖 Hermes Agent 内部 API（可能随版本变动）

**Hermes Gateway Adapter 接口概览**（从源码分析得出）：

```
Category          | Methods
------------------|--------------------------------------------------------
消息生命周期       | send(chat_id, content, reply_to, metadata)
                  | edit_message(chat_id, msg_id, content, finalize, metadata)
                  | delete_message(chat_id, msg_id)
流式草稿          | send_draft(chat_id, draft_id, content, metadata)
                  | supports_draft_streaming(chat_type, metadata) -> bool
消息拆分          | truncate_message(text, limit, len_fn) -> list[str]
                  | message_len_fn_for_chat(chat_id) -> Callable
                  | max_message_length_for_chat(chat_id) -> int
配置标志          | REQUIRES_EDIT_FINALIZE: bool
                  | RESEND_FINAL_ON_EMPTY_STREAM_FALLBACK: bool
                  | prefers_fresh_final_streaming(text, metadata) -> bool
```

**插件入口示例**：

```python
# vast_platform.py — 放置于 hermes-agent/gateway/platforms/vast/

from gateway.platform_registry import PlatformEntry, platform_registry
from gateway.config import PlatformConfig

class VASTAdapter:
    """将 VAST WebSocket 事件桥接到 Hermes Gateway 流"""
    
    def __init__(self, config: PlatformConfig):
        self.ws_url = f"{config.ws_url}?key={config.connection_key}"
        self._ws = None
        self._pending_mentions = {}  # mention_id -> (chat_id, reply_to)
        self._stream_callbacks = {}  # chat_id -> callback
    
    async def connect(self):
        """建立到 VAST 的 WebSocket 连接"""
        ...
    
    # ── 实现 Hermes Gateway 要求的方法 ──
    
    async def send(self, chat_id, content, reply_to=None, metadata=None):
        """Gateway 调用此方法发送消息到 VAST"""
        await self._ws.send(json.dumps({
            "type": "bot_reply",
            "mention_id": self._pending_mentions.get(chat_id, {}).get("mention_id"),
            "channel_id": chat_id,
            "text": content,
        }))
    
    async def edit_message(self, chat_id, message_id, content, finalize=False, metadata=None):
        """VAST 不支持编辑消息，降级为直接发送"""
        if finalize:
            await self.send(chat_id, content)
    
    # ... 其余方法提供默认实现或 no-op

def register():
    platform_registry.register(PlatformEntry(
        name="vast",
        label="VAST IM",
        adapter_factory=lambda cfg: VASTAdapter(cfg),
        check_fn=lambda: True,  # websockets 是 Python 标准行为
        required_env=["VAST_CONNECTION_KEY", "VAST_WS_URL"],
        install_hint="pip install websockets",
        emoji="💬",
    ))
```

### 4.2 独立 Connector 脚本（降级方案）

**Pros**：
- ~200 行 Python，实现简单
- 不依赖 Hermes Agent，可用于 Ollama/vLLM/OpenAI 等任意 LLM
- 部署灵活（独立进程、Docker 容器、systemd 服务）

**Cons**：
- 额外进程
- 无 Hermes 高级特性（skills/memory/user-model）
- 批量返回（非流式）

### 4.3 选型建议

| 场景 | 推荐 |
|------|------|
| 已使用 Hermes Agent 的高级用户 | Hermes Plugin |
| 使用 Ollama / vLLM / OpenAI 的用户 | 独立 Connector |
| 快速原型 / 测试 | 独立 Connector |

---

## 五、VAST 后端实施清单

### 5.1 数据库迁移

```
db/migrations/012_add_bot_connection.up.sql    -- connection_mode / connection_key
db/migrations/012_add_bot_connection.down.sql
```

### 5.2 WebSocket 协议扩展

`src/ws/protocol.rs` 新增：

```rust
// 服务端 -> 连接器
ServerEvent::BotMention {
    mention_id: String,         // 用于关联回复的唯一 ID
    channel_id: String,
    messages: Vec<ChatMessage>, // 频道上下文（与 HTTP 模式相同格式）
    model: String,
    system_prompt: String,
}

// 连接器 -> 服务端
ClientEvent::BotReply {
    mention_id: String,
    channel_id: String,
    text: String,
}

// 心跳
ClientEvent::Ping,
ServerEvent::Pong,
```

### 5.3 Bot WS Handler

`src/ws/mod.rs` 新增 `/ws/bot` 升级处理器：

- Query 提取 `key` 参数
- 查询 bots 表验证 `connection_key`
- 升级 WebSocket，注册为 bot 连接（标记 `is_bot: true`）
- 自动订阅 Bot 所有频道
- 广播 `Presence(online)`
- 断线时广播 `Presence(offline)`

### 5.4 Mention 处理流程变更

`src/api/messages.rs` 中 `spawn_bot_mentions` / `trigger_bot_response`：

- **Connector 模式 + 在线**：先插入占位消息，通过 WS 发送 `BotMention`，等待 `BotReply`
- **Connector 模式 + 离线**：插入提示 "{name} 当前离线，请检查连接状态"
- **HTTP 模式**：保持现有行为

### 5.5 Admin API

`src/api/admin/bots.rs`：

- `CreateBotRequest` 新增 `connection_mode`
- `create_bot`：Connector 模式时生成 `connection_key`
- `BotView` 新增 `connection_mode`、`connection_key_preview`（脱敏）
- 新增 `POST /api/admin/bots/:id/regenerate-key`
- 新增在线状态查询（缓存于 `ConnectionPool` 中）

### 5.6 前端

- `AdminBotsPage.tsx`：表单新增 Connection Mode 选择 + Connector Key 展示弹窗
- `frontend/src/api/admin.ts`：类型更新

---

## 六、协议细节

### 6.1 BotMention 事件格式

```json
{
  "type": "bot_mention",
  "mention_id": "ment_abc123",
  "channel_id": "ch_xyz789",
  "messages": [
    {"role": "system", "content": "你是一个友好的助手"},
    {"role": "user", "content": "alice: 你好"},
    {"role": "user", "content": "bob: @hermes 今天天气怎么样？"}
  ],
  "model": "qwen2.5:7b",
  "system_prompt": "你是一个友好的中文技术助手"
}
```

### 6.2 BotReply 事件格式

```json
{
  "type": "bot_reply",
  "mention_id": "ment_abc123",
  "channel_id": "ch_xyz789",
  "text": "今天天气晴朗，适合出门！"
}
```

### 6.3 连接生命周期

```
Connector/Plugin              VAST
  |                              |
  |-- WS connect + key -------->|
  |                              |-- 验证 connection_key
  |<-- Presence(online) ---------|  (广播到 Bot 所在频道)
  |                              |
  |<-- BotMention --------------|  (用户 @mention 触发)
  |   (connector 调用 LLM...)    |
  |-- BotReply ---------------->|
  |                              |  (VAST 插入消息 + 广播)
  |                              |
  |<-- Ping (15s) --------------|  (心跳)
  |-- Pong -------------------->|
  |                              |
  |-- WS close / timeout ------>|
  |<-- Presence(offline) --------|  (广播)
  |                              |
  |-- WS reconnect + key ------>|  (重连)
  |<-- Presence(online) ---------|
```

### 6.4 离线处理时序

```
用户 @mention Bot，但无活跃 WS 连接
  |
  ├── VAST 检查: connection_mode == 'connector' && 无活跃 WS
  │     ├── 插入消息: "🤖 {name} 当前离线，请在运行 Hermes 的机器上检查连接状态"
  │     └── 不再做其他处理
  │
  └── VAST 检查: connection_mode == 'http'
        └── 保持现有行为 (POST api_url)
```

---

## 七、安全考量

| 场景 | 措施 |
|------|------|
| connection_key 泄露 | Regenerate Key 立即吊销 |
| 连接器冒充其他 Bot | 每个 Bot 独立 Key |
| WS 连接劫持 | 连接建立后 Key 不再传输；TLS 下全程加密 |
| 重放攻击 | `mention_id` 一次性，拒绝重复的 `BotReply` |
