# QuotaLoom

<p align="center">
  <img src="images/main.gif" alt="QuotaLoom floating panel" width="420">
</p>

<p align="center">
  简体中文 · <a href="README.en.md">English</a>
</p>

QuotaLoom 是一款本地优先的桌面用量统计工具，支持 Claude Code、Codex CLI 和 ChatGPT Codex。它增量解析本机会话 JSONL，提供常驻桌面的悬浮窗和完整统计面板。

> [!IMPORTANT]
> 本项目是社区维护的非官方工具，与 OpenAI、Anthropic 或其关联公司无隶属或背书关系。Codex、ChatGPT、Claude 等名称归各自权利人所有。

## 功能

- 查看今日、过去 24 小时、7 天或 30 天的 Token 用量。
- 分开展示新输入、缓存输入和模型输出。
- 统计调用次数、会话数、缓存命中率和模型分布。
- 对 ChatGPT Codex 账户显示每周额度使用与重置时间。
- 按模型价格表估算费用，并支持自定义价格倍率。
- 增量读取仍在写入的日志，自动响应追加、截断和文件变化。
- 通过悬浮窗、完整面板和系统托盘快速查看数据。

费用仅为估算值，不代表服务商账单。

## 数据来源

应用根据所选数据目录的结构识别来源：

| 来源          | 会话目录                          |
| ------------- | --------------------------------- |
| Claude Code   | `projects/`                       |
| Codex CLI     | `sessions/`、`archived_sessions/` |
| ChatGPT Codex | `sessions/`、`archived_sessions/` |

应用优先读取 `$CODEX_HOME`，默认回退到 `~/.codex`。Claude Code 用户可以在界面中选择相应的 `~/.claude` 目录。

## 隐私

- 会话日志只在本机读取，不会上传提示词或回复内容。
- 本地 SQLite 只保存统计所需的会话标识、时间、模型、Token 数量、解析进度和源文件路径。
- 应用启动时以及手动刷新价格时，会向 `https://models.dev/api.json` 发出 HTTPS GET 请求；请求中不包含会话数据。
- 显示 ChatGPT Codex 每周额度时，应用会启动本机 `codex app-server` 并请求账户限额。Codex CLI 可能使用现有登录状态访问服务商；本应用不读取或保存登录凭据。
- 项目不包含遥测或行为分析。

完整说明见[隐私与数据处理](docs/PRIVACY.md)。提交问题时，请勿上传原始会话 JSONL、应用数据库或包含私人路径的截图。

## 安装

稳定版本发布后，可从仓库的 **Releases** 页面下载对应平台的安装包。发布目标包括：

- macOS：Apple Silicon 与 Intel
- Windows：x86-64
- Linux：x86-64 AppImage、Deb 或 RPM（以每个 Release 实际资产为准）

## 本地开发

需要：

- Node.js 24
- npm 11 或更高版本
- Rust 1.96
- [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)

安装依赖并启动前端演示：

```bash
npm ci
npm run dev
```

启动桌面应用：

```bash
npm run tauri dev
```

Linux 的 Tauri 桌面构建需要 WebKitGTK、AppIndicator 和常用编译工具。Debian/Ubuntu 可安装：

```bash
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev patchelf \
  pkg-config libssl-dev
```

## 质量检查

```bash
npm run check
npm audit --audit-level=high

cd src-tauri
cargo fmt --check
cargo clippy --locked --no-default-features --all-targets -- -D warnings
cargo test --locked --no-default-features
```

完整桌面检查需要已安装当前平台的 Tauri 系统依赖：

```bash
cd src-tauri
cargo check --locked --all-targets --all-features
```

## 项目结构

```text
src/                    React 前端与悬浮窗
src-tauri/src/core/     日志解析、SQLite 和价格计算核心
src-tauri/src/desktop.rs
                        Tauri 窗口、托盘和命令层
docs/                   架构、隐私和发布文档
.github/                CI、发布、Issue 和 PR 配置
```

架构细节见[架构说明](docs/ARCHITECTURE.md)。

## 参与贡献

提交改动前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md) 和 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)。安全问题请按 [SECURITY.md](SECURITY.md) 私下报告。

## 许可证

本项目使用 [MIT License](LICENSE)。
