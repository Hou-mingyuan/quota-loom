# 发布流程

发布由 `v<semver>` 标签触发。工作流构建 macOS Apple Silicon、macOS Intel、Windows x86-64 和 Linux x86-64 安装包，并自动创建公开的 GitHub Release。

## 准备版本

1. 更新 `CHANGELOG.md`，把 Unreleased 内容移动到带日期的版本标题下。
2. 同步修改以下三个版本号：
   - `package.json`
   - `src-tauri/Cargo.toml`
   - `src-tauri/tauri.conf.json`
3. 运行 `npm run version:check`。
4. 运行 README 中的全部质量检查和桌面构建检查。
5. 确认应用标识、版权人、仓库 URL、支持平台和隐私说明仍然准确。

## 签名状态

当前工作流发布未签名、未公证的安装包，并在 Release Notes 中显示安全警告。macOS Gatekeeper 和 Windows SmartScreen 可能在安装时拦截或警告；这不影响自动构建和发布。

后续需要正式签名时，应配置：

- macOS Developer ID 证书、证书密码、Apple ID / App Store Connect 公证凭据
- Windows 代码签名证书或受信任签名服务凭据
- Tauri Updater 私钥（仅在启用自动更新后）

这些凭据不得出现在 PR 工作流、仓库文件、日志或普通环境变量中。配置签名前，还需要恢复工作流中的平台签名步骤，并通过 GitHub Secrets 注入凭据。

## 创建发布

```bash
git tag -s v0.1.0 -m "QuotaLoom v0.1.0"
git push origin v0.1.0
```

标签必须与清单中的版本完全一致，否则工作流会失败。

## 发布后检查

- 所有平台构建任务通过。
- 文件名和架构正确，没有意外的调试产物。
- 未签名状态已在 Release Notes 中明确说明；启用签名后再验证 macOS、Windows 签名及 Apple 公证。
- 公开仓库的 GitHub Artifact Attestation 可验证；个人账户名下的私有仓库会跳过此步骤。
- 在干净设备或虚拟机上执行安装、启动、托盘、悬浮窗和数据目录选择冒烟测试。
- Release Notes 包含升级风险、已知限制和校验说明。
