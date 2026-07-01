# IM Enterprise — 单文件部署企业即时通讯系统

## TL;DR

> **核心目标**：构建一个 Rust + React + SQLite 的单二进制文件 IM 系统，支持游标同步、权限管控、FTS5 搜索、消息线程、Emoji 反应、在线状态，实现 Linux x86_64 单文件零依赖部署。
>
> **交付物**：
> - `im-server` 二进制（Rust Axum + 嵌入式 React SPA + SQLite）
> - 完整 REST API + WebSocket 实时推送
> - FTS5 全文搜索、Slack 风格线程、Unicode Emoji 反应
> - 申请/邀请双流权限管控 + 频道存档
> - TDD 覆盖（bun test + cargo test）
>
> **预估规模**：XL（~40 个任务，5 个并行波次）
> **并行执行**：YES — 5 波次
> **关键路径**：T1 → T6 → T15 → T30 → T36 → T37 → F1-F4

---

## Context

### 原始需求
对 `docs/slack-like2.md` 进行全量深度分析、精确审核、联网更新依赖版本，生成可工程化落地的产品方案。

### 访谈摘要
**已确认决策**：
- **功能扩展**：FTS5 搜索、在线状态、Typing 指示、Emoji 反应、消息线程
- **测试策略**：TDD（RED → GREEN → REFACTOR）
- **代码编辑器**：Monaco Editor
- **数据库**：SQLite + WAL 模式，不讨论 PostgreSQL
- **部署**：Linux x86_64 单文件部署
- **线程**：Slack 风格内联展开，平级（一级回复）
- **反应**：仅 Unicode Emoji
- **私信**：1:1 + 群组 DM
- **注册**：自注册 + 邀请码
- **删除**：仅软删除
- **文件**：≤50MB，常用类型（图片/文档/压缩包）

### 研究结果（联网验证依赖版本）
| 依赖 | 版本 | 发布日期 | 说明 |
|------|------|----------|------|
| Rust stable | 1.96.1 | 2026-06-30 | 2024 Edition 自 1.85.0 稳定 |
| axum | 0.8.9 | 2026-04-14 | 0.9 在 main 分支开发中 |
| tokio | 1.52.3 | 2026-05-08 | full features |
| sqlx | 0.9.0 | 2026-05-21 | MSRV 1.94（⚠️ 高要求） |
| rust-embed | 8.11.0 | 2026-01-14 | compression + include-exclude |
| React | 19.2.7 | 2026-06-01 | React 20 不存在 |
| Vite | 8.1.2 | 2026-06 | Rolldown (Rust bundler) |
| TailwindCSS | 4.3.1 | 2026-06 | CSS-first 配置 |
| TypeScript | 6.0.3 | 2026-04 | — |
| React Router | 7.18.0 | 2026-06-16 | 纯 SPA 模式 |
| @tanstack/react-query | 5.101.2 | 2026-06 | REST 缓存 |
| 其他前端 | Zustand 5.0.14, dayjs 1.11.21, lucide-react 1.22.0, Monaco Editor 0.55.1 |

### Metis 审计
**已解决的 6 个阻塞问题**：线程模式、反应范围、私信、注册方式、编辑/删除、文件限制均已确认。

**架构默认决策**（已应用）：
- TLS：内置 rustls + 可选 Nginx 反向代理（保持单文件理念）
- JWT：access token 15min，refresh token 7d
- 多设备：支持多 WS 连接，前端按 cursor 去重
- 备份：内置 cron + `VACUUM INTO`（Litestream 为可选增强）
- 性能目标：50 并发 WS + 100 REST req/s，p95 < 200ms

---

## Work Objectives

### 核心目标
构建一个 Rust + React + SQLite 的单二进制文件企业即时通讯系统，支持游标同步、强权限管控、FTS5 搜索、消息线程、Emoji 反应、在线状态，实现 Linux x86_64 单文件零依赖部署。

### 具体交付物
- `im-server` 二进制（Rust，Linux x86_64）
- 嵌入式 React SPA 前端（通过 rust-embed）
- `data/im.db` SQLite 数据库（自动创建）
- `data/uploads/` 文件存储目录
- REST API 端点（auth、channels、messages、files、search、requests、invitations）
- WebSocket 实时推送端点

### 完成定义
- [ ] `./im-server` 启动后，浏览器访问 `http://localhost:3000` 显示完整前端
- [ ] `cargo test` 全部通过
- [ ] `cd frontend && bun test` 全部通过
- [ ] `curl http://localhost:3000/api/health` 返回 `{"status":"ok"}`
- [ ] 50 并发 WS 连接 + 100 REST req/s 下 p95 延迟 < 200ms

### Must Have
- 单二进制部署（no Node.js, no Nginx, no PostgreSQL）
- 游标消息同步（WS 推送事件 + REST 按需拉取）
- 完整权限流（owner/admin/member + 申请/邀请审批 + 存档拦截）
- 文件上传（本地存储，≤50MB，常用类型白名单）
- FTS5 全文消息搜索
- 在线状态 + Typing 指示器
- Emoji 反应（Unicode）
- Slack 风格平级消息线程
- 1:1 + 群组私信
- TDD 测试覆盖

### Must NOT Have
- 视频/音频通话、屏幕共享、WebRTC
- Bot API、Webhook、Slash 命令
- SSO/OAuth/OIDC（仅 JWT + 用户名/密码）
- 自定义 Emoji/贴纸上传（仅 Unicode）
- 管理仪表盘、统计分析
- 国际化/多语言（仅英文）
- 邮件/推送通知（仅应用内）
- 移动端适配（桌面优先）
- 暗黑模式
- PostgreSQL 迁移路径
- Windows/macOS 编译目标
- AI 过度抽象（无 repository pattern、无 plugin system、无 ORM 层）

---

## Verification Strategy

> **零人工干预** — 所有验证由代理执行。不接受需要人为确认的验收标准。

### 测试决策
- **基础设施**：需从零搭建（bun test 前端 + cargo test 后端）
- **自动化测试**：TDD
- **前端框架**：bun test + @testing-library/react + vitest
- **后端框架**：cargo test + sqlx::test + tokio::test
- **TDD 流程**：每任务 = RED（失败测试） → GREEN（最小实现） → REFACTOR（清理）

### QA 策略
每个任务须包含代理执行的 QA 场景。
- **前端/UI**：Playwright（`playwright` skill）— 导航、交互、断言 DOM、截图
- **TUI/CLI**：`interactive_bash`（tmux）— 运行命令、发送按键、验证输出
- **API/后端**：Bash（curl）— 发送请求、断言状态码 + 响应字段
- **数据库**：Bash（sqlite3）— 查询验证数据完整性
- **证据**：保存至 `.sisyphus/evidence/task-{N}-{scenario-slug}.{ext}`

---

## Execution Strategy

### 并行波次

```
Wave 1 (基础设施 — 11 task, MAX PARALLEL):
├── T1: Rust 项目骨架 + Cargo.toml 依赖
├── T2: React 项目骨架 + package.json
├── T3: TailwindCSS v4 + Vite 配置
├── T4: 前端路由骨架 + Zustand stores
├── T5: SQLite Schema + Migration + FTS5
├── T6: Axum 路由骨架 + WebSocket 基础设施
├── T7: JWT 认证 + Argon2 密码哈希
├── T8: 错误处理框架 + 统一响应格式
├── T9: 日志/追踪基础设施 (tracing)
├── T10: rust-embed SPA 托管 + History fallback
└── T11: 优雅关闭 + 健康检查端点

Wave 2 (核心消息 + 频道 — 8 task, MAX PARALLEL):
├── T12: 用户注册/登录 API (depends: 5, 7, 8)
├── T13: Channel CRUD API (depends: 5, 8)
├── T14: 消息发送 + cursor 游标拉取 API (depends: 5, 8)
├── T15: WS 连接池管理 + 事件推送 (depends: 6, 8, 11)
├── T16: 权限中间件 + 频道成员管理 (depends: 5, 7, 8)
├── T17: Channel 存档/取消存档 (depends: 5, 8, 16)
├── T18: 登录/注册 UI 页面 (depends: 2, 3, 4)
└── T19: Channel 列表 + 消息 UI 基础 (depends: 2, 3, 4)

Wave 3 (权限流 + 私信 + 增强 — 8 task, MAX PARALLEL):
├── T20: 加入申请 + 邀请审批 API (depends: 15, 16)
├── T21: DM 私信支持 (depends: 14, 15, 16)
├── T22: 文件上传 + MIME 验证 + 大小限制 (depends: 5, 8)
├── T23: 在线状态 WebSocket 推送 (depends: 15)
├── T24: Typing 指示器 WS 推送 (depends: 15)
├── T25: Emoji 反应 API + WS 同步 (depends: 5, 15)
├── T26: FTS5 搜索 API (depends: 5, 16)
└── T27: 消息软删除 API + WS 通知 (depends: 14, 15)

Wave 4 (线程 + 前端增强 + 部署 — 6 task, MAX PARALLEL):
├── T28: 消息线程 API (thread_parent_id) (depends: 14, 15)
├── T29: Monaco Editor 集成 + 代码片段消息 (depends: 2, 19)
├── T30: WS Hook + Cursor Sync 前端逻辑 (depends: 15, 19)
├── T31: 前端: 权限流 UI + 申请/邀请提醒 (depends: 18, 19, 20)
├── T32: 前端: Emoji 反应 + 打字指示器 UI (depends: 19, 24, 25)
└── T33: 反向代理配置 + systemd unit (depends: 10)

Wave 5 (集成 + 优化 + TLS — 5 task):
├── T34: TLS/HTTPS 支持 (rustls 自签名 + Let's Encrypt)
├── T35: SQLite PRAGMA 调优 + 性能验证 (depends: 5)
├── T36: 前端路由守卫 + 整体交互完善 (depends: 18, 19, 30, 31, 32)
├── T37: 全量集成测试 (depends: ALL)
└── T38: 构建脚本 + release 编译 (depends: ALL)

Wave FINAL (4 parallel review agents → user okay):
├── F1: Plan Compliance Audit (oracle)
├── F2: Code Quality Review (unspecified-high)
├── F3: Real Manual QA (unspecified-high + playwright)
└── F4: Scope Fidelity Check (deep)
```

### 依赖矩阵

- **T1**: — → T2, T5, T6, T7, T8, T9, T10, T11（无阻塞）
- **T2**: T1 → T3, T4, T18, T29（React 骨架）
- **T3**: T2 → T18, T19（Vite + TailwindCSS）
- **T4**: T2 → T18, T19（路由 + store）
- **T5**: T1 → T12, T13, T14, T16, T17, T22, T25, T26, T35（DB schema）
- **T6**: T1 → T15, T37（Axum + WS 骨架）
- **T7**: T1 → T12, T16, T18（JWT auth）
- **T8**: T1 → T12, T13, T14, T15, T16, T17, T20, T22, T25, T26, T27（错误框架）
- **T9**: T1 → —（独立日志基础设施）
- **T10**: T1 → T33（rust-embed）
- **T11**: T1 → T15, T37（健康检查 + 优雅关闭）
- **T12**: T5, T7, T8 → T18（用户注册/登录 API）
- **T13**: T5, T8 → T19（Channel CRUD）
- **T14**: T5, T8 → T21, T27, T28（消息 API）
- **T15**: T6, T8, T11 → T20, T21, T23, T24, T25, T27, T28, T30（WS 池）
- **T16**: T5, T7, T8 → T17, T20, T21, T26（权限中间件）
- **T17**: T5, T8, T16 → —（存档逻辑）
- **T18**: T2, T3, T4, T7, T12 → T31, T36（登录 UI）
- **T19**: T2, T3, T4, T13 → T29, T30, T31, T32, T36（消息 UI）
- **T20**: T15, T16 → T31（申请/邀请 API）
- **T21**: T14, T15, T16 → —（DM 支持）
- **T22**: T5, T8 → —（文件上传）
- **T23**: T15 → —（在线状态）
- **T24**: T15 → T32（Typing 指示器）
- **T25**: T5, T15 → T32（Emoji 反应）
- **T26**: T5, T16 → —（FTS5 搜索）
- **T27**: T14, T15 → —（软删除）
- **T28**: T14, T15 → —（消息线程）
- **T29**: T2, T19 → —（Monaco Editor）
- **T30**: T15, T19 → T36（WS + Cursor Sync 前端）
- **T31**: T18, T19, T20 → T36（权限 UI）
- **T32**: T19, T24, T25 → T36（反应 + 打字 UI）
- **T33**: T10 → —（代理配置）
- **T34**: — → T37（TLS）
- **T35**: T5 → T37（PRAGMA 调优）
- **T36**: T18, T19, T30, T31, T32 → T37（路由守卫）
- **T37**: ALL → T38, F1-F4（集成测试）
- **T38**: ALL → F1-F4（构建脚本）

