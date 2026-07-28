# VAST Bot Connector 设计方案

## 一、背景与目标

### 当前问题

VAST 的 Bot 集成采用 **HTTP 反向调用** 模式：当用户在频道中 `@mention` Bot 时，VAST 通过 `reqwest` 向 Bot 配置的 `api_url` 发起 `POST /v1/chat/completions` 请求。这要求 Bot 所在的机器 **具备公网可达的 IP**，而在自托管场景下（家庭宽带、内网服务器、NAT 后），Bot 无法被 VAST 访问。

### 目标

提供一种 **连接器（Connector）** 模式：Bot 以 WebSocket 客户端身份主动出站连接 VAST，VAST 通过这条已建立的持久连接推送 `@mention` 事件并接收回复。连接器可对接 **任意 OpenAI 兼容 API**（Ollama、vLLM、LM Studio、OpenAI、Hermes Agent 等）。

### 架构概览

```
┌─────────────┐   WS /ws/bot?key=xxx      ┌─────────────────┐
│    VAST     │ ◄────────────────────────► │  Bot Connector   │
│             │   BotMention {context}     │  (Python)        │
│             │ ◄────────────────────────► │  ~200 lines      │
│             │   BotReply {text}          │                  │
└─────────────┘                            └────────┬─────────┘
                                                     │
                                                     │ HTTP POST
                                                     │ /v1/chat/completions
                                                     ▼
                                           ┌─────────────────┐
                                           │    LLM Backend   │
                                           │  Ollama / vLLM / │
                                           │  OpenAI / Hermes │
                                           └─────────────────┘
```

**关键特性**：

- Bot 仅需**出站**网络连接 —— 消除公网 IP 需求
- 连接器是**通用**的 —— 不绑定 Hermes Agent，支持任何 OpenAI 兼容 API
- WS 连接是**持久**的 —— 断线自动重连，不会丢失消息
- 当前 HTTP 模式**向后兼容** —— 已有的 HTTP Bot 继续工作

---

## 二、在 VAST 中添加 Bot

### 2.1 创建 Bot 流程

**入口**：管理控制台 -> Bots 页面 -> "Create Bot"

**创建表单字段**：

| 字段 | 必填 | 说明 |
|------|------|------|
| **Name** (`name`) | 是 | Bot 的唯一标识名，也是 `@mention` 关键词。例如 `hermes` |
| **Display Name** | 否 | Bot 的显示名称 |
| **Connection Mode** | 是 | `HTTP` 或 `Connector` |
| **API URL** (`api_url`) | HTTP 模式必填 | LLM 的 `/v1/chat/completions` 端点 |
| **API Key** (`api_key`) | 否 | API 访问密钥（服务端存储，界面不可见） |
| **Model** (`model`) | 否 | LLM 模型名称，默认 `hermes` |
| **System Prompt** | 否 | 系统提示词 |
| **Connector Key** | Connector 模式自动生成 | 连接器认证密钥，创建时一次性显示 |

**操作步骤**：

1. 管理员登录管理面板 -> Bots 页面
2. 点击 "Create Bot"
3. 填写 Name（如 `hermes`）、Display Name、Model、System Prompt
4. 选择 Connection Mode：
   - **HTTP 模式**：填写 API URL 和 API Key（当前行为，不变）
   - **Connector 模式**：无需填写 API URL/Key（由连接器自行配置）
5. 提交创建
6. 若选择 Connector 模式，创建成功后**弹窗展示 Connector Key**（仅此一次，复制后关闭即不可再查看）
7. Bot 创建后默认 `is_active = true`，自动生成对应的用户记录（`is_bot = 1`）

### 2.2 Bot 加入频道

与现有流程完全相同：频道 Owner 在频道设置中将 Bot 加为成员（`POST /api/channels/{id}/bots`）。

### 2.3 触发与回复流程

1. 用户在频道中输入 `@bot_name` 或 `@display_name`
2. VAST 检测到 mention，收集频道消息历史作为上下文
3. **Connector 模式**：通过 WebSocket 发送 `BotMention` 事件给已连接的 Connector
4. **HTTP 模式**（fallback）：直接 POST 到 `api_url`
5. 先广播占位消息 "收到，{name} 正在处理..."
6. 连接器/HTTP 返回回复后，VAST 以 Bot 身份写入频道并广播

### 2.4 Bot 列表管理界面

新增列和功能：

| 列 | 说明 |
|---|---|
| Name / Display Name | 同现有 |
| Connection Mode | HTTP 或 Connector |
| Status | Active / Inactive；Connector 模式额外显示**在线/离线** |
| API URL | HTTP 模式显示地址；Connector 模式显示 `ck_****` |
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

### 3.2 connection_mode 语义

| 值 | 含义 |
|---|---|
| `http` | 当前模式。需配置 `api_url`，VAST 通过 HTTP POST 调用 LLM API |
| `connector` | 连接器模式。无需 `api_url`/`api_key`（由连接器自行配置），VAST 通过 WebSocket 推送事件 |

