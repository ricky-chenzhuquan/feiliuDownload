# 飞流下载 · FeiLiu Downloader

多线程高速下载器 — 基于 Tauri 2，一套代码产出 **macOS / Windows 桌面版** 和 **iOS 版**。

![技术栈](https://img.shields.io/badge/Tauri-2-blue) ![Rust](https://img.shields.io/badge/Rust-stable-orange) ![React](https://img.shields.io/badge/React-19-61dafb)

## 特性

- ⚡ **多连接分片下载**：每任务最高 64 连接，8MiB 动态分片队列，快连接自动多拿任务
- ⏸️ **暂停 / 继续 / 断点续传**：分片文件即进度状态，App 重启后可从断点恢复
- 🔍 **智能探测**：自动识别文件名、大小、是否支持断点；不支持的服务器自动降级单连接
- 📦 **多任务并发**：同时下载多个文件，实时速度 / ETA / 连接数展示
- 🌐 **完整官网**：`website/` 目录，支持 GitHub Pages 一键部署

## 项目结构

```
FeiLiu/
├── app/                  # Tauri 2 应用（桌面 + iOS）
│   ├── src/              # React + TypeScript 前端
│   └── src-tauri/
│       └── src/downloader/   # Rust 多连接下载引擎
│           ├── probe.rs      # URL 探测（大小/断点/文件名）
│           ├── engine.rs     # 核心引擎（分片队列/并发/续传）
│           └── state.rs      # 分片持久化
├── website/              # 官网（纯静态，GitHub Pages 可用）
├── scripts/              # 图标生成等工具脚本
└── .github/workflows/    # CI：自动构建 Windows / macOS 产物
```

## 本地开发

### 桌面版（macOS / Windows / Linux）

```bash
cd app
pnpm install
pnpm tauri dev        # 开发模式
pnpm tauri build      # 产出 .dmg / .msi / .exe
```

产物在 `app/src-tauri/target/release/bundle/`。

### iOS 版

前置：Xcode + Rust。首次需要添加 iOS 编译目标：

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cd app
pnpm tauri ios init   # 已初始化过则跳过
pnpm tauri ios dev    # 真机/模拟器开发（需签名）
pnpm tauri ios build --target aarch64-sim   # 模拟器包，产物在 gen/apple/build/arm64-sim/
```

`tauri ios init` 还需要这些系统工具（本机缺失时安装）：

- `xcodegen`：生成 Xcode 工程
- `cocoapods`：`gem install cocoapods --user-install`
- `libimobiledevice`：`brew install libimobiledevice`（真机部署检测用）

> iOS 限制：应用退到后台后下载会暂停（系统限制），需保持前台运行。

### 运行测试

```bash
cd app/src-tauri
cargo test                        # 纯单元测试
FEILIU_NET_TEST=1 cargo test     # 额外跑真实网络下载链路测试
```

## 官网部署（GitHub Pages）

`website/` 是纯静态站点。推到 GitHub 仓库后：

1. 仓库 Settings → Pages → Source 选 `main` 分支 `/website` 目录
2. 或直接用仓库根的 `.github/workflows/deploy-website.yml` 自动部署

## License

MIT