### Agent 调度摘要

- **Wave 1**: 11 — T1-T8 → `quick`, T9 → `quick`, T10 → `quick`, T11 → `quick`
- **Wave 2**: 8 — T12-T19 → `quick`/`unspecified-low`
- **Wave 3**: 8 — T20-T27 → `quick`/`unspecified-low`
- **Wave 4**: 6 — T28-T30 → `unspecified-low`, T31-T32 → `visual-engineering`, T33 → `quick`
- **Wave 5**: 5 — T34 → `unspecified-high`, T35 → `quick`, T36 → `visual-engineering`, T37 → `unspecified-high`, T38 → `quick`
- **FINAL**: 4 — F1 → `oracle`, F2 → `unspecified-high`, F3 → `unspecified-high`, F4 → `deep`

---

## TODOs

- [x] 1. Rust 项目骨架 + Cargo.toml 依赖

  **What to do**:
  - `cargo init` 在项目根目录创建 Rust 项目，命名 `im-server`
  - 写入 `Cargo.toml`，包含所有依赖及精确版本号
  - 设置 `edition = "2024"`, `rust-version = "1.96"`
  - 依赖列表（精确版本）：axum 0.8.9 (features: ws), tokio 1.52.3 (features: full), sqlx 0.9.0 (features: runtime-tokio, sqlite, chrono, uuid, migrate), rust-embed 8.11.0 (features: compression, include-exclude), serde 1.0.228 (features: derive), serde_json 1.0.150, jsonwebtoken 10.4.0 (features: aws_lc_rs), argon2 0.5.3, tower 0.5.3, tower-http 0.7.0 (features: cors, limit, trace, timeout), tracing 0.1.44, tracing-subscriber 0.3.23 (features: env-filter, json, fmt), uuid 1.23.4 (features: v4, v7, serde), chrono 0.4.45 (features: serde), dotenvy 0.15.7, mime_guess 2.0.5
  - 创建目录结构：`src/main.rs`, `src/db/`, `src/api/`, `src/ws/`, `src/models/`, `src/embed.rs`, `src/auth/`, `src/error.rs`
  - `src/main.rs`：最小可运行的 Axum 程序（Hello World 级别，确保 `cargo run` 编译通过）
  - 创建 `db/migrations/` 空目录（后续 T5 填充）

  **Must NOT do**:
  - 不要使用 cargo workspace（单 crate 项目）
  - 不要添加 PostgreSQL/Turso/MySQL 驱动依赖
  - 不要引入 ORM（Diesel/SeaORM）

  **Recommended Agent Profile**:
  - **Category**: `quick`（Rust 模板化项目搭建，无复杂逻辑）
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1（与 T2-T11 并行）
  - **Blocks**: T2, T5, T6, T7, T8, T9, T10, T11
  - **Blocked By**: None

  **References**:
  - `docs/slack-like2.md:145-184` — rust-embed 与 Axum 路由参考模式
  - `docs/slack-like2.md:215-237` — 目录结构参考
  - 研究结果 Rust 依赖表（本会话中 Librarian 返回值）：精确版本号

  **Acceptance Criteria**:
  - [ ] `cargo build` 成功编译（无错误）
  - [ ] `cargo run` 启动并监听端口（默认 3000）
  - [ ] `Cargo.toml` 中所有依赖版本精确固定（no `*`, no `^`）

  **QA Scenarios**:
  ```
  Scenario: 项目骨架编译通过
    Tool: Bash
    Steps:
      1. cd /home/alex/Projects/vast && cargo build 2>&1
    Expected Result: 退出码 0，无 error 输出
    Evidence: .sisyphus/evidence/task-1-build.shell

  Scenario: Cargo.toml 依赖完整性检查
    Tool: Bash
    Steps:
      1. cargo metadata --format-version=1 2>&1 | jq '.packages[].name' | sort
    Expected Result: 包含 axum, tokio, sqlx, rust-embed, serde, jsonwebtoken, argon2, tower, tower-http, tracing, tracing-subscriber, uuid, chrono, dotenvy, mime_guess
    Evidence: .sisyphus/evidence/task-1-deps.json
  ```

  **Commit**: YES
  - Message: `chore(init): scaffold Rust project with Cargo.toml dependencies`
  - Files: `Cargo.toml`, `src/main.rs`, `src/db/`, `src/api/`, `src/ws/`, `src/models/`, `src/embed.rs`, `src/auth/`, `src/error.rs`

- [x] 2. React 项目骨架 + package.json

  **What to do**:
  - 在 `frontend/` 目录下使用 Vite 创建 React + TypeScript 项目
  - `package.json` 依赖（精确版本）：react 19.2.7, react-dom 19.2.7, react-router 7.18.0, @tanstack/react-query 5.101.2, zustand 5.0.14, lucide-react 1.22.0, dayjs 1.11.21, monaco-editor 0.55.1, @uiw/react-monacoeditor 或 @monaco-editor/react
  - devDependencies：typescript 6.0.3, @types/react, @types/react-dom, vite 8.1.2, @vitejs/plugin-react 6.x, @tailwindcss/vite, tailwindcss 4.3.1, @testing-library/react, @testing-library/jest-dom, vitest, jsdom, playwright
  - 创建目录结构：`src/components/`, `src/hooks/`, `src/stores/`, `src/api/`, `src/pages/`, `src/types/`
  - 配置 `vite.config.ts`：API 代理（`/api` → `http://localhost:3000`，`/ws` → `ws://localhost:3000`），build 输出 `dist/`
  - 配置 `tsconfig.json`：strict mode, path aliases (`@/` → `src/`)
  - `src/main.tsx`：最小 React 18 渲染入口
  - `src/App.tsx`：React Router 骨架（BrowserRouter）

  **Must NOT do**:
  - 不要使用 Next.js 或 Remix（纯 SPA）
  - 不要配置 SSR/SSG/Server Components（不适用）
  - 不要使用 yarn/pnpm（统一使用 bun）

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES（与 T1 并行，但 T1 完成后才能安装依赖）
  - **Parallel Group**: Wave 1
  - **Blocks**: T3, T4, T18, T29
  - **Blocked By**: T1（项目根目录结构确定后）

  **References**:
  - 研究结果前端依赖表（本会话 Librarian 返回值）：精确版本号
  - Vite 8 文档：Rolldown 替代 esbuild+Rollup，`build.rolldownOptions`
  - React Router 7 文档：SPA 模式，`createBrowserRouter`

  **Acceptance Criteria**:
  - [ ] `cd frontend && bun install` 成功（无错误）
  - [ ] `cd frontend && bun run dev` 启动 Vite dev server（端口 5173）
  - [ ] 浏览器访问 `http://localhost:5173` 显示 React 渲染内容

  **QA Scenarios**:
  ```
  Scenario: 前端骨架启动
    Tool: Bash
    Preconditions: Node.js/bun 已安装
    Steps:
      1. cd /home/alex/Projects/vast/frontend && bun install 2>&1
      2. timeout 10 bun run dev 2>&1 || true
    Expected Result: 步骤1退出码 0，步骤2显示 "Local: http://localhost:5173/"
    Evidence: .sisyphus/evidence/task-2-devserver.shell
  ```

  **Commit**: YES
  - Message: `chore(init): scaffold React + Vite + TypeScript frontend`
  - Files: `frontend/package.json`, `frontend/vite.config.ts`, `frontend/tsconfig.json`, `frontend/src/`

- [x] 3. TailwindCSS v4 + Vite 配置完善

  **What to do**:
  - 配置 TailwindCSS v4 CSS-first 方式（`@import "tailwindcss"` 在 `src/index.css`）
  - Vite 插件配置：`tailwindcss()` + `react()`
  - 配置 `build.rolldownOptions`：manualChunks 分离 monaco-editor
  - 添加 Vite proxy 配置：REST API (`/api`) + WebSocket (`/ws`) 指向 Rust 后端
  - 验证热重载（HMR）正常工作

  **Must NOT do**:
  - 不要创建 `tailwind.config.js` 或 `postcss.config.js`（v4 不需要）
  - 不要安装 autoprefixer/postcss-import（v4 内置）

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1（与 T1, T2, T4 并行，但需 T2 提供 base）
  - **Blocks**: T18, T19
  - **Blocked By**: T2

  **References**:
  - TailwindCSS v4 文档：CSS-first config, `@tailwindcss/vite` plugin
  - Vite 8 文档：`server.proxy`, `build.rolldownOptions`

  **Acceptance Criteria**:
  - [ ] `cd frontend && bun run dev` 无 CSS 相关错误
  - [ ] `import "tailwindcss"` 在 CSS 入口中生效
  - [ ] Tailwind utility classes 在组件中可用（如 `className="text-red-500"` 渲染红色文字）

  **QA Scenarios**:
  ```
  Scenario: TailwindCSS utility classes 生效
    Tool: Playwright
    Preconditions: frontend dev server 运行中
    Steps:
      1. 访问 http://localhost:5173
      2. 检查页面中是否存在 Tailwind-generated CSS（通过 devtools 检查 computed styles）
    Expected Result: 页面使用 TailwindCSS 样式，无 unstyled 回退
    Evidence: .sisyphus/evidence/task-3-tw.png
  ```

  **Commit**: YES
  - Message: `chore(frontend): configure TailwindCSS v4 + Vite proxy`
  - Files: `frontend/vite.config.ts`, `frontend/src/index.css`, `frontend/src/main.tsx`

- [x] 4. 前端路由骨架 + Zustand stores

  **What to do**:
  - 使用 React Router 7 `createBrowserRouter` 定义路由：`/login`, `/register`, `/channels`, `/channels/:channelId`, `/channels/:channelId/thread/:messageId`, `/dm/:userId`, `/search`
  - 使用 Zustand 创建核心 stores：
    - `authStore`：token, user, login(), logout(), register()
    - `channelStore`：channels[], currentChannel, setCurrentChannel()
    - `messageStore`：messages by channel, lastCursor by channel, addMessage(), setMessages()
    - `presenceStore`：onlineUsers[], typingUsers[]
    - `reactionStore`：reactions by message, addReaction(), removeReaction()
  - 创建 API 客户端模块（`src/api/client.ts`）：带 JWT 自动注入的 fetch 封装
  - 创建类型定义文件 `src/types/index.ts`：User, Channel, Message, Reaction, etc.

  **Must NOT do**:
  - 不要使用 Redux（已选 Zustand）
  - 不要在 store 中直接做 API 调用（通过 hooks 或 TanStack Query）

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1（与 T1-T3 并行，需 T2）
  - **Blocks**: T18, T19
  - **Blocked By**: T2

  **References**:
  - React Router 7 docs：`createBrowserRouter`
  - Zustand docs：slice pattern
  - `docs/slack-like2.md:97-121` — cursor sync 概念（影响 messageStore 设计）

  **Acceptance Criteria**:
  - [ ] 所有路由 `bun run dev` 下可访问（匹配时渲染对应组件，即使组件为空）
  - [ ] `authStore` 的 token 可通过 `useAuthStore()` hook 读写
  - [ ] `messageStore` 包含 `lastCursor` per channel

  **QA Scenarios**:
  ```
  Scenario: 路由导航正常工作
    Tool: Playwright
    Steps:
      1. 访问 http://localhost:5173/login
      2. 导航至 /channels（即使空白组件也应有路由匹配）
      3. 检查 URL 变化（不出现 404）
    Expected Result: URL 变化正常，无白屏崩溃
    Evidence: .sisyphus/evidence/task-4-routing.png
  ```

  **Commit**: YES
  - Message: `feat(frontend): add React Router skeleton + Zustand stores + type definitions`
  - Files: `frontend/src/App.tsx`, `frontend/src/stores/`, `frontend/src/api/`, `frontend/src/types/`