### 3.3 connection_key 生命周期

- **生成**：创建 Connector 模式 Bot 时，后端使用 `Uuid::new_v4()` 生成
- **展示**：仅在创建成功的响应中返回完整 `connection_key`。后续列表接口返回脱敏形式 `ck_****`
- **重新生成**：Edit Bot 时可点击 "Regenerate Key" 重新生成（旧密钥立即失效）
- **存储安全**：当前直接存储明文。Connector Key 的安全模型不同于用户密码————泄露影响单个 Bot，且可通过 Regenerate Key 快速吊销

### 3.4 Connector 认证流程

```
1. Connector 启动，读取配置文件中的 connection_key
2. Connector 连接 ws://vast-host:3000/ws/bot?key={connection_key}
3. VAST 查询 bots 表，查找 connection_key 匹配的记录
4. 若找到且 bot.is_active = true：
   - 连接建立，注册为 bot 身份
   - 根据 bot.user_id 关联的 channel_members，自动订阅 Bot 所在频道
   - 广播 Presence(online) 事件
5. 若未找到或 bot.is_active = false：
   - 返回 401，关闭连接
```

### 3.5 连接器配置文件

LLM API 端点和密钥由**连接器自身**管理，不存储在 VAST 中：

```yaml
# bot-connector.yaml
vast:
  url: "ws://your-vast-server:3000/ws/bot"
  reconnect_interval: 5  # 断线重连间隔（秒）

bots:
  - connection_key: "550e8400-e29b-41d4-a716-446655440000"
    llm:
      url: "http://localhost:11434/v1"   # Ollama 本地地址
      model: "qwen2.5:7b"
      api_key: ""                         # Ollama 无需密钥
      timeout: 120
    system_prompt: "你是一个友好的中文技术助手"

  - connection_key: "660e8400-e29b-41d4-a716-446655440001"
    llm:
      url: "https://api.openai.com/v1"
      model: "gpt-4o-mini"
      api_key: "sk-xxxxxxxx"
      timeout: 60
```

**设计理由**：LLM 配置放在连接器端，因为：
- LLM 端点可能在本地网络（`localhost:11434`），VAST 无法访问
- API Key 不需要离开 Bot 所在的机器
- 支持多个 Bot 使用同一连接器进程

---

## 四、与 Hermes Agent 集成

### 4.1 Hermes Agent 的两种部署模式

**模式 A：CLI 模式（本地终端交互）**

```bash
# 直接运行 hermes CLI
hermes
# 或通过 Docker
docker compose up
```

此时 Hermes 是一个交互式终端程序，不暴露 HTTP API。

**模式 B：Gateway 模式（连接消息平台）**

```bash
hermes gateway
```

Gateway 模式支持连接外部消息平台（Discord/Telegram/Slack）。在此基础上，可将 VAST 视为一个新的消息平台。

### 4.2 集成方式对比

| 方式 | 优点 | 缺点 |
|------|------|------|
| **方式 1：Connector 旁路（推荐）** | 无需修改 Hermes Agent 代码；通用性强（任何 LLM） | Connector 与 Hermes Agent 独立运行 |
| **方式 2：Hermes Plugin** | 原生集成，单一进程 | 需开发 Hermes Plugin；绑死 Hermes |

### 4.3 推荐集成：Connector 旁路

```
┌──────────────────────────────────────────────────────┐
│                    用户机器                           │
│                                                      │
│  ┌──────────────┐   WS    ┌──────────────┐          │
│  │     VAST     │ ◄─────► │  Connector    │          │
│  │  (公网/内网)  │         │  (Python)     │          │
│  └──────────────┘         │               │          │
│                           │  调用 LLM API  │          │
│                           └───────┬───────┘          │
│                                   │                  │
│                    ┌──────────────┴──────────────┐   │
│                    │                             │   │
│              ┌─────┴─────┐              ┌───────┴──┐│
│              │  Ollama    │              │ Hermes   ││
│              │  (本地)    │              │ Agent    ││
│              └───────────┘              │ (CLI)    ││
│                                         └──────────┘│
└──────────────────────────────────────────────────────┘
```

**部署步骤**：

```bash
# 1. 启动 Hermes Agent（或其他 LLM 后端）
docker compose up -d   # 或 hermes / ollama serve

# 2. 编写连接器配置文件 bot-connector.yaml
#    填入 VAST 中创建的 Bot 的 connection_key 和 LLM 地址

# 3. 运行连接器
python3 bot-connector.py --config bot-connector.yaml

# 4. 作为 systemd 服务持久运行（可选）
sudo cp bot-connector.service /etc/systemd/system/
sudo systemctl enable --now bot-connector
```

### 4.4 bot-connector.py 核心逻辑

