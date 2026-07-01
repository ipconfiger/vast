# Playwright E2E 全业务测试工程

## 目标
覆盖 IM 系统全部前端业务逻辑，所有测试独立可重复运行。

## 测试文件规划

| 文件 | 覆盖范围 | 预估测试数 |
|------|----------|:---------:|
| `helpers.ts` | 共享工具：register, login, createChannel, sendMessage | — |
| `auth.spec.ts` | 注册/登录/鉴权守卫/token刷新（已有，增强） | 7→9 |
| `channels.spec.ts` | 频道 CRUD / 存档 / 设置 / 切换 | 6 |
| `chat.spec.ts` | 消息发送/显示/删除/分页（已有，增强） | 3→8 |
| `permissions.spec.ts` | 申请/审批/拒绝/邀请/角色变更/踢人 | 7 |
| `dm.spec.ts` | 1:1 DM / 群组 DM / 权限隔离 | 5 |
| `search.spec.ts` | FTS5 搜索 / 前缀搜索 / 权限过滤 | 4 |
| `threads.spec.ts` | 线程回复 / 线程排除 / 线程查看 | 4 |
| `reactions.spec.ts` | 添加反应 / 计数 / 移除 / 跨用户 | 4 |
| **总计** | | **~48** |

## helpers.ts 设计
```ts
export async function registerUser(page, username, password, inviteCode='IM2024')
export async function loginUser(page, username, password)
export async function createChannel(page, name, description?)
export async function sendMessage(page, channelId, text)
export async function ensureLoggedIn(page) // auto-register unique user
```