- [x] 5. SQLite Schema + Migration + FTS5

  **What to do**:
  - 创建 `db/migrations/001_initial_schema.up.sql` 包含完整 DDL：
    - `users`：id TEXT PK, username UNIQUE, display_name, password_hash, avatar_url, created_at
    - `sessions`：id TEXT PK, user_id FK, token_hash, created_at, expires_at
    - `invite_codes`：code TEXT PK, created_by_user_id FK, max_uses, use_count, is_active, created_at
    - `channels`：id TEXT PK, name, description, owner_id FK, is_direct BOOLEAN, is_group_dm BOOLEAN, is_archived BOOLEAN, created_at
    - `channel_members`：(channel_id, user_id) PK, role TEXT CHECK, joined_at
    - `messages`：id INTEGER PK AUTOINCREMENT, msg_id TEXT UNIQUE, channel_id FK, sender_id FK, msg_type TEXT CHECK, payload TEXT, thread_parent_id INTEGER FK(self), deleted_at INTEGER, edited_at INTEGER, created_at INTEGER
    - `reactions`：(message_id, user_id, emoji) PK, created_at
    - `join_requests`：id TEXT PK, channel_id FK, user_id FK, status TEXT CHECK, created_at
    - `invitations`：id TEXT PK, channel_id FK, inviter_id FK, invitee_id FK, status TEXT CHECK, created_at
    - `read_receipts`：(user_id, channel_id) PK, last_read_message_id, updated_at
  - 创建 `messages_fts` FTS5 虚拟表（external content, 'porter unicode61' tokenizer）
  - 创建 FTS5 同步触发器（INSERT/UPDATE/DELETE on messages）
  - 创建关键索引：`idx_messages_channel_time`, `idx_messages_thread`, `idx_channel_members_user`, `idx_sessions_user`
  - 创建对应的 `down.sql` 迁移文件
  - 在 `src/db/mod.rs` 中实现数据库初始化：`sqlx::migrate!()` 自动运行，设置 PRAGMA（WAL, foreign_keys, synchronous=NORMAL, cache_size, mmap_size, busy_timeout）

  **Must NOT do**:
  - 不要使用 Refinery 或其他迁移工具（仅 sqlx::migrate!）
  - 不要在应用代码中执行 `CREATE TABLE IF NOT EXISTS`（通过 migration 管理）

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1
  - **Blocks**: T12, T13, T14, T16, T17, T22, T25, T26, T35
  - **Blocked By**: T1

  **References**:
  - `docs/slack-like2.md:32-91` — 原始 schema（用户、频道、消息、申请、邀请表）
  - SQLite FTS5 文档：external content 模式
  - SQLx migrate 文档：`sqlx::migrate!("db/migrations")` 宏
  - 研究结果 SQLite PRAGMA 配置：`journal_mode=WAL, synchronous=NORMAL, cache_size=-64000, mmap_size=268435456, busy_timeout=5000`

  **Acceptance Criteria**:
  - [ ] `cargo run` 首次启动自动创建 `data/im.db` + 运行所有 migration
  - [ ] `sqlite3 data/im.db ".tables"` 列出所有 11 张表 + messages_fts vtable
  - [ ] `sqlite3 data/im.db "PRAGMA journal_mode"` 返回 `wal`
  - [ ] `sqlite3 data/im.db "PRAGMA foreign_keys"` 返回 `1`

  **QA Scenarios**:
  ```
  Scenario: 数据库自动创建并运行 migration
    Tool: Bash
    Steps:
      1. rm -f data/im.db data/im.db-wal data/im.db-shm
      2. cargo run &
      3. sleep 3
      4. sqlite3 data/im.db ".tables"
      5. kill %1
    Expected Result: 步骤4输出包含 users, channels, channel_members, messages, reactions, sessions, invite_codes, join_requests, invitations, read_receipts, messages_fts
    Evidence: .sisyphus/evidence/task-5-tables.txt

  Scenario: WAL 模式和 FK 强制
    Tool: Bash
    Steps:
      1. sqlite3 data/im.db "PRAGMA journal_mode; PRAGMA foreign_keys;"
    Expected Result: wal / 1
    Evidence: .sisyphus/evidence/task-5-pragma.txt
  ```

  **Commit**: YES
  - Message: `feat(db): add initial schema + FTS5 + migration setup`
  - Files: `db/migrations/`, `src/db/mod.rs`

- [x] 6. Axum 路由骨架 + WebSocket 基础设施

  **What to do**:
  - `src/main.rs` 中：配置 Axum Router，挂载 API 路由组 (`/api`), WebSocket 路由 (`/ws`), SPA fallback
  - `src/ws/mod.rs`：WebSocket 连接池模型
    - `ConnectionPool` 结构体：`DashMap<UserId, DashSet<ConnectionId>>` 用户连接映射，`DashMap<ChannelId, broadcast::Sender<WsEvent>>` 频道广播器，`DashMap<ConnectionId, ConnectionState>` 连接元数据
    - `handle_socket()` 函数：socket 分裂 (sender/receiver)、心跳 ping-pong (15s interval, 30s timeout)、tokio::select! 竞态
    - `broadcast_to_channel()` 函数：向频道所有成员推送事件
    - `cleanup_connection()` 函数：移除连接、清理空频道广播器
  - `src/ws/protocol.rs`：WebSocket 事件类型定义
    - ServerEvent 枚举：new_msg, msg_deleted, reaction_update, thread_reply, typing, presence, join_request, invitation, channel_archived, member_added, member_removed, error, pong
    - ClientEvent 枚举：ping, typing_start, typing_stop
  - 定义 Axum 统一 State 类型 `AppState`（包含 SqlitePool, ConnectionPool, Config）

  **Must NOT do**:
  - 不要在 on_upgrade 内部进行 JWT 验证（在 upgrade 前完成）
  - 不要使用全局静态变量（所有状态通过 Axum State 注入）

  **Recommended Agent Profile**:
  - **Category**: `unspecified-low`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1（与 T5, T7-T11 并行）
  - **Blocks**: T15, T37
  - **Blocked By**: T1

  **References**:
  - axum 0.8 文档：`WebSocketUpgrade`, `on_upgrade`
  - tokio-tungstenite：Ping/Pong 处理
  - 研究结果：DashMap 连接池模式、`broadcast::channel(256)` per channel、`BroadcastPolicy::DropConnection`
  - `docs/slack-like2.md:99-113` — WS 推送事件格式

  **Acceptance Criteria**:
  - [ ] `cargo build` 编译通过，包含 ws 模块
  - [ ] `/ws` 端点可接受 WebSocket 连接
  - [ ] `AppState` 结构体包含所有必要字段

  **QA Scenarios**:
  ```
  Scenario: WebSocket 连接建立和心跳
    Tool: Bash (使用 websocat)
    Preconditions: im-server 运行在 localhost:3000
    Steps:
      1. echo '{"type":"ping"}' | timeout 5 websocat ws://localhost:3000/ws
    Expected Result: 连接成功建立，收到 {"type":"pong"} 响应
    Evidence: .sisyphus/evidence/task-6-websocket.txt
  ```
  
  **Commit**: YES
  - Message: `feat(ws): add Axum router skeleton + WebSocket connection pool`
  - Files: `src/main.rs`, `src/ws/mod.rs`, `src/ws/protocol.rs`

- [x] 7. JWT 认证 + Argon2 密码哈希

  **What to do**:
  - `src/auth/mod.rs`：JWT 创建/验证/刷新逻辑
    - `create_token(user_id)` → access token (15min TTL) + refresh token (7d TTL)
    - `validate_token(token)` → Claims { user_id, exp }
    - `refresh_token(refresh_token)` → 新 access token
  - `src/auth/middleware.rs`：Axum 认证中间件
    - 从 `Authorization: Bearer <token>` 提取 JWT
    - 验证失败返回 401 `{"error":{"code":"UNAUTHORIZED","message":"Invalid or expired token"}}`
    - 验证成功将 `user_id` 注入 request extensions
  - Argon2id 配置：`m=65536, t=3, p=1`（OWASP 推荐参数）
  - `hash_password(plain)` / `verify_password(plain, hash)` 函数
  - JWT secret 从环境变量 `JWT_SECRET` 加载（开发默认值: `dev-secret-change-me`）

  **Must NOT do**:
  - 不要使用 bcrypt（已选 Argon2id）
  - 不要在代码中硬编码 JWT secret（通过 dotenvy + env）

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1
  - **Blocks**: T12, T16, T18
  - **Blocked By**: T1

  **References**:
  - jsonwebtoken 10.x 文档：`encode`, `decode`, `Validation`, `EncodingKey`, `DecodingKey`
  - argon2 0.5 文档：`Argon2::default()`, `hash_password`, `verify_password`
  - OWASP Password Storage Cheat Sheet：Argon2id 推荐参数

  **Acceptance Criteria**:
  - [ ] `cargo test` 中 auth 相关测试通过（token 创建 + 验证 + 过期检测 + 密码哈希来回）
  - [ ] `create_token("test-user")` 返回有效 JWT
  - [ ] `validate_token(valid_token)` 返回 Ok(Claims)
  - [ ] `validate_token(expired_token)` 返回 Err

  **QA Scenarios**:
  ```
  Scenario: JWT 创建和验证
    Tool: Bash (cargo test)
    Steps:
      1. cargo test auth::tests -- --nocapture 2>&1
    Expected Result: 所有测试通过，包括 token_creation, token_validation, token_expiry, password_hash_roundtrip
    Evidence: .sisyphus/evidence/task-7-auth-test.txt
  ```

  **Commit**: YES
  - Message: `feat(auth): add JWT authentication + Argon2id password hashing`
  - Files: `src/auth/mod.rs`, `src/auth/middleware.rs`

