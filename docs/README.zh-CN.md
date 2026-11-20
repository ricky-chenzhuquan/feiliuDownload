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

## 🌍 支持的站点

| 类型 | 站点 | 解析 | 下载 |
|---|---|---|---|
| 视频 | B 站（合集/系列一键批量）· 抖音（去水印）· 快手 · 小红书 · 微博 · 通用网页 | ✅ 直链 | ✅ 多线程 |
| 音乐 | 网易云音乐 | ✅ 单曲/歌单 | ✅ 免费歌直链（VIP 提示），歌单一键批量 |
| 音乐 | 82DJ 舞曲 | ✅ 歌曲页 | ✅ WebView 解析音频流 + Referer |
| 视频 | Twitter / X 视频推文 | ✅ 直链（自动选最高码率） | ✅ 多线程 |
| 开发 | GitHub Releases | ✅ 页面→资产 | ✅ 单资产直下（匿名 API） |
| 公有领域 | Internet Archive 条目页 | ✅ 直链（公开无需登录） | ✅ 多线程 |
| 音乐 | Bandcamp 专辑/曲目页 | ✅ 直链（mp3 流） | ✅ 多线程 |
| 种子 | 磁力 / .torrent | ✅ | ✅ BT 引擎（DHT） |
| 网盘 | 蓝奏云（单文件直链；文件夹一键批量） | ✅ 直链 | ✅ 内嵌 WebView 代取 |
| 网盘 | 阿里云盘 · 百度网盘 · CSDN | ✅ 结构列出 | ⚠️ 平台限制（登录墙/指纹墙） |

## ✨ 功能特性
## ✨ 功能特性

- 🚀 **多连接加速** — 文件自动切分为 8MiB 分片，最高 **64 连接并行**下载；对限速服务器效果拔群。
- 🔀 **多源聚合加速** — 为同一文件粘贴多个镜像线路，分片在不同 CDN 间轮询，多路带宽叠加。
- 📺 **站点解析（17+ 平台）** — B 站（视频 / 合集 / 系列 / 番剧一键批量）、抖音（去水印）、快手、小红书、微博、Twitter/X、网易云（免费歌直链 + 歌单批量）、82DJ、蓝奏云（文件夹批量）、GitHub Releases、Internet Archive（公有领域音视频）、Bandcamp（专辑一键批量）、Pixiv（单图 / 多图第1页 / 动图帧zip）、GoFile（gofile.io/d/ 分享直下，会话 cookie 自动透传）；网盘/CSDN 分享诚实引导结构列出。
- 🧲 **BT 下载** — 支持磁力链与 .torrent 种子（librqbit 引擎，DHT + Tracker）。
- ⏸️ **暂停 / 继续 / 重启无忧** — 分片进度实时落盘，中断后从精确字节恢复；支持**启动时自动恢复**。
- 🔍 **智能探测** — 自动识别文件名、大小、是否支持断点；不支持的服务器自动降级单连接。
- 🗂️ **多任务管理** — 并行任务，实时速度 / 剩余时间 / 连接数；一键全部暂停 / 全部继续。
- 📋 **快捷添加** — 剪贴板监听与拖拽链接到窗口，自动预填新建任务。
- 🖱️ **任务管理增强** — 右键菜单（复制链接/文件名/打开所在文件夹）、任务详情展开（含解析类任务的原始分享链接）、文件名搜索过滤、清除已完成、**跨清除持久的累计统计**、**批量上限可调**（默认 30）。
- 🧬 **跨形态查重** — 同一视频以短链（b23.tv / v.douyin / xhslink）、完整链接或分享文本粘贴均识别为同一条；付费/不可访问内容给出诚实提示而非假任务。
- 🔄 **应用内自动更新** — 更新清单通过 GitHub Releases 发布，更新包经 minisign 签名校验。
- 🛡️ **纯净无广告** — 无广告、无捆绑、无数据收集，Rust 引擎本地运行。
- 🪶 **轻量极速** — 基于 Tauri 2，安装包仅约 4MB。

## 📥 下载

| 平台 | 链接 | 状态 |
|---|---|---|
| macOS（Apple Silicon） | [FeiLiu_0.1.1_aarch64.dmg](https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/FeiLiu_0.1.1_aarch64.dmg) | ✅ |
| Windows 10/11（64 位） | [FeiLiu_0.1.1_x64-setup.exe](https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/FeiLiu_0.1.1_x64-setup.exe) | ✅ CI 自动构建 |
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
FEILIU_NET_TEST=1 cargo test -- --test-threads=1  # 追加真实网络测试

# 真实测试直接打各站点线上 CDN，务必串行执行——并行会触发
# 各站点的风控限流，导致偶发失败
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
