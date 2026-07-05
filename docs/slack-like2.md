这是一个非常经典且极具挑战的企业级 IM 架构需求。结合您提出的 **“Rust后端 + TS前端”、“单文件极简部署”、“轻量级游标同步”、“强管控权限流”** 等核心诉求，我为您设计了一套完整、可落地的工程方案。

---

### 一、 系统架构概览

本方案采用 **“前后端分离开发，单二进制打包部署”** 的架构。
- **后端 (Rust)**：基于 `Axum` 框架，提供 RESTful API 与 WebSocket 服务。使用 `SQLite` 作为数据库（免去独立部署数据库的烦恼），使用本地文件系统存储附件。
- **前端 (TypeScript)**：基于 `React` + `Vite` + `TailwindCSS` 构建 SPA。
- **单文件部署原理**：利用 Rust 的 `rust-embed` 库，在编译期将前端构建产物（HTML/JS/CSS）直接嵌入到 Rust 二进制文件中。运行时，Rust 进程同时充当 API 服务器和静态资源服务器。

---

### 二、 核心技术栈选型

| 模块 | 技术选型 | 说明 |
| --- | --- | --- |
| **后端框架** | `Axum` + `Tokio` | 高性能异步 Web 框架，原生支持 WebSocket |
| **数据库** | `SQLite` + `SQLx` | 单文件部署的绝配，零配置，支持高并发读 |
| **实时通信** | `WebSocket` (原生) | 仅用于推送“游标事件”，不传输大体积正文 |
| **前端框架** | `React 19` + `Vite 7` | 现代化、极速构建 |
| **样式** | `Tailwind CSS 4` | 原子化 CSS，与 Vite 插件深度集成 |
| **状态管理** | `Zustand 5` | 轻量级，适合管理 WS 状态和消息游标 |
| **数据获取** | `@tanstack/react-query 5` | 服务端状态缓存与同步 |
| **图标** | `lucide-react` | 轻量、一致的图标库 |
| **代码片段** | `Monaco Editor` | 提供企业级代码高亮与编辑体验 |
| **单文件打包** | `rust-embed` | 将前端 `dist` 目录编译进 Rust 二进制 |
| **AI Bot 集成** | `reqwest` (with `json` feature) | 调用 OpenAI 兼容的 Hermes API，将 AI Agent 作为虚拟频道成员接入 |

> 注：`reqwest` 已在 `Cargo.toml` 中存在，Bot 模块为其启用了 `json` feature。

---

### 三、 核心数据模型设计 (SQLite)

为了支持极简部署，所有数据存储在单一的 `im.db` 文件中。