- [x] 8. 错误处理框架 + 统一响应格式

  **What to do**:
  - `src/error.rs`：定义 `AppError` 枚举（实现 `IntoResponse`）
    - 错误码：UNAUTHORIZED(401), FORBIDDEN(403), NOT_FOUND(404), CONFLICT(409), PAYLOAD_TOO_LARGE(413), UNSUPPORTED_MEDIA_TYPE(415), INTERNAL(500)
  - 统一 JSON 错误响应格式：`{"error":{"code":"STRING","message":"STRING"}}`
  - `impl From<sqlx::Error>` for `AppError`：数据库错误转换
  - `impl From<jsonwebtoken::errors::Error>` for `AppError`：JWT 错误转换
  - Axum 全局 404 fallback（返回 JSON 而非 HTML）
  - 请求体大小限制（通过 tower-http `RequestBodyLimitLayer`）

  **Must NOT do**:
  - 不要在错误响应中暴露内部堆栈信息（生产环境）
  - 不要为每个 handler 单独处理错误（统一通过 `AppError`）

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1
  - **Blocks**: T12, T13, T14, T15, T16, T17, T20, T22, T25, T26, T27
  - **Blocked By**: T1

  **References**:
  - axum docs：`IntoResponse` trait, `(StatusCode, Json<T>)` 响应模式
  - tower-http docs：`RequestBodyLimitLayer`, `CorsLayer`

  **Acceptance Criteria**:
  - [ ] `GET /api/nonexistent` 返回 `{"error":{"code":"NOT_FOUND","message":"..."}}` (HTTP 404, Content-Type: application/json)
  - [ ] 所有 `AppError` 变体序列化为统一 JSON 格式
  - [ ] SQLite 错误（如 UNIQUE constraint violation）正确转换为 CONFLICT 错误

  **QA Scenarios**:
  ```
  Scenario: API 404 返回 JSON 而非 HTML
    Tool: Bash (curl)
    Steps:
      1. curl -s -w '\n%{http_code}' http://localhost:3000/api/nonexistent
    Expected Result: 响应体为 {"error":{"code":"NOT_FOUND","message":"..."}}，HTTP 状态码 404，Content-Type 包含 application/json
    Evidence: .sisyphus/evidence/task-8-error-format.json

  Scenario: 请求体过大被拦截
    Tool: Bash
    Steps:
      1. dd if=/dev/urandom bs=1M count=11 2>/dev/null | base64 > /tmp/big.txt
      2. curl -s -w '\n%{http_code}' -X POST http://localhost:3000/api/auth/register -H 'Content-Type: application/json' -d "$(printf '{"username":"a","password":"a","invite_code":"a","big":"'; cat /tmp/big.txt; printf '"}')"
    Expected Result: HTTP 413
    Evidence: .sisyphus/evidence/task-8-payload-too-large.txt
  ```

  **Commit**: YES
  - Message: `feat(error): add unified error handling + JSON response format`
  - Files: `src/error.rs`, `src/main.rs`（middleware 注册）

- [x] 9. 日志/追踪基础设施

  **What to do**:
  - 在 `main.rs` 中初始化 `tracing-subscriber`：
    - 开发环境：`fmt` layer（人类可读输出）+ `env-filter`（`RUST_LOG` 环境变量控制）
    - 生产环境：`json` layer（结构化日志）
  - 在关键路径添加 `tracing::instrument` 宏：HTTP 请求、WS 连接/断开、消息发送、数据库查询
  - 日志级别：INFO（正常操作）、WARN（可恢复错误）、ERROR（不可恢复）
  - WS 连接日志：connect/disconnect with user_id, connection_id, total_connections
  - 添加 `tracing-log` 桥接（将 `log` crate 日志路由到 tracing）

  **Must NOT do**:
  - 不要使用 `println!` 或 `eprintln!` 进行日志输出
  - 不要在日志中输出 JWT token 或密码哈希

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1（与所有任务并行）
  - **Blocks**: None（独立基础设施）
  - **Blocked By**: T1

  **QA Scenarios**:
  ```
  Scenario: 结构化日志输出
    Tool: Bash
    Steps:
      1. RUST_LOG=info cargo run 2>&1 &
      2. sleep 2
      3. curl -s http://localhost:3000/api/health
      4. kill %1 2>/dev/null
    Expected Result: 日志输出包含 timestamp, level, target 信息，请求日志可见
    Evidence: .sisyphus/evidence/task-9-tracing.log
  ```

  **Commit**: YES
  - Message: `feat(obs): add tracing + structured logging`
  - Files: `src/main.rs`（tracing 初始化）

- [x] 10. rust-embed SPA 托管 + History fallback

  **What to do**:
  - `src/embed.rs`：使用 `rust-embed` 嵌入 `frontend/dist/` 目录
  - `serve_frontend()` handler：从嵌入资产服务文件，SPA fallback（未匹配路径返回 `index.html`）
  - 确保 `/api/*` 路由在 SPA fallback 前匹配（404 返回 JSON 而非 HTML）
  - MIME 类型正确设置（通过 `mime_guess` 从文件扩展名推断）
  - 构建脚本：确保 `frontend/dist/` 在 `cargo build` 前存在（或提供友好错误提示）
  - `.gitignore`：忽略 `data/`, `frontend/dist/`, `target/`

  **Must NOT do**:
  - 不要将 `/api/*` 请求 fallback 到 index.html
  - 不要在嵌入时包含 source maps（通过 `#[exclude = "*.map"]`）

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1（独立于其他任务）
  - **Blocks**: T33
  - **Blocked By**: T1

  **References**:
  - `docs/slack-like2.md:145-184` — rust-embed + Axum 路由配置参考代码
  - rust-embed 8.x 文档：`RustEmbed` derive macro

  **Acceptance Criteria**:
  - [ ] `cargo build` 成功嵌入 `frontend/dist/`（如果 dist 存在）
  - [ ] `cargo run` 后 `GET /` 返回 index.html
  - [ ] `GET /channels/any-uuid` 返回 index.html（SPA fallback）
  - [ ] `GET /api/nonexistent` 返回 JSON 404（不 fallback）

  **QA Scenarios**:
  ```
  Scenario: SPA fallback 正常工作
    Tool: Bash (curl)
    Preconditions: frontend/dist/ 存在（即使只有 index.html）
    Steps:
      1. curl -s -o /dev/null -w '%{http_code}' http://localhost:3000/channels/test-id
      2. curl -s -o /dev/null -w '%{http_code}' http://localhost:3000/api/nonexistent
    Expected Result: 步骤1返回 200 (index.html), 步骤2返回 404 (JSON error)
    Evidence: .sisyphus/evidence/task-10-fallback.txt
  ```

  **Commit**: YES
  - Message: `feat(embed): add rust-embed SPA hosting + history fallback`
  - Files: `src/embed.rs`, `src/main.rs`

- [x] 11. 优雅关闭 + 健康检查端点

  **What to do**:
  - `GET /api/health`：返回 `{"status":"ok","db":"connected"}` 或 `{"status":"degraded","db":"error"}`
  - 优雅关闭：
    - 捕获 SIGTERM/SIGINT（通过 tokio `signal`）
    - 收到信号后：停止接受新连接 → 等待活跃 WS 连接关闭（max 5s timeout） → 关闭数据库连接池 → 退出
    - 关闭期间所有 API 请求返回 503 Service Unavailable
  - CORS 中间件配置（开发环境允许所有来源，生产环境限制）
  - 请求超时中间件（tower-http `TimeoutLayer`, 30s）

  **Must NOT do**:
  - 不要在收到 SIGTERM 时立即强制退出（必须等待 in-flight 请求完成）

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1（与 T5-T10 并行）
  - **Blocks**: T15, T37
  - **Blocked By**: T1

  **QA Scenarios**:
  ```
  Scenario: 健康检查端点响应
    Tool: Bash (curl)
    Steps:
      1. curl -s http://localhost:3000/api/health | jq .
    Expected Result: {"status":"ok","db":"connected"}
    Evidence: .sisyphus/evidence/task-11-health.json

  Scenario: 优雅关闭
    Tool: Bash
    Steps:
      1. cargo run &
      2. sleep 2
      3. kill -TERM %1
      4. wait %1
      5. echo "exit code: $?"
    Expected Result: 退出码 0，日志包含 "shutting down" / "graceful shutdown complete"
    Evidence: .sisyphus/evidence/task-11-shutdown.log
  ```

  **Commit**: YES
  - Message: `feat(server): add graceful shutdown + health check + CORS`
  - Files: `src/main.rs`

- [x] 12. 用户注册/登录 API

  **What to do**:
  - `POST /api/auth/register`：验证 username/password 格式，验证 invite_code 有效性，创建用户 + 返回 JWT token pair
  - `POST /api/auth/login`：验证凭据 → 返回 JWT token pair + 创建 session 记录
  - `POST /api/auth/refresh`：验证 refresh token → 返回新 access token
  - `POST /api/auth/logout`：使 session 失效（软删除 sessions 记录）
  - 输入验证：username: 3-32 chars alphanumeric, password: ≥8 chars with mix, invite_code: exists + not expired
  - TDD：先写测试（注册成功、用户名重复、无效邀请码、登录成功、密码错误、token 刷新、登出）

  **Must NOT do**:
  - 不要在注册时要求邮箱验证（仅邀请码控制）
  - 不要在日志中输出密码或 token

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 2（与 T13-T19 并行）
  - **Blocks**: T18
  - **Blocked By**: T5, T7, T8

  **QA Scenarios**:
  ```
  Scenario: 用户注册 → 登录 → 刷新 token
    Tool: Bash (curl)
    Steps:
      1. 注册: curl -s -X POST localhost:3000/api/auth/register -H 'Content-Type: application/json' -d '{"username":"alice","password":"Alice123!","invite_code":"IM2024"}'
      2. 登录: curl -s -X POST localhost:3000/api/auth/login -H 'Content-Type: application/json' -d '{"username":"alice","password":"Alice123!"}'
      3. 提取 token 并调用 /api/auth/refresh
    Expected Result: 步骤1返回 201 + token pair, 步骤2返回 200 + token pair, 步骤3返回 200 + new access token
    Evidence: .sisyphus/evidence/task-12-auth-flow.json

  Scenario: 重复用户名被拒绝
    Tool: Bash (curl)
    Steps:
      1. curl -s -w '\n%{http_code}' -X POST localhost:3000/api/auth/register -H 'Content-Type: application/json' -d '{"username":"alice","password":"Alice123!","invite_code":"IM2024"}'
    Expected Result: HTTP 409, {"error":{"code":"CONFLICT","message":"Username already exists"}}
    Evidence: .sisyphus/evidence/task-12-duplicate.txt
  ```

  **Commit**: YES
  - Message: `feat(auth): add register/login/refresh/logout API endpoints`
  - Files: `src/api/auth.rs`, tests

- [x] 13. Channel CRUD API

  **What to do**:
  - `POST /api/channels`：创建频道（自动将创建者设为 owner + member）
  - `GET /api/channels`：列出用户所在频道（含 member 角色信息）
  - `GET /api/channels/:id`：频道详情
  - `PATCH /api/channels/:id`：仅 owner 可修改（name, description）
  - 输入验证：name: 1-80 chars, description: max 500 chars
  - TDD：先写测试（创建、列表、非成员不可见、修改权限检查）

  **Must NOT do**:
  - 不要让非成员看到频道详情（权限控制）

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 2
  - **Blocks**: T19
  - **Blocked By**: T5, T8

  **QA Scenarios**:
  ```
  Scenario: 频道 CRUD 完整流程
    Tool: Bash (curl)
    Steps:
      1. 用户A创建频道 → 201 + channel_id
      2. 用户A列出频道 → 200 + channels[] 包含刚创建的频道
      3. 用户B列出频道 → 200 + channels[] 不包含（非成员）
    Expected Result: 所有步骤按预期返回
    Evidence: .sisyphus/evidence/task-13-channel-crud.json
  ```

  **Commit**: YES
  - Message: `feat(channels): add channel CRUD API`
  - Files: `src/api/channels.rs`, `src/models/channel.rs`

