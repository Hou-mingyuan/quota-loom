# 贡献指南

感谢你为 QuotaLoom 提交改进。参与前请遵守 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)。

## 开始之前

- Bug 请先搜索现有 Issue，并使用 Bug 模板提交最小复现。
- 功能改动建议先提交 Feature Request，确认范围后再实现。
- 安全漏洞不要公开提交，请按照 [SECURITY.md](SECURITY.md) 私下报告。
- 不要在 Issue、日志或测试夹具中提交真实会话内容、Token、用户名或本地绝对路径。

## 开发环境

项目使用 Node.js 24、npm 和 Rust 1.96。安装平台对应的 [Tauri 2 前置依赖](https://v2.tauri.app/start/prerequisites/) 后运行：

```bash
npm ci
npm run tauri dev
```

浏览器模式使用演示数据，可运行：

```bash
npm run dev
```

## 提交改动

1. 从最新的 `main` 创建短生命周期分支。
2. 保持改动聚焦，并为行为变化补充测试和文档。
3. 使用清晰的提交信息；推荐 `feat:`、`fix:`、`docs:`、`test:`、`refactor:`、`chore:` 等前缀。
4. 在提交 PR 前运行完整检查：

   ```bash
   npm run check
   npm audit --audit-level=high
   cd src-tauri
   cargo fmt --check
   cargo clippy --locked --no-default-features --all-targets -- -D warnings
   cargo test --locked --no-default-features
   ```

5. 如果修改 Tauri 桌面层，再运行 `cargo check --locked --all-targets --all-features`。

## Pull Request 要求

- 说明问题、解决方案和验证方式。
- 关联相关 Issue。
- UI 变化提供截图或短视频，但必须清除私人数据。
- 不混入无关格式化或重构。
- 接受维护者要求的合理调整，并保持 CI 通过。

贡献内容在合并后将按本项目的 [MIT License](LICENSE) 发布。提交贡献即表示你有权按该许可证提供相关内容。
