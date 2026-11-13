<div align="center">

# ⚡ 飞流下载 · FeiLiu Downloader

**多线程高速下载器 — 最高 64 连接并行，完整断点续传。**

[English](../README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | [Deutsch](README.de.md)

[![Tauri 2](https://img.shields.io/badge/Tauri-2-blue)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-stable-orange)](https://rust-lang.org)
[![React 19](https://img.shields.io/badge/React-19-61dafb)](https://react.dev)
[![License](https://img.shields.io/badge/License-MIT-green)](#开源协议)

**[🌐 官方网站](https://ricky-chenzhuquan.github.io/feiliuDownload/)** — 选择你的平台下载。

</div>

---

## ✨ 功能特性

- 🚀 **多连接加速** — 文件自动切分为 8MiB 分片，最高 **64 连接并行**下载；对限速服务器效果拔群。
- ⏸️ **暂停 / 继续 / 重启无忧** — 分片进度实时落盘，中断后从精确字节恢复；支持**启动时自动恢复**。
- 🔍 **智能探测** — 自动识别文件名、大小、是否支持断点；不支持的服务器自动降级单连接。
- 🗂️ **多任务管理** — 并行任务，实时速度 / 剩余时间 / 连接数；一键全部暂停 / 全部继续。
- 🔄 **应用内自动更新** — 更新清单通过 GitHub Releases 发布，更新包经 minisign 签名校验。
- 🛡️ **纯净无广告** — 无广告、无捆绑、无数据收集，Rust 引擎本地运行。
- 🪶 **轻量极速** — 基于 Tauri 2，安装包仅约 4MB。

## 📥 下载

| 平台 | 链接 | 状态 |
|---|---|---|
| macOS（Apple Silicon） | [FeiLiu_0.1.0_aarch64.dmg](https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/FeiLiu_0.1.0_aarch64.dmg) | ✅ |
| Windows 10/11（64 位） | [FeiLiu_0.1.0_x64-setup.exe](https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/FeiLiu_0.1.0_x64-setup.exe) | ✅ CI 自动构建 |
| iOS | [FeiLiu_0.1.0_ios.ipa](https://raw.githubusercontent.com/ricky-chenzhuquan/feiliuDownload/main/website/downloads/FeiLiu_0.1.0_ios.ipa) · 开发签名 | ⚠️ 仅限已注册设备 · TestFlight 🔜 |
| Android | [FeiLiu_0.1.0_aarch64.apk](https://raw.githubusercontent.com/ricky-chenzhuquan/feiliuDownload/main/website/downloads/FeiLiu_0.1.0_aarch64.apk) · 自签名 | ✅ arm64-v8a |

👉 全部版本：[github.com/ricky-chenzhuquan/feiliuDownload/releases](https://github.com/ricky-chenzhuquan/feiliuDownload/releases)

## 🏗️ 项目结构

```
FeiLiu/
├── app/                        # Tauri 2 应用（桌面 + iOS）
│   ├── src/                    # React + TypeScript 前端
│   └── src-tauri/
│       └── src/downloader/     # Rust 下载引擎
│           ├── probe.rs        # URL 探测（大小 / 断点 / 文件名）
│           ├── engine.rs       # 分片引擎（并行队列 / 续传）
│           └── state.rs        # 分片持久化
├── website/                    # 官网（纯静态，英/中/日/德）
├── docs/                       # 多语言文档
├── scripts/                    # 图标与工具脚本
└── .github/workflows/          # CI：桌面构建、官网部署
```

## 🛠️ 本地构建

### 桌面版（macOS / Windows / Linux）

```bash
cd app
pnpm install
pnpm tauri dev        # 开发模式
pnpm tauri build      # 产出 .dmg / .msi / .exe
```

### iOS 版

前置：Xcode、Rust，以及 `xcodegen`、`cocoapods`（`gem install cocoapods --user-install`）、`libimobiledevice`（`brew install libimobiledevice`）。

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cd app
pnpm tauri ios init            # 仅首次
pnpm tauri ios build --target aarch64-sim
```

> iOS 限制：应用退到后台后下载会暂停（系统限制）。

### 测试

```bash
cd app/src-tauri
cargo test                    # 单元测试
FEILIU_NET_TEST=1 cargo test # 追加真实网络下载/合并测试
```

## 🔄 自动更新（维护者）

应用会检查 `https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/latest.json`。

发布带签名的更新包需要在仓库 Secrets 中配置：

1. `TAURI_SIGNING_PRIVATE_KEY` — minisign 私钥内容（`pnpm tauri signer generate` 生成）
2. `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — 私钥密码（无密码则为空字符串）

两个 Secret 都存在时，CI 会自动追加 `--config '{"bundle":{"createUpdaterArtifacts":true}}'`，签名更新包并在 GitHub Release 中生成 `latest.json`。

## 🌐 官网

`website/` 为纯静态站点，内置语言切换（English / 简体中文 / 日本語 / Deutsch）。推送到 `main` 后由 `.github/workflows/deploy-website.yml` 自动部署到 GitHub Pages。

## 开源协议

MIT