- [x] 14. 消息发送 + cursor 游标拉取 API

  **What to do**:
  - `POST /api/channels/:channel_id/messages`：发送消息（验证发送者为频道成员，频道未 archive）
  - `GET /api/channels/:channel_id/messages?after_cursor=&limit=50`：游标分页拉取
  - `GET /api/channels/:channel_id/messages?around_cursor=&limit=20`：围绕某条消息的上下文拉取（线程用）
  - 消息载荷验证：msg_type 必须为 text/file/code，payload JSON 格式正确
  - 返回格式：`{messages: [...], next_cursor: 1054, has_more: true}`
  - `after_cursor=0` 返回最新 N 条消息
  - TDD：发送消息、游标分页、空频道、非成员被拒、archive 频道被拒

  **Must NOT do**:
  - 不要在 WS handler 中直接操作数据库（WS 仅推送事件）
  - 不要返回超过 limit 条消息

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 2
  - **Blocks**: T21, T27, T28
  - **Blocked By**: T5, T8

  **QA Scenarios**:
  ```
  Scenario: 游标消息分页
    Tool: Bash (curl)
    Preconditions: 频道中有 60 条消息
    Steps:
      1. GET /api/channels/:id/messages?after_cursor=0&limit=20
      2. 提取 next_cursor，再次请求 after_cursor={next_cursor}&limit=20
      3. 验证两次返回无重复消息
    Expected Result: 每次返回 ≤20 条消息，无重复，cursor 单调递增
    Evidence: .sisyphus/evidence/task-14-cursor-pagination.json
  ```

  **Commit**: YES
  - Message: `feat(messages): add message send + cursor-based pagination API`
  - Files: `src/api/messages.rs`, `src/models/message.rs`

- [x] 15. WS 连接池管理 + 事件推送

  **What to do**:
  - 实现 `ConnectionPool` 完整逻辑：用户连接注册/注销、频道订阅、事件广播
  - ws_handler：JWT 从 query param 验证 → on_upgrade → handle_socket
  - `handle_socket()`：split socket → send_task (读 broadcast rx 推送到 WS) + recv_task (处理 ping/typing_start/typing_stop) + heartbeat_task (15s ping)
  - 事件推送：当消息被插入时，通过 `broadcast_to_channel()` 推送到频道所有在线用户
  - 连接清理：heartbeat 超时 60s → 关闭连接 → 从所有 map 中移除 → 广播 presence:offline
  - 在线统计：`GET /api/presence/:channel_id` 返回在线用户列表

  **Must NOT do**:
  - 不要在 WS handler 中进行重量级操作（仅推送，不查数据库）

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 2
  - **Blocks**: T20, T21, T23, T24, T25, T27, T28, T30
  - **Blocked By**: T6, T8, T11

  **QA Scenarios**:
  ```
  Scenario: 用户发送消息 → WS 推送到其他用户
    Tool: Bash (curl + websocat)
    Preconditions: 用户A(W1)和用户B(W2)在同一频道
    Steps:
      1. W2 连接 WS: websocat ws://localhost:3000/ws?token=<token_B>
      2. 用户A 通过 REST 发送消息
      3. 检查 W2 收到的 WS 帧
    Expected Result: W2 收到 {"type":"new_msg","channel_id":"...","cursor":...,"sender_id":"...","msg_type":"text","preview":"..."}
    Evidence: .sisyphus/evidence/task-15-ws-push.json

  Scenario: 连接断开清理
    Tool: Bash
    Steps:
      1. 建立 WS 连接，记录服务器连接数
      2. kill WS 客户端（不发送 close frame）
      3. 等待 60s heartbeat 超时
      4. 查询 GET /api/presence/:channel_id
    Expected Result: 超时后用户从在线列表移除
    Evidence: .sisyphus/evidence/task-15-cleanup.json
  ```

  **Commit**: YES
  - Message: `feat(ws): add connection pool management + event broadcasting`
  - Files: `src/ws/mod.rs`

- [x] 16. 权限中间件 + 频道成员管理

  **What to do**:
  - `src/auth/middleware.rs`：`RequireAuth` 中间件（提取 JWT → 验证 → 注入 user_id）
  - `src/api/channel_members.rs`：
    - `POST /api/channels/:id/members`：owner/admin 添加成员
    - `DELETE /api/channels/:id/members/:user_id`：owner 移除成员 / 成员自行离开
    - `PATCH /api/channels/:id/members/:user_id/role`：owner 变更成员角色
    - `GET /api/channels/:id/members`：列出频道成员及角色
  - 权限检查辅助函数：`is_owner()`, `is_admin()`, `is_member()`, `can_manage_members()`
  - 频道操作权限：发消息需要 member+, 管理需要 admin+, 转让/删除需要 owner

  **Must NOT do**:
  - 不要在每个 handler 中重复编写权限检查逻辑

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 2
  - **Blocks**: T17, T20, T21, T26
  - **Blocked By**: T5, T7, T8

  **QA Scenarios**:
  ```
  Scenario: 非成员被拒绝发消息
    Tool: Bash (curl)
    Steps:
      1. 用户B（非频道成员）尝试 POST /api/channels/:id/messages
    Expected Result: HTTP 403, {"error":{"code":"FORBIDDEN","message":"Not a channel member"}}
    Evidence: .sisyphus/evidence/task-16-permission.txt
  ```

  **Commit**: YES
  - Message: `feat(permissions): add auth middleware + channel member management`
  - Files: `src/auth/middleware.rs`, `src/api/channel_members.rs`

- [x] 17. Channel 存档/取消存档

  **What to do**:
  - `POST /api/channels/:id/archive`：owner 存档频道（设置 is_archived=true）
  - `POST /api/channels/:id/unarchive`：owner 取消存档
  - 存档后的写拦截：在消息发送、成员管理、频道修改 handler 中检查 `is_archived`
  - WS 推送 `channel_archived` / `channel_unarchived` 事件
  - TDD：存档后发消息被拒、存档后修改成员被拒、存档后读消息仍允许、非 owner 无法存档

  **Must NOT do**:
  - 不要在数据库中硬删除频道（仅标记）
  - 不要让非 owner 执行存档操作

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 2
  - **Blocks**: None
  - **Blocked By**: T5, T8, T16

  **QA Scenarios**:
  ```
  Scenario: 存档频道写操作被拦截
    Tool: Bash (curl)
    Steps:
      1. Owner 存档频道
      2. Member 尝试发送消息
      3. Member 尝试读取消息
    Expected Result: 步骤2返回 403, 步骤3返回 200
    Evidence: .sisyphus/evidence/task-17-archive.txt
  ```

  **Commit**: YES
  - Message: `feat(channels): add archive/unarchive with write blocking`
  - Files: `src/api/channels.rs`

- [x] 18. 登录/注册 UI 页面

  **What to do**:
  - `src/pages/LoginPage.tsx`：用户名 + 密码表单，错误提示，登录成功 → 存储 token → 跳转 `/channels`
  - `src/pages/RegisterPage.tsx`：用户名 + 密码 + 邀请码表单，注册成功 → 自动登录
  - `src/components/AuthGuard.tsx`：路由守卫，未登录 → 重定向 `/login`
  - 使用 TanStack Query 的 `useMutation` 处理注册/登录
  - TailwindCSS 样式：居中卡片布局、输入框焦点态、加载中 spinner、错误红色边框
  - TDD：表单提交、错误状态、成功跳转

  **Must NOT do**:
  - 不要在 localStorage 中存储密码
  - 不要使用内联样式（统一 TailwindCSS）

  **Recommended Agent Profile**:
  - **Category**: `visual-engineering`
  - **Skills**: `["frontend-ui-ux"]`

  **Parallelization**:
  - **Parallel Group**: Wave 2
  - **Blocks**: T31, T36
  - **Blocked By**: T2, T3, T4, T7, T12

  **QA Scenarios**:
  ```
  Scenario: 登录成功 → 跳转首页
    Tool: Playwright
    Preconditions: 测试用户已注册
    Steps:
      1. 访问 http://localhost:5173/login
      2. 输入 username: "alice", password: "Alice123!"
      3. 点击 Login 按钮
      4. 等待导航至 /channels
    Expected Result: URL 变为 /channels，页面显示频道列表（即使为空）
    Evidence: .sisyphus/evidence/task-18-login.png

  Scenario: 登录失败错误提示
    Tool: Playwright
    Steps:
      1. 访问 http://localhost:5173/login
      2. 输入错误密码
      3. 点击 Login 按钮
    Expected Result: 页面显示红色错误提示（不跳转），输入框红色边框
    Evidence: .sisyphus/evidence/task-18-login-error.png
  ```

  **Commit**: YES
  - Message: `feat(frontend): add login + register pages with auth guard`
  - Files: `frontend/src/pages/LoginPage.tsx`, `frontend/src/pages/RegisterPage.tsx`, `frontend/src/components/AuthGuard.tsx`

- [x] 19. Channel 列表 + 消息 UI 基础

  **What to do**:
  - `src/pages/ChannelListPage.tsx`：左侧频道列表 + 右侧消息区域（类似 Slack 布局）
  - `src/components/ChannelSidebar.tsx`：频道列表（含 DM），高亮当前频道，创建频道按钮
  - `src/components/MessageList.tsx`：消息列表渲染（虚拟滚动优化，至少支持 500 条消息流畅滚动）
  - `src/components/MessageInput.tsx`：消息输入框 + 发送按钮（Enter 发送，Shift+Enter 换行）
  - `src/components/MessageBubble.tsx`：单条消息渲染（头像、用户名、时间、消息体、消息类型分发）
  - `src/components/TextMessage.tsx`：纯文本消息（@mention 高亮）
  - 使用 Zustand 管理当前频道和消息状态
  - TDD：消息渲染、频道切换、输入框提交

  **Must NOT do**:
  - 不要渲染消息时做全量 re-render（使用 React.memo + useMemo）
  - 不要硬编码消息样式

  **Recommended Agent Profile**:
  - **Category**: `visual-engineering`
  - **Skills**: `["frontend-ui-ux"]`

  **Parallelization**:
  - **Parallel Group**: Wave 2
  - **Blocks**: T29, T30, T31, T32, T36
  - **Blocked By**: T2, T3, T4, T13

  **QA Scenarios**:
  ```
  Scenario: 频道列表 + 消息发送
    Tool: Playwright
    Preconditions: 用户已登录，有一个频道
    Steps:
      1. 访问 /channels
      2. 点击左侧频道列表中的频道
      3. 在消息输入框输入 "Hello world" 并回车
      4. 检查消息列表中出现 "Hello world"
    Expected Result: 消息出现在消息列表，输入框清空
    Evidence: .sisyphus/evidence/task-19-message-send.png
  ```

  **Commit**: YES
  - Message: `feat(frontend): add channel list + message UI with input`
  - Files: `frontend/src/pages/ChannelListPage.tsx`, `frontend/src/components/ChannelSidebar.tsx`, `frontend/src/components/MessageList.tsx`, `frontend/src/components/MessageInput.tsx`, `frontend/src/components/MessageBubble.tsx`

- [x] 20. 加入申请 + 邀请审批 API

  **What to do**:
  - `POST /api/channels/:id/join-request`：用户申请加入（创建 join_requests 记录 → WS 推送 owner）
  - `GET /api/requests`：owner/admin 查看待审批申请
  - `PUT /api/requests/:id/approve`：批准加入 → 添加 member 记录 → WS 推送 member_added
  - `PUT /api/requests/:id/reject`：拒绝加入
  - `POST /api/channels/:id/invitations`：owner 邀请用户 → 创建 invitations 记录 → WS 推送目标用户
  - `GET /api/invitations`：用户查看收到的邀请
  - `PUT /api/invitations/:id/accept`：接受邀请 → 添加 member 记录
  - `PUT /api/invitations/:id/reject`：拒绝邀请
  - 状态机验证：pending → approved/rejected, pending → accepted/rejected
  - TDD：申请-审批流程、邀请-接受流程、重复申请被拒、非 owner 不能审批

  **Must NOT do**:
  - 不要让已加入成员再次申请
  - 不要让已处理（非 pending）的申请再次审批

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 3
  - **Blocks**: T31
  - **Blocked By**: T15, T16

  **QA Scenarios**:
  ```
  Scenario: 完整审批流程
    Tool: Bash (curl)
    Steps:
      1. 用户B申请加入频道C
      2. Owner查看待审批列表 → 包含用户B的申请
      3. Owner批准申请
      4. 用户B列出频道 → 包含频道C
      5. 用户B发送消息到频道C → 成功
    Expected Result: 步骤2包含申请, 步骤4包含频道, 步骤5返回 201
    Evidence: .sisyphus/evidence/task-20-approval-flow.json
  ```

  **Commit**: YES
  - Message: `feat(permissions): add join request + invitation approval APIs`
  - Files: `src/api/requests.rs`, `src/api/invitations.rs`