```sql
-- 用户表
CREATE TABLE users (
    id TEXT PRIMARY KEY, -- UUID
    username TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL DEFAULT '',   -- 显示昵称
    password_hash TEXT NOT NULL,
    avatar_url TEXT DEFAULT '',              -- 头像 URL（默认空字符串）
    token_epoch INTEGER NOT NULL DEFAULT 0,  -- Token 纪元：递增可强制下线所有旧 JWT
    is_bot BOOLEAN NOT NULL DEFAULT 0,       -- 是否为 Bot 用户（由 Bot 创建流程置 1）
    created_at INTEGER NOT NULL
);

-- 会话表（登录态记录，用于审计与踢出）
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    token_hash TEXT NOT NULL,                -- 存哈希，不存明文
    is_refresh BOOLEAN NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT 1,    -- 置 0 即可在服务端吊销会话
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

-- 邀请码表（注册时需提供有效邀请码）
CREATE TABLE invite_codes (
    code TEXT PRIMARY KEY,
    created_by_user_id TEXT,                 -- 创建者（系统码为 NULL）
    max_uses INTEGER NOT NULL DEFAULT 100,
    use_count INTEGER NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL
);

-- 管理员审计日志（追加写，管理员控制台的所有写操作都落一条）
CREATE TABLE admin_audit_logs (
    id TEXT PRIMARY KEY,
    action TEXT NOT NULL,                    -- 如 user.disable / invite_code.create
    target_type TEXT,
    target_id TEXT,
    details TEXT,                            -- JSON 上下文
    performed_at INTEGER NOT NULL
);

-- 频道表
CREATE TABLE channels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    owner_id TEXT NOT NULL,
    is_archived BOOLEAN DEFAULT FALSE, -- 存档标识
    created_at INTEGER NOT NULL,
    FOREIGN KEY(owner_id) REFERENCES users(id)
);

-- 频道成员与权限
CREATE TABLE channel_members (
    channel_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('owner', 'admin', 'member')),
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (channel_id, user_id)
);

-- 消息表 (核心：使用自增ID作为游标 Cursor)
CREATE TABLE messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT, -- 单调递增，作为同步游标
    msg_id TEXT UNIQUE NOT NULL,          -- 业务UUID
    channel_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    msg_type TEXT NOT NULL CHECK(msg_type IN ('text', 'file', 'code')),
    payload TEXT NOT NULL,                -- JSON格式，存储正文/文件路径/代码
    created_at INTEGER NOT NULL
);

-- 加入申请表 (用户主动申请 -> Owner审批)
CREATE TABLE join_requests (
    id TEXT PRIMARY KEY,
    channel_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    status TEXT DEFAULT 'pending',        -- pending, approved, rejected
    created_at INTEGER NOT NULL
);

-- 邀请表 (Owner拉人 -> 目标用户确认)
CREATE TABLE invitations (
    id TEXT PRIMARY KEY,
    channel_id TEXT NOT NULL,
    inviter_id TEXT NOT NULL,
    invitee_id TEXT NOT NULL,
    status TEXT DEFAULT 'pending',        -- pending, accepted, rejected
    created_at INTEGER NOT NULL
);

-- AI Bot 配置表（管理员创建的 Hermes Agent 实例）
CREATE TABLE bots (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,                 -- 关联 users.id（创建时同步插入 is_bot=1 的用户记录）
    name TEXT UNIQUE NOT NULL,             -- @mention 用的唯一标识
    display_name TEXT NOT NULL,
    api_url TEXT NOT NULL,                 -- Hermes API 地址（OpenAI 兼容）
    api_key TEXT NOT NULL,                 -- 调用密钥（仅服务端使用，不外泄）
    system_prompt TEXT NOT NULL DEFAULT '',
    model TEXT NOT NULL DEFAULT '',
    is_active BOOLEAN NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(user_id) REFERENCES users(id)
);
```

---

### 四、 核心机制落地方案

#### 1. 轻量级消息同步机制 (Cursor-based Sync)
**痛点**：传统 WS 推送全量消息会导致带宽浪费和 WS 阻塞。
**方案**：WS 仅作为“信令通道”，消息正文通过 REST 按需拉取。

* **WS 推送事件 (Rust -> TS)**：
  当有新消息时，后端通过 WS 推送极简的元数据（实际 serde 标签为 `type`，而非 `event`）：
  ```json
  {
    "type": "new_msg",
    "channel_id": "ch_123",
    "cursor": 1054,          // 数据库自增ID，作为游标
    "sender_id": "user_A",
    "msg_type": "file",
    "preview": "[图片] photo.jpg" // 用于UI占位渲染
  }
  ```
  当消息正文变化时（如加入申请被批准、消息被编辑等），后端推送 `msg_updated` 事件，前端按需刷新对应实体：
  ```json
  { "type": "msg_updated", "channel_id": "ch_123", "cursor": 1054 }
  ```
* **前端拉取逻辑 (TS)**：
  前端维护每个 Channel 的 `last_cursor`。收到 WS 事件后，若当前正在查看该 Channel，则调用 REST API 拉取：
  ```typescript
  // GET /api/channels/{channel_id}/messages?after_cursor=1000&limit=50
  const fetchMessages = async (channelId: string, cursor: number) => {
    const res = await fetch(`/api/channels/${channelId}/messages?after_cursor=${cursor}`);
    return res.json();
  };
  ```
* **断线重连**：WS 断开重连后，前端只需带上本地保存的 `last_cursor` 请求 REST API，即可无缝补齐断网期间的消息，**彻底解决消息丢失问题**。

