# 架构说明

QuotaLoom 是一个 Tauri 2 桌面应用。React 前端只负责呈现和用户交互，Rust 核心负责本地文件读取、增量解析、持久化与价格计算。

## 主要组件

```text
Claude/Codex JSONL
        │
        ▼
增量解析器 ──► SQLite 统计缓存 ──► UsageService
                                      │
models.dev 价格目录 ──────────────────┤
codex app-server ──► 每周账户额度 ─────┤
                                      ▼
                              Tauri commands/events
                                      │
                         ┌────────────┴────────────┐
                         ▼                         ▼
                    完整 Dashboard              悬浮窗口
```

### Rust 核心

- `parser.rs`：从上次字节偏移继续读取完整 JSONL 行；处理追加、截断、未完成末行和会话重放。
- `database.rs`：保存统计事件、解析游标和模型价格，生成时间范围内的聚合结果。
- `service.rs`：协调数据源检测、同步、文件监听、价格刷新和数据目录偏好。
- `pricing.rs`：使用十进制定点数计算估算成本，避免浮点金额误差。
- `types.rs`：前后端共享数据结构的 Rust 定义。

统计事件包含会话标识、时间、模型、Token 数量和源文件路径，不保存提示词或回复正文。

### Tauri 桌面层

`desktop.rs` 创建 Dashboard、悬浮窗和托盘，暴露最小化命令，并通过 `usage-updated` 事件通知前端刷新。窗口 capability 分开配置，CSP 禁止远程脚本、对象和 frame。

### React 前端

- `src/lib/api.ts` 封装 Tauri 命令；浏览器开发模式返回演示数据。
- `src/hooks/useUsage.ts` 使用 React Query 管理刷新与缓存。
- `src/dashboard/` 和 `src/floating/` 分别实现完整面板和悬浮窗口。

## 数据流与失败策略

1. 启动时打开本地 SQLite，并从保存的游标继续同步会话文件。
2. 文件监听器发现变化后触发增量同步；Windows 网络路径使用轮询监听器。
3. 对 Codex 数据源，应用按需启动 `codex app-server --stdio`，读取 ChatGPT 账户的每周额度百分比与重置时间；结果只在内存中缓存 60 秒。
4. 价格目录或账户额度查询失败不会阻止本地用量统计，应用继续使用内置或已缓存价格。
5. 单个损坏或不可读文件会形成警告，不影响其他文件同步。

## 安全边界

- 文件系统访问发生在 Rust 后端，前端不能任意读取文件。
- 目录选择由 Tauri Dialog capability 提供。
- Rust 后端只会从受控候选路径或 `CODEX_BINARY` 启动 `codex app-server`，不接受前端提供的可执行文件或参数。
- 预期网络活动包括读取 `models.dev` 的公开价格目录，以及 Codex CLI 为读取账户限额而进行的服务商请求。
- SQLite 和偏好文件保存在操作系统分配的应用数据目录中。