- [x] 21. DM 私信支持

  **What to do**:
  - `POST /api/dm`：创建 DM 频道（或返回已存在的），设置 `is_direct=true`
    - 1:1 DM：参数 `user_ids: [user_A, user_B]`，查找现有 DM 频道或创建
    - 群组 DM：参数 `user_ids: [...]`, `is_group_dm=true`, `name`（可选）
  - `GET /api/dm`：列出用户的 DM 频道（区别于普通频道）
  - DM 频道无 owner 概念（is_direct 频道跳过权限检查除成员外）
  - DM 频道不可存档（is_direct 频道拒绝 archive 操作）
  - WS 事件：new_msg 照常推送（前端的 channelSidebar 需支持 DM 渲染）
  - TDD：创建 1:1 DM、复用现有 DM、创建群组 DM、DM 不可存档

  **Must NOT do**:
  - 不要让 DM 频道有 owner 角色
  - DM 频道不应出现在公共频道列表

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 3
  - **Blocks**: None
  - **Blocked By**: T14, T15, T16

  **QA Scenarios**:
  ```
  Scenario: 1:1 DM 创建与复用
    Tool: Bash (curl)
    Steps:
      1. 用户A创建与用户B的DM → 返回 channel_id
      2. 再次创建与用户B的DM → 返回相同 channel_id（复用）
      3. 用户C尝试查看A-B的DM → 403（非成员）
    Expected Result: 步骤2返回相同channel_id, 步骤3返回403
    Evidence: .sisyphus/evidence/task-21-dm.json
  ```

  **Commit**: YES
  - Message: `feat(dm): add direct message + group DM support`
  - Files: `src/api/dm.rs`