#### 2. 强管控权限与审批流
* **申请加入**：用户浏览公开频道列表 -> 点击申请 -> 生成 `join_requests` 记录 -> Owner 的 WS 收到 `join_request` 通知 -> Owner 调用 `PUT /api/requests/{id}/approve`。
* **邀请加入**：Owner 在频道内 `@` 或从通讯录选人 -> 生成 `invitations` 记录 -> 目标用户 WS 收到 `invitation` 通知 -> 目标用户调用 `PUT /api/invitations/{id}/accept`。
* **转让与踢人**：Owner 专属 API，后端通过 JWT 解析 `user_id` 并与 `channels.owner_id` 比对进行鉴权。

#### 3. 消息类型与 Payload 设计
消息正文统一使用 JSON 存储在 `payload` 字段中，前端根据 `msg_type` 渲染不同组件：
* **文本**：`{"content": "Hello World", "mentions": ["user_B"]}`
* **文件**：`{"file_name": "demo.mp4", "mime": "video/mp4", "size": 10240, "url": "/api/files/uuid"}`
* **代码片段**：`{"language": "rust", "code": "fn main() {}", "filename": "main.rs"}`

#### 4. Archive (存档) 机制
* **触发**：Owner 调用 `POST /api/channels/{id}/archive`。
* **后端拦截**：在 Axum 的中间件或业务逻辑层，所有针对该 Channel 的写操作（发消息、删消息、修改成员）必须先查询 `is_archived` 字段。若为 `true`，直接返回 `403 Forbidden`。
* **前端表现**：UI 顶部出现“该频道已存档，仅供查阅”的横幅，输入框禁用。

#### 5. AI Bot 集成（Hermes Agent）
将外部 AI Agent（通过 OpenAI 兼容的 Hermes API）以“虚拟频道成员”身份接入频道，实现 @mention 触发的 AI 回复。
* **创建流程**：管理员通过 `/api/admin/bots` 创建 Bot，后端会同步插入一条 `users.is_bot=1` 的用户记录，并在 `bots` 表写入 API 地址、密钥、System Prompt 等配置。
* **加入频道**：频道 Owner 在频道设置中通过 `POST /api/channels/{id}/bots` 将 Bot 加为成员。
* **触发与上下文**：用户在频道中输入 `@bot_name` 或 `@display_name`（大小写不敏感）时，后端异步派发任务，收集该频道的完整消息历史作为上下文，调用 Bot 的 Hermes API（`POST /v1/chat/completions`），并将响应以 Bot 用户身份回写到频道。
* **链式触发**：Bot 的回复若 @mention 了其他 Bot，会递归触发它们（最大深度 3，每 Bot 每频道 10 秒冷却），实现多 Agent 协作。
* **公开接口**：`GET /api/bots` 仅返回 `id / name / display_name`，不泄露任何密钥信息。

---

### 五、 单文件极简部署方案 (核心亮点)

如何实现**一个二进制文件 = 整个 IM 服务**？

#### 1. 前端资源嵌入 (Rust 端)
使用 `rust-embed` 将前端 `dist` 目录编译进二进制。
```rust
// src/embed.rs
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/dist/"] // 指向 Vite 构建产物目录
struct Asset;

// src/main.rs 中的 Axum 路由配置
use axum::{routing::get, Router, http::StatusCode, response::IntoResponse};
use std::path::PathBuf;

async fn serve_frontend(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    
    // 尝试从嵌入的资源中获取文件
    match Asset::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(axum::http::header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => {
            // SPA 路由 Fallback：未匹配到的路径返回 index.html
            match Asset::get("index.html") {
                Some(content) => ([(axum::http::header::CONTENT_TYPE, "text/html")], content.data).into_response(),
                None => StatusCode::NOT_FOUND.into_response(),
            }
        }
    }
}

// 路由挂载
let app = Router::new()
    .nest("/api", api_routes)      // 业务 API
    .route("/ws", get(ws_handler)) // WebSocket
    .fallback(serve_frontend);     // 托管前端 SPA
```

#### 2. 数据持久化目录
程序启动时，自动在**可执行文件同级目录**创建 `data` 文件夹，用于存放 SQLite 数据库和上传的文件。
```rust
// 获取可执行文件所在目录
let exe_dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
let data_dir = exe_dir.join("data");
let db_url = format!("sqlite:{}/im.db?mode=rwc", data_dir.display());
let upload_dir = data_dir.join("uploads");
// 自动初始化目录和数据库表...
```

