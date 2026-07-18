# Changelog

本项目的显著变化将记录在此文件中。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

## [0.1.3] - 2026-07-18

### Fixed

- macOS 安装包改用完整的 ad-hoc 应用签名，避免 Apple Silicon 下载版本被误报为“已损坏”。
- 发布工作流增加 macOS 应用签名完整性检查。

## [0.1.2] - 2026-07-18

### Added

- 模型价格支持直接编辑输入、缓存输入和输出单价，未匹配目录的模型也可手动定价。
- 用户自定义价格持久化到本地数据库，并在刷新 models.dev 目录时保留。

## [0.1.0] - 2026-07-16

### Added

- 本地增量解析 Claude Code、Codex CLI 和 ChatGPT Codex 会话。
- Token、缓存、调用、模型分布和估算成本统计。
- 悬浮窗、完整面板和系统托盘。
- 账户周额度、剩余百分比及重置时间展示。
- 悬浮窗进度条按周额度或缓存命中率自适应展示。
- 本地价格目录刷新与模型倍率设置。