- [x] 22. 文件上传 + MIME 验证 + 大小限制

  **What to do**:
  - `POST /api/files/upload`：multipart 文件上传
    - MIME 类型白名单：image/*, application/pdf, text/plain, application/zip, application/gzip, application/json, text/csv, application/vnd.openxmlformats-officedocument.*, video/mp4, audio/mpeg
    - 大小限制：50MB（前后端双重验证）
    - 文件名安全：UUID-based 存储路径 `data/uploads/{uuid}.{ext}`，保留原始文件名用于下载
  - `GET /api/files/:file_id`：文件下载（正确 Content-Type, Content-Disposition）
  - 文件消息发送：上传后返回 file_id，在消息 payload 中引用
  - 磁盘空间检查：写入前验证可用空间
  - `data/uploads/` 自动创建
  - TDD：上传成功、过大被拒、禁止类型被拒、下载正确 MIME

  **Must NOT do**:
  - 不要以用户提供的文件名保存到磁盘（路径遍历攻击）
  - 不要在内存中缓冲整个大文件（流式写入）

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 3
  - **Blocks**: None
  - **Blocked By**: T5, T8

  **QA Scenarios**:
  ```
  Scenario: 文件上传成功并下载
    Tool: Bash (curl)
    Steps:
      1. dd if=/dev/urandom bs=1K count=10 of=/tmp/test.png
      2. curl -s -w '\n%{http_code}' -X POST localhost:3000/api/files/upload -H "Authorization: Bearer <token>" -F "file=@/tmp/test.png"
      3. curl -s -o /tmp/downloaded.png -w '\n%{http_code}' localhost:3000/api/files/<file_id> -H "Authorization: Bearer <token>"
      4. diff /tmp/test.png /tmp/downloaded.png
    Expected Result: 步骤2返回201 + file_id, 步骤3返回200, 步骤4无差异
    Evidence: .sisyphus/evidence/task-22-upload-download.txt

  Scenario: 超大文件被拒
    Tool: Bash
    Steps:
      1. dd if=/dev/urandom bs=1M count=51 of=/tmp/big.bin
      2. curl -s -w '\n%{http_code}' -X POST localhost:3000/api/files/upload -H "Authorization: Bearer <token>" -F "file=@/tmp/big.bin"
    Expected Result: HTTP 413
    Evidence: .sisyphus/evidence/task-22-size-limit.txt
  ```

  **Commit**: YES
  - Message: `feat(files): add file upload with MIME validation + size limit`
  - Files: `src/api/files.rs`

- [x] 23. 在线状态 WebSocket 推送

  **What to do**:
  - 用户 WS 连接时：广播 `presence(user_id, online)` 给用户所在所有频道
  - 用户 WS 断开时（超时 60s）：广播 `presence(user_id, offline)`
  - `GET /api/presence/:channel_id`：返回频道在线用户列表 `{online_users: ["user_id1", ...]}`
  - `GET /api/presence/users`：返回全局在线用户列表（DM 列表显示在线状态）
  - 前端 `presenceStore`：onlineUsers set，收到 WS 事件后更新
  - TDD：用户连接后列表包含、断开后移除、多频道状态一致性

  **Must NOT do**:
  - 不要持久化在线状态到数据库（仅内存）

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 3
  - **Blocks**: None
  - **Blocked By**: T15

  **QA Scenarios**:
  ```
  Scenario: 在线状态变更
    Tool: Bash (curl + websocat)
    Steps:
      1. 用户A WS 连接 → GET /api/presence/:channel_id → 包含 user_A
      2. 断开用户A WS → 等待 60s → GET /api/presence/:channel_id → 不包含 user_A
    Expected Result: 连接时包含, 超时后不包含
    Evidence: .sisyphus/evidence/task-23-presence.json
  ```

  **Commit**: YES
  - Message: `feat(presence): add online/offline presence via WebSocket`
  - Files: `src/api/presence.rs`, `src/ws/mod.rs`

- [x] 24. Typing 指示器 WS 推送

  **What to do**:
  - 前端：用户输入时（debounce 300ms），发送 `{"type":"typing_start","channel_id":"..."}` 到 WS
  - 前端：用户停止输入（2s 无输入或发送消息后），发送 `{"type":"typing_stop","channel_id":"..."}`
  - 后端：收到 typing_start → 广播 `{"type":"typing","channel_id":"...","user_id":"...","thread_parent_cursor":null}`（排除发送者）
  - 后端：收到 typing_stop → 停止广播（通过超时自动清理：5s 无 typing_start → 视为停止）
  - 前端 `presenceStore`：typingUsers map (channel_id → [user_ids])，UI 显示 "Alice is typing..."
  - TDD：typing 事件推送、非发送者收到、超时自动清除

  **Must NOT do**:
  - 不要将 typing 事件持久化

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 3
  - **Blocks**: T32
  - **Blocked By**: T15

  **QA Scenarios**:
  ```
  Scenario: Typing 指示器
    Tool: Bash (curl + websocat)
    Steps:
      1. 用户A WS 连接
      2. 用户B WS 连接（监听）
      3. 用户A WS 发送 {"type":"typing_start","channel_id":"<ch>"}
      4. 检查用户B WS 收到的帧
    Expected Result: 用户B 收到 {"type":"typing","channel_id":"<ch>","user_id":"<user_A>"}
    Evidence: .sisyphus/evidence/task-24-typing.json
  ```

  **Commit**: YES
  - Message: `feat(typing): add typing indicators via WebSocket`
  - Files: `src/ws/mod.rs`

- [x] 25. Emoji 反应 API + WS 同步

  **What to do**:
  - `POST /api/messages/:msg_id/reactions`：添加反应（`{"emoji":"👍"}`）
  - `DELETE /api/messages/:msg_id/reactions/:emoji`：移除自己的反应
  - `GET /api/messages/:msg_id/reactions`：获取消息的反应汇总（每个 emoji 的计数 + 当前用户是否已反应）
  - 反应变更时 WS 推送 `reaction_update` 事件到频道
  - TDD：添加反应、重复添加幂等（IGNORE）、移除反应、反应计数准确性

  **Must NOT do**:
  - 不要让用户移除他人的反应
  - 不要接受非 Unicode emoji

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 3
  - **Blocks**: T32
  - **Blocked By**: T5, T15

  **QA Scenarios**:
  ```
  Scenario: Emoji 反应流程
    Tool: Bash (curl)
    Steps:
      1. 用户A 对消息 M 添加 👍
      2. GET /api/messages/M/reactions → {"👍": {"count":1,"reacted_by_me":true}}
      3. 用户B 添加 👍
      4. GET /api/messages/M/reactions → {"👍": {"count":2,"reacted_by_me":false}}
      5. 用户A 移除 👍
      6. GET /api/messages/M/reactions → {"👍": {"count":1,"reacted_by_me":false}}
    Expected Result: 计数正确，reacted_by_me 按用户正确返回
    Evidence: .sisyphus/evidence/task-25-reactions.json
  ```

  **Commit**: YES
  - Message: `feat(reactions): add emoji reaction API + WS sync`
  - Files: `src/api/reactions.rs`

- [x] 26. FTS5 消息搜索 API

  **What to do**:
  - `GET /api/search?q=keyword&channel_id=optional&limit=20`：
    - 使用 FTS5 BM25 排序：`SELECT ... FROM messages_fts WHERE messages_fts MATCH ? ORDER BY bm25(messages_fts) LIMIT ?`
    - 搜索结果返回 snippet（`snippet(messages_fts, 1, '<mark>', '</mark>', '...', 20)`）
    - 权限过滤：仅搜索用户所在频道的消息（`WHERE channel_id IN (SELECT channel_id FROM channel_members WHERE user_id = ?)`）
  - 支持搜索操作符：短语搜索 `"exact phrase"`、前缀搜索 `prefix*`、布尔 `AND/OR/NOT`
  - TDD：精确匹配、模糊匹配、无结果、权限过滤

  **Must NOT do**:
  - 不要搜索用户不在的频道消息
  - 不要返回超过 limit 条结果

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 3
  - **Blocks**: None
  - **Blocked By**: T5, T16

  **QA Scenarios**:
  ```
  Scenario: FTS5 全文搜索
    Tool: Bash (curl)
    Steps:
      1. 发送消息 "deploy to production at 3pm"
      2. 发送消息 "development environment setup"
      3. GET /api/search?q=production
      4. GET /api/search?q=devel*
    Expected Result: 步骤3返回包含 "production" 的消息（snippet 高亮），步骤4返回包含 "development" 的消息（前缀匹配），其他用户搜索不到非所在频道消息
    Evidence: .sisyphus/evidence/task-26-search.json
  ```

  **Commit**: YES
  - Message: `feat(search): add FTS5 full-text message search`
  - Files: `src/api/search.rs`

- [x] 27. 消息软删除 API + WS 通知

  **What to do**:
  - `DELETE /api/messages/:msg_id`：仅消息发送者可删除，设置 `deleted_at = now()`
  - 删除后 WS 推送 `{"type":"msg_deleted","channel_id":"...","cursor":1054}` 到频道
  - 消息拉取 API 过滤：`WHERE deleted_at IS NULL`（默认不返回已删除消息）
  - 前端：收到 msg_deleted → 从消息列表移除对应消息的 DOM（或显示 "This message was deleted"）
  - TDD：发送者删除成功、非发送者被拒、删除后拉取不返回、WS 事件收到

  **Must NOT do**:
  - 不要硬删除消息（数据保留用于合规审计）
  - 不要让非发送者删除他人消息

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 3
  - **Blocks**: None
  - **Blocked By**: T14, T15

  **QA Scenarios**:
  ```
  Scenario: 软删除消息
    Tool: Bash (curl)
    Steps:
      1. 用户A发送消息 → 获得 cursor: 100
      2. 用户A删除消息 → 204
      3. 用户A拉取消息 → 不包含 cursor 100
      4. 用户B（非发送者）尝试删除 → 403
    Expected Result: 步骤2返回204, 步骤3不包含该消息, 步骤4返回403
    Evidence: .sisyphus/evidence/task-27-soft-delete.json
  ```

  **Commit**: YES
  - Message: `feat(messages): add soft delete API + WS notification`
  - Files: `src/api/messages.rs`

- [x] 28. 消息线程 API

  **What to do**:
  - 修改消息发送 API：如果请求包含 `thread_parent_id`，设置 `messages.thread_parent_id` 字段
  - `GET /api/channels/:channel_id/messages/:msg_id/thread`：获取线程回复（`WHERE thread_parent_id = :msg_id ORDER BY created_at`）
  - 线程回复发送时 WS 推送 `thread_reply` 事件（含 `thread_parent_cursor` 字段）
  - 线程消息的 `preview` 字段显示 "N replies"
  - 前端：点击线程回复数 → 内联展开线程回复列表（Slack 风格，平级）
  - TDD：发送线程回复、拉取线程、非成员不可见线程、线程回复的 cursor 正确

  **Must NOT do**:
  - 不要支持嵌套线程（回复的回复），仅平级
  - 不要在 channel 主消息列表的 cursor sync 中包含线程回复（cursor 仅计算 top-level 消息）

  **Recommended Agent Profile**:
  - **Category**: `unspecified-low`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 4
  - **Blocks**: None
  - **Blocked By**: T14, T15

  **QA Scenarios**:
  ```
  Scenario: 消息线程回复
    Tool: Bash (curl)
    Steps:
      1. 用户A发送top-level消息 → cursor: 100
      2. 用户B发送线程回复 (thread_parent_id=100) → cursor: 101
      3. GET /api/channels/:id/messages/100/thread → 包含 cursor 101
      4. GET /api/channels/:id/messages?after_cursor=99 → 仅包含 100（不包含 101）
    Expected Result: 线程回复仅在线程查询中出现，不在主通道消息列表中
    Evidence: .sisyphus/evidence/task-28-thread.json
  ```

  **Commit**: YES
  - Message: `feat(threads): add Slack-style inline message threads`
  - Files: `src/api/messages.rs`

- [x] 29. Monaco Editor 集成 + 代码片段消息

  **What to do**:
  - 安装 `@monaco-editor/react` 包，配置 lazy loading
  - 创建 `src/components/CodeSnippetInput.tsx`：语言选择下拉框 + Monaco Editor + 代码内容
  - 消息发送时：`msg_type = "code"`, `payload = {"language":"rust","code":"fn main() {}","filename":"main.rs"}`
  - 创建 `src/components/CodeMessage.tsx`：代码块渲染（Monaco 只读模式），语法高亮，行号
  - Monaco 通过 `manualChunks` 单独打包（已在 T3 配置）
  - TDD：代码发送、代码渲染、语言切换

  **Must NOT do**:
  - 不要在首页立即加载 Monaco（lazy load via React.lazy）

  **Recommended Agent Profile**:
  - **Category**: `visual-engineering`
  - **Skills**: `["frontend-ui-ux"]`

  **Parallelization**:
  - **Parallel Group**: Wave 4
  - **Blocks**: None
  - **Blocked By**: T2, T19

  **QA Scenarios**:
  ```
  Scenario: 代码片段发送与渲染
    Tool: Playwright
    Steps:
      1. 在频道中点击 Code Snippet 按钮
      2. 选择语言 "Rust"
      3. 输入代码 "fn main() { println!(\"hello\"); }"
      4. 点击发送
      5. 检查消息列表中出现语法高亮的代码块
    Expected Result: 代码块正确渲染，行号显示，关键字高亮
    Evidence: .sisyphus/evidence/task-29-code-snippet.png
  ```

  **Commit**: YES
  - Message: `feat(frontend): integrate Monaco Editor for code snippets`
  - Files: `frontend/src/components/CodeSnippetInput.tsx`, `frontend/src/components/CodeMessage.tsx`

- [x] 30. WS Hook + Cursor Sync 前端逻辑

  **What to do**:
  - `src/hooks/useWebSocket.ts`：WS 连接管理 hook
    - 连接建立后发送 JWT 验证
    - 自动重连（指数退避：1s, 2s, 4s, 8s, 16s, max 5 retries）
    - 重连后：遍历所有频道 `lastCursor`，调用 REST API 补齐遗漏消息
  - `src/hooks/useCursorSync.ts`：Cursor 同步管理
    - 维护 `lastCursor` per channel 在 Zustand messageStore
    - 收到 `new_msg` WS 事件 → 如果当前正在查看该频道 → 立即拉取消息
    - 频道切换时：`GET /api/channels/:id/messages?after_cursor=lastCursor`
  - 消息去重：基于 `msg_id`（UUID）去重（多设备场景）
  - TDD：WS 连接建立、重连、cursor 补齐

  **Must NOT do**:
  - 不要在 WS 断开时清空消息列表
  - 不要每个组件都创建 WS 连接（全局单例）

  **Recommended Agent Profile**:
  - **Category**: `unspecified-low`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 4
  - **Blocks**: T36
  - **Blocked By**: T15, T19

  **QA Scenarios**:
  ```
  Scenario: WS 重连后消息补齐
    Tool: Playwright + Bash
    Steps:
      1. 用户B 在频道中，WS 连接正常
      2. 模拟断开（停止/重启后端）
      3. 用户A 发送 3 条消息
      4. 重启后端，用户B WS 重连
      5. 检查消息列表是否包含遗漏的 3 条消息
    Expected Result: 所有遗漏消息补齐，无重复
    Evidence: .sisyphus/evidence/task-30-reconnect.png
  ```

  **Commit**: YES
  - Message: `feat(frontend): add WebSocket hook + cursor sync logic`
  - Files: `frontend/src/hooks/useWebSocket.ts`, `frontend/src/hooks/useCursorSync.ts`

- [x] 31. 前端：权限流 UI + 申请/邀请提醒

  **What to do**:
  - `src/components/JoinRequestButton.tsx`：公开频道显示 "Request to Join" 按钮
  - `src/components/PendingRequestsBadge.tsx`：owner/admin 在侧边栏看到待审批数徽章
  - `src/pages/RequestsPage.tsx`：待审批申请列表（approve/reject 按钮）
  - `src/components/InvitationToast.tsx`：收到邀请时弹出 Toast（accept/reject）
  - `src/components/ChannelSettingsModal.tsx`：频道设置（修改名称、管理成员、存档）
  - `src/components/MemberList.tsx`：频道成员列表（显示角色 badge、owner 可见移除/kick 按钮）
  - TDD：组件渲染、按钮交互、Toast 弹出

  **Must NOT do**:
  - 不要让非 owner/admin 看到管理按钮

  **Recommended Agent Profile**:
  - **Category**: `visual-engineering`
  - **Skills**: `["frontend-ui-ux"]`

  **Parallelization**:
  - **Parallel Group**: Wave 4
  - **Blocks**: T36
  - **Blocked By**: T18, T19, T20

  **QA Scenarios**:
  ```
  Scenario: 申请加入 → Owner 审批
    Tool: Playwright
    Steps:
      1. 用户B 浏览频道列表 → 点击 "Request to Join"
      2. 用户A (owner) 看到待审批徽章
      3. 用户A 打开待审批页面 → Approve
      4. 用户B 的频道列表中出现新频道
    Expected Result: 申请 → 审批 → 成员加入流程完整
    Evidence: .sisyphus/evidence/task-31-permission-ui.png
  ```

  **Commit**: YES
  - Message: `feat(frontend): add permission flow UI + invite toasts`
  - Files: `frontend/src/components/JoinRequestButton.tsx`, `frontend/src/components/PendingRequestsBadge.tsx`, `frontend/src/pages/RequestsPage.tsx`, `frontend/src/components/InvitationToast.tsx`, `frontend/src/components/ChannelSettingsModal.tsx`, `frontend/src/components/MemberList.tsx`

- [x] 32. 前端：Emoji 反应 + 打字指示器 UI

  **What to do**:
  - `src/components/ReactionPicker.tsx`：悬停消息 → 显示 Emoji 反应选择器（常用 Unicode emoji）
  - `src/components/ReactionBar.tsx`：消息下方显示反应计数 + 高亮当前用户已反应项
  - 反应添加/移除：点击 emoji → API 调用（toggle 行为）
  - `src/components/TypingIndicator.tsx`：频道底部显示 "Alice, Bob are typing..."
  - 顶部的在线状态点（绿色/灰色圆点 + 用户名旁）
  - TDD：反应显示、添加反应、移除反应、打字指示器显示/消失

  **Must NOT do**:
  - 不要在反应选择器中显示自定义 emoji

  **Recommended Agent Profile**:
  - **Category**: `visual-engineering`
  - **Skills**: `["frontend-ui-ux"]`

  **Parallelization**:
  - **Parallel Group**: Wave 4
  - **Blocks**: T36
  - **Blocked By**: T19, T24, T25

  **QA Scenarios**:
  ```
  Scenario: Emoji 反应交互
    Tool: Playwright
    Steps:
      1. 悬停消息 → 点击反应按钮
      2. 选择 👍 emoji
      3. 检查消息下方显示 "👍 1"
      4. 再次点击 👍 → 反应移除 → 显示消失
    Expected Result: 反应正确添加/移除，计数更新
    Evidence: .sisyphus/evidence/task-32-reaction-ui.png

  Scenario: 打字指示器
    Tool: Playwright
    Steps:
      1. 用户A 在频道中输入文字
      2. 用户B 的频道底部显示 "Alice is typing..."
      3. 用户A 停止输入 → 指示器消失
    Expected Result: 打字指示器正确显示/消失
    Evidence: .sisyphus/evidence/task-32-typing-ui.png
  ```

  **Commit**: YES
  - Message: `feat(frontend): add emoji reaction picker + typing indicator UI`
  - Files: `frontend/src/components/ReactionPicker.tsx`, `frontend/src/components/ReactionBar.tsx`, `frontend/src/components/TypingIndicator.tsx`

- [x] 33. 反向代理配置 + systemd unit

  **What to do**:
  - 创建 `deploy/nginx.conf`：Nginx 反向代理配置模板（TLS 终止 + proxy_pass → localhost:3000）
  - 创建 `deploy/im-server.service`：systemd unit 文件
    - `ExecStart=/opt/im-server/im-server`
    - `WorkingDirectory=/opt/im-server`
    - `Restart=always`, `RestartSec=5`
    - `EnvironmentFile=/opt/im-server/.env`
  - 创建 `deploy/install.sh`：自动化部署脚本
    - 复制二进制到 `/opt/im-server/`
    - 安装 systemd unit
    - 建议的目录权限
  - 创建 `.env.example`：`JWT_SECRET=`, `INVITE_CODE=`, `SERVER_PORT=3000`, `UPLOAD_MAX_SIZE=52428800`

  **Must NOT do**:
  - 不要假设 80/443 端口（默认 3000，由反向代理处理）

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 4
  - **Blocks**: None
  - **Blocked By**: T10

  **QA Scenarios**:
  ```
  Scenario: systemd unit 语法有效
    Tool: Bash
    Steps:
      1. systemd-analyze verify deploy/im-server.service 2>&1 || true
    Expected Result: 无严重语法错误
    Evidence: .sisyphus/evidence/task-33-systemd.txt
  ```

  **Commit**: YES
  - Message: `chore(deploy): add nginx config + systemd unit + install script`
  - Files: `deploy/nginx.conf`, `deploy/im-server.service`, `deploy/install.sh`, `.env.example`

- [x] 34. TLS/HTTPS 支持

  **What to do**:
  - 通过 rustls + axum-server 实现内置 HTTPS
  - `Cargo.toml`：添加 `axum-server` + `rustls` + `rustls-pemfile` 依赖
  - 配置模式（通过环境变量 `TLS_MODE`）：
    - `none`：仅 HTTP（开发默认）
    - `self-signed`：自签名证书（内网测试）
    - `lets-encrypt`：Let's Encrypt 自动证书（需要域名 + 端口 80/443）
  - 开发环境：生成自签名证书脚本 `scripts/gen-self-signed-cert.sh`
  - HTTP → HTTPS 重定向（Let's Encrypt 模式下）

  **Must NOT do**:
  - 不要强制 HTTPS（允许纯 HTTP 用于内网部署）
  - 不要在代码中硬编码证书路径

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 5
  - **Blocks**: T37
  - **Blocked By**: None

  **QA Scenarios**:
  ```
  Scenario: 自签名 HTTPS 连接
    Tool: Bash (curl)
    Steps:
      1. TLS_MODE=self-signed cargo run &
      2. sleep 2
      3. curl -k -s -o /dev/null -w '%{http_code}' https://localhost:3443/api/health
    Expected Result: HTTP 200
    Evidence: .sisyphus/evidence/task-34-tls.txt
  ```

  **Commit**: YES
  - Message: `feat(tls): add rustls HTTPS support with multiple modes`
  - Files: `Cargo.toml`, `src/main.rs`, `scripts/gen-self-signed-cert.sh`

- [x] 35. SQLite PRAGMA 调优 + 性能验证

  **What to do**:
  - 确认 T5 中的 PRAGMA 设置生效（启动时日志输出当前 PRAGMA 值）
  - 添加 `PRAGMA optimize` 在关闭时调用（分析表统计）
  - 性能测试脚本 `scripts/bench.sh`：
    - 插入 1000 条消息的吞吐量（writes/sec）
    - 并发读取 100 请求的延迟分布（p50, p95, p99）
    - 50 并发 WS 连接的内存占用
  - 调整参数：cache_size, mmap_size, busy_timeout 根据测试结果微调
  - 添加 `data/` 目录磁盘空间监控（写操作前检查可用空间 > 100MB）

  **Must NOT do**:
  - 不要在每次连接时重复设置 PRAGMA（通过 SqliteConnectOptions 初始化一次）

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 5
  - **Blocks**: T37
  - **Blocked By**: T5

  **QA Scenarios**:
  ```
  Scenario: WAL 性能基准
    Tool: Bash
    Steps:
      1. bash scripts/bench.sh 2>&1
    Expected Result: 插入速度 > 1000 writes/sec，读取 p95 < 50ms，50 WS 连接内存 < 500MB
    Evidence: .sisyphus/evidence/task-35-bench.txt
  ```

  **Commit**: YES
  - Message: `perf(db): fine-tune SQLite PRAGMAs + add benchmark script`
  - Files: `src/db/mod.rs`, `scripts/bench.sh`

- [x] 36. 前端路由守卫 + 整体交互完善

  **What to do**:
  - 完善 `AuthGuard`：验证 token 有效性（检查 expiry，过期 → 自动刷新或跳转登录）
  - 路由守卫：`/channels/*` 需登录，`/login` 和 `/register` 已登录时重定向到 `/channels`
  - 空状态处理：无频道时显示引导提示，无消息时显示 "No messages yet"，搜索无结果时显示提示
  - 加载状态：频道列表加载 skeleton，消息加载 spinner
  - 错误边界：React Error Boundary 捕获渲染错误 → 显示 "Something went wrong"
  - 全局 Toast 通知：错误提示、成功提示（使用自定义 Toast 组件或 lucide-react icons）
  - 快捷键：`Ctrl+K` 打开搜索，`Esc` 关闭弹窗
  - 响应式基础：侧边栏在小屏幕可折叠

  **Must NOT do**:
  - 不要使用 CSS-in-JS（统一 TailwindCSS）
  - 不要引入额外的 UI 库（用 TailwindCSS + lucide-react）

  **Recommended Agent Profile**:
  - **Category**: `visual-engineering`
  - **Skills**: `["frontend-ui-ux"]`

  **Parallelization**:
  - **Parallel Group**: Wave 5
  - **Blocks**: T37
  - **Blocked By**: T18, T19, T30, T31, T32

  **QA Scenarios**:
  ```
  Scenario: 未登录访问受保护路由
    Tool: Playwright
    Steps:
      1. 清除 localStorage
      2. 访问 http://localhost:5173/channels
      3. 等待重定向
    Expected Result: 被重定向到 /login
    Evidence: .sisyphus/evidence/task-36-auth-guard.png

  Scenario: Token 过期自动刷新
    Tool: Playwright
    Steps:
      1. 使用即将过期的 token 访问 /channels
      2. 检查是否自动调用 /api/auth/refresh
      3. 继续正常使用
    Expected Result: 无中断，自动刷新成功
    Evidence: .sisyphus/evidence/task-36-token-refresh.png
  ```

  **Commit**: YES
  - Message: `feat(frontend): add route guards, empty states, error boundaries, keyboard shortcuts`
  - Files: `frontend/src/components/AuthGuard.tsx`, `frontend/src/components/ErrorBoundary.tsx`, `frontend/src/components/Toast.tsx`, `frontend/src/App.tsx`

- [x] 37. 全量集成测试

  **What to do**:
  - 后端集成测试（`tests/integration/`）：
    - `auth_flow.rs`：注册 → 登录 → 刷新 → 登出 → 令牌失效
    - `channel_flow.rs`：创建 → 加入 → 发消息 → 存档 → 读消息
    - `permission_flow.rs`：申请 → 审批 → 角色变更 → 踢人
    - `message_flow.rs`：发送 → 游标拉取 → 编辑 → 软删除 → FTS5 搜索
    - `ws_flow.rs`：连接 → 推送 → 重连 → 消息补齐
  - 前端集成测试（Playwright）：
    - `auth.spec.ts`：登录/注册页面完整流程
    - `chat.spec.ts`：频道消息发送接收 + WS 实时更新
    - `permissions.spec.ts`：申请/邀请流程 UI
    - `reactions.spec.ts`：Emoji 反应交互
  - 端到端测试脚本 `scripts/e2e-test.sh`：启动服务器 → 运行所有测试 → 关闭服务器

  **Must NOT do**:
  - 不要在集成测试中使用生产数据
  - 不要跳过任何测试文件

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: `["playwright"]`

  **Parallelization**:
  - **Parallel Group**: Wave 5
  - **Blocks**: T38, F1-F4
  - **Blocked By**: ALL previous tasks

  **QA Scenarios**:
  ```
  Scenario: 全量测试通过
    Tool: Bash
    Steps:
      1. cargo test --test integration 2>&1
      2. cd frontend && bun test 2>&1
      3. cd frontend && npx playwright test 2>&1
    Expected Result: 所有测试通过，0 failures
    Evidence: .sisyphus/evidence/task-37-all-tests.txt
  ```

  **Commit**: YES（每个测试文件独立提交）
  - Message: `test(integration): add full integration test suite`
  - Files: `tests/integration/`, `frontend/tests/`, `scripts/e2e-test.sh`

- [x] 38. 构建脚本 + release 编译

  **What to do**:
  - 创建 `scripts/build.sh`：一键构建脚本
    - 检查前置条件（Rust 1.96+, Node.js/bun, frontend/dist/）
    - `cd frontend && bun run build`（Vite production build，输出到 `dist/`）
    - `cargo build --release`（编译 release 二进制，嵌入前端 dist）
    - 输出二进制路径和大小
  - 创建 `Makefile`：快捷命令（`make dev`, `make build`, `make test`, `make clean`）
  - GitHub Actions CI 配置（可选，`.github/workflows/ci.yml`）：
    - `cargo test --release`
    - `cargo clippy -- -D warnings`
    - `cargo fmt --check`
    - `cd frontend && bun test`
    - `cd frontend && bun run build`
  - `README.md`：项目说明、快速启动、部署指南、API 文档链接

  **Must NOT do**:
  - 不要在构建脚本中硬编码路径

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: `[]`

  **Parallelization**:
  - **Parallel Group**: Wave 5
  - **Blocks**: F1-F4
  - **Blocked By**: ALL

  **QA Scenarios**:
  ```
  Scenario: 一键构建成功
    Tool: Bash
    Steps:
      1. bash scripts/build.sh 2>&1
    Expected Result: 退出码 0，输出 im-server 二进制路径和大小
    Evidence: .sisyphus/evidence/task-38-build.txt
  ```

  **Commit**: YES
  - Message: `chore(build): add build script + Makefile + CI config + README`
  - Files: `scripts/build.sh`, `Makefile`, `.github/workflows/ci.yml`, `README.md`

---

## Final Verification Wave

- [x] F1. **Plan Compliance Audit** — `oracle`
  读取方案端到端。验证每个 Must Have 实现存在（读文件、curl 端点、运行命令）。检查每个 Must NOT Have 模式不在代码中（搜索禁止模式并拒绝，标注 file:line）。验证 evidence 文件在 `.sisyphus/evidence/` 中。
  输出：`Must Have [N/N] | Must NOT Have [N/N] | Tasks [N/N] | VERDICT: APPROVE/REJECT`

- [x] F2. **Code Quality Review** — `unspecified-high`
  运行 `tsc --noEmit` + `cargo clippy` + `cargo test` + `bun test`。审查所有变更文件：`as any`/`@ts-ignore`、空 catch、console.log（产品代码）、注释掉的代码、未使用导入。检查 AI slop：过度注释、过度抽象、通用命名（data/result/item/temp）。
  输出：`Build [PASS/FAIL] | Lint [PASS/FAIL] | Tests [N pass/N fail] | Files [N clean/N issues] | VERDICT`

- [x] F3. **Real Manual QA** — `unspecified-high` (+ `playwright` skill)
  从干净状态启动。执行每个任务的每个 QA scenario — 按精确步骤，捕获证据。测试跨任务集成（功能协同工作）。测试边缘情况：空状态、无效输入、快速操作。保存至 `.sisyphus/evidence/final-qa/`。
  输出：`Scenarios [N/N pass] | Integration [N/N] | Edge Cases [N tested] | VERDICT`

- [x] F4. **Scope Fidelity Check** — `deep`
  对每个任务：读 "What to do"、读实际 diff (git log/diff)。验证 1:1 — spec 中所有内容均已构建（无遗漏），spec 外内容均未构建（无蔓延）。检查 "Must NOT do" 合规性。检测跨任务污染：Task N 触碰 Task M 的文件。标记未统计变更。
  输出：`Tasks [N/N compliant] | Contamination [CLEAN/N issues] | Unaccounted [CLEAN/N files] | VERDICT`

---

## Commit Strategy

- **提交粒度**：每完成一个 TODO 提交一次
- **提交格式**：`type(scope): desc`
- **预提交**：`cargo test && cd frontend && bun test`（相关测试通过后提交）
- **分支**：`main`（单分支，本方案为全量构建）

---

## Success Criteria

### 验证命令
```bash
# 健康检查
curl -s http://localhost:3000/api/health
# Expected: {"status":"ok","db":"connected"}

# 注册用户（需要邀请码，测试用预设码 IM2024）
curl -s -X POST http://localhost:3000/api/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"Admin123!","invite_code":"IM2024"}'
# Expected: {"token":"eyJ..."}, HTTP 201

# 全量测试
cargo test && cd frontend && bun test
# Expected: all tests pass
```

### 最终检查清单
- [ ] 所有 "Must Have" 存在且可验证
- [ ] 所有 "Must NOT Have" 无违规
- [ ] `cargo test` 全部通过
- [ ] `cd frontend && bun test` 全部通过
- [ ] `im-server` 二进制可独立运行（无外部依赖）
- [ ] `data/` 目录自动创建，数据库自动初始化