```python
import asyncio
import json
import websockets
import httpx
import yaml

async def handle_bot(config, vast_url):
    """单个 Bot 的连接循环：连接 VAST -> 监听事件 -> 调用 LLM -> 回复"""
    key = config["connection_key"]
    llm = config["llm"]
    
    while True:
        try:
            async with websockets.connect(f"{vast_url}?key={key}") as ws:
                print(f"Bot connected: {key[:8]}...")
                async for raw in ws:
                    event = json.loads(raw)
                    if event["type"] == "bot_mention":
                        # 调用 LLM API
                        async with httpx.AsyncClient(timeout=llm["timeout"]) as client:
                            resp = await client.post(
                                f"{llm['url']}/chat/completions",
                                headers={"Authorization": f"Bearer {llm['api_key']}"},
                                json={
                                    "model": llm["model"],
                                    "messages": event["messages"],
                                }
                            )
                            reply = resp.json()["choices"][0]["message"]["content"]
                        
                        # 发送回复
                        await ws.send(json.dumps({
                            "type": "bot_reply",
                            "channel_id": event["channel_id"],
                            "text": reply,
                        }))
        except Exception as e:
            print(f"Connection lost: {e}, reconnecting in 5s...")
            await asyncio.sleep(5)

async def main():
    with open("bot-connector.yaml") as f:
        config = yaml.safe_load(f)
    
    vast_url = config["vast"]["url"]
    tasks = [handle_bot(bot_cfg, vast_url) for bot_cfg in config["bots"]]
    await asyncio.gather(*tasks)

asyncio.run(main())
```

---

## 五、VAST 后端实施清单

### 5.1 数据库迁移

```
db/migrations/012_add_bot_connection.up.sql    -- 新增 connection_mode / connection_key
db/migrations/012_add_bot_connection.down.sql  -- ALTER TABLE 不可逆
```

### 5.2 WebSocket 协议扩展

`src/ws/protocol.rs` 新增事件类型：

```rust
// 服务端 -> 客户端（连接器）
ServerEvent::BotMention {
    channel_id: String,
    mention_id: String,       // 用于关联回复的 ID
    messages: Vec<ChatMessage>, // 频道上下文（与 HTTP 模式相同）
    model: String,
    system_prompt: String,
}

// 客户端（连接器） -> 服务端
ClientEvent::BotReply {
    mention_id: String,       // 对应 BotMention 的 mention_id
    channel_id: String,
    text: String,
}
```

### 5.3 Bot WS Handler

`src/ws/mod.rs` 新增 `/ws/bot` 升级处理器：

- 从 Query 提取 `key` 参数
- 查询 bots 表验证 `connection_key`
- 升级 WebSocket，注册为 bot 连接
- 自动订阅 Bot 所在的所有频道

### 5.4 mention 处理流程变更

`src/api/messages.rs` 中 `spawn_bot_mentions` / `trigger_bot_response`：

- Connector 模式：通过 WS 发送 `BotMention` 事件，不发起 HTTP 请求
- HTTP 模式：保持现有行为

### 5.5 Admin API

`src/api/admin/bots.rs`：

- `CreateBotRequest` 新增 `connection_mode` 字段
- `create_bot`：生成 `connection_key`（Connector 模式时）
- `BotView` 新增 `connection_mode`、`connection_key_preview`（脱敏）
- 新增 `POST /api/admin/bots/:id/regenerate-key`

### 5.6 前端

- `AdminBotsPage.tsx`：表单新增 Connection Mode 选择，Connector Key 展示弹窗
- `frontend/src/api/admin.ts`：类型更新

---

## 六、协议细节

### 6.1 BotMention 事件格式

```json
{
  "type": "bot_mention",
  "channel_id": "ch_abc123",
  "mention_id": "ment_xyz789",
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
  "mention_id": "ment_xyz789",
  "channel_id": "ch_abc123",
  "text": "今天天气晴朗，适合出门！"
}
```

### 6.3 连接生命周期

```
Connector                  VAST
  |                          |
  |-- WS connect ---------->|
  |<-- Presence(online) -----|  (广播到 Bot 所在频道)
  |                          |
  |<-- BotMention ----------|  (用户 @mention 触发)
  |-- BotReply ------------>|
  |                          |  (VAST 插入消息 + 广播 NewMsg)
  |                          |
  |<-- Ping ----------------|  (15s 心跳)
  |-- Pong ---------------->|
  |                          |
  |-- WS close ------------>|  (断线)
  |<-- Presence(offline) ---|  (广播到 Bot 所在频道)
  |                          |
  |-- WS reconnect -------->|  (5s 后重连)
  |<-- Presence(online) -----|
```

---

## 七、安全考量

| 场景 | 措施 |
|------|------|
| connection_key 泄露 | Regenerate Key 立即吊销旧密钥 |
| 连接器冒充其他 Bot | 每个 Bot 独立 Key，无法跨 Bot |
| WS 连接劫持 | 连接建立后 Key 不再传输；TLS 下全程加密 |
| 重放攻击 | `mention_id` 一次性，拒绝重复的 `BotReply` |
| Bot 离线时消息积压 | 不积压 —— Bot 离线时不推送，用户看到占位消息后等待 |