#### 3. 打包与运行
```bash
# 1. 构建前端
cd frontend && bun run build

# 2. 编译 Rust (开启 release 优化)
cargo build --release

# 3. 部署
# 将 target/release/im-server 拷贝到任意服务器
./im-server
```
**部署结果**：无需安装 Node.js、无需安装 Nginx、无需安装 MySQL/PostgreSQL。只有一个几十 MB 的二进制文件，运行即启动，数据自动保存在同级 `data/` 目录。支持直接打包进 Docker 或直接在裸机 Linux/Windows 上运行。

---

### 六、 工程目录结构

```text
im-enterprise/
├── backend/                 # Rust 后端
│   ├── src/
│   │   ├── main.rs          # 入口，Axum 路由，WS 管理
│   │   ├── db/              # SQLite 初始化与 Migration
│   │   ├── api/             # RESTful 接口 (channels, messages, users)
│   │   ├── ws/              # WebSocket 连接池与事件分发
│   │   ├── models/          # 数据结构与 Serde 序列化
│   │   └── embed.rs         # rust-embed 静态资源托管
│   ├── Cargo.toml
│   └── data/                # 运行时生成的目录 (im.db, uploads/)
├── frontend/                # TypeScript 前端
│   ├── src/
│   │   ├── components/      # UI 组件 (MessageList, CodeBlock, FilePreview)
│   │   ├── hooks/           # useWebSocket, useCursorSync
│   │   ├── stores/          # Zustand 状态管理
│   │   ├── api/             # Axios/Fetch 封装
│   │   └── App.tsx
│   ├── dist/                # Vite 构建产物 (被 Rust 嵌入)
│   └── package.json
└── README.md
```

> **实现差异说明**：实际实现中，后端代码直接在仓库根目录 `src/` 下（即上图的 `backend/src/` 实际路径为 `<repo-root>/src/`），前端仍位于 `frontend/`。这样 `cargo`/`make` 在仓库根目录即可工作，无需 `cd backend`。

---

### 七、 落地开发步骤建议

1. **Phase 1：基建与单文件跑通 (1周)**
   - 搭建 Rust Axum + SQLite 基础骨架。
   - 配置 `rust-embed`，实现前端 Vite 构建后，Rust 能直接托管 SPA 并处理 History API 路由。
   - 实现 JWT 登录与用户鉴权中间件。
2. **Phase 2：核心消息流与游标同步 (2周)**
   - 实现 REST 消息发送与分页拉取接口。
   - 实现 WebSocket 服务端广播逻辑。
   - 前端实现 WS 监听、游标维护、断线重连与 REST 按需拉取逻辑。
3. **Phase 3：频道管控与审批流 (1.5周)**
   - 实现 Channel 的 CRUD、Owner 权限校验。
   - 实现 `join_requests` 和 `invitations` 的申请/审批状态机。
   - 实现 Archive 存档逻辑及写拦截。
4. **Phase 4：多媒体与代码片段 (1.5周)**
   - 实现文件上传接口（限制大小，存储到本地 `data/uploads`）。
   - 前端集成 `Monaco Editor` 处理代码片段。
   - 前端实现图片预览、音视频播放器、文件下载。
5. **Phase 5：优化与打包 (0.5周)**
   - 性能调优（SQLite 开启 WAL 模式，连接池配置）。
   - 编写跨平台编译脚本（Linux x86_64, Windows, macOS）。

### 八、 方案优势总结
1. **极致运维**：单文件部署，彻底消灭“前端打包、Nginx配置、数据库安装、环境依赖”等繁琐步骤，非常适合企业内部快速分发和私有化部署。
2. **高性能与低带宽**：游标同步机制（Cursor-based）将 WS 的负载降到最低，即使万人企业群，WS 也只会推送几十字节的元数据，消息正文走 HTTP/2 多路复用拉取，体验丝滑。
3. **安全与强管控**：基于 SQLite 的事务保证审批流的一致性；Archive 机制从底层 API 拦截篡改，满足企业合规审计要求。
4. **内存安全**：Rust 保证了后端在处理高并发 WS 连接和文件 IO 时，不会出现内存泄漏和 GC 停顿。
