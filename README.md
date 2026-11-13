<div align="center">

# ⚡ FeiLiu Downloader · 飞流下载

**A blazing-fast multi-threaded downloader — up to 64 parallel connections, full resume support.**

[English](README.md) | [简体中文](docs/README.zh-CN.md) | [日本語](docs/README.ja.md) | [Deutsch](docs/README.de.md)

[![Tauri 2](https://img.shields.io/badge/Tauri-2-blue)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-stable-orange)](https://rust-lang.org)
[![React 19](https://img.shields.io/badge/React-19-61dafb)](https://react.dev)
[![License](https://img.shields.io/badge/License-MIT-green)](#license)

**[🌐 Official Website](https://ricky-chenzhuquan.github.io/feiliuDownload/)** — pick your platform and download.

</div>

---

## ✨ Features

- 🚀 **Multi-connection acceleration** — files are split into 8 MiB chunks and downloaded with up to **64 parallel connections**. Dramatic speedups on throttled servers.
- ⏸️ **Pause / resume / restart-safe** — chunk progress lives on disk in real time; interrupted downloads resume from the exact byte. Optional **auto-resume on launch**.
- 🔍 **Smart probing** — filename, size and range-support are detected automatically; servers without byte-range support fall back to a single connection gracefully.
- 🗂️ **Multi-task manager** — parallel tasks with live speed, ETA and connection count. Pause-all / resume-all in one click.
- 🔄 **In-app auto-update** — update manifests are published via GitHub Releases; update packages are minisign-verified before install.
- 🛡️ **Clean & private** — no ads, no bundleware, no telemetry. A local Rust engine.
- 🪶 **Feather-light** — built on Tauri 2: a ~4 MB installer.

## 📥 Download

| Platform | Link | Status |
|---|---|---|
| macOS (Apple Silicon) | [FeiLiu_0.1.0_aarch64.dmg](https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/FeiLiu_0.1.0_aarch64.dmg) | ✅ |
| Windows 10/11 (64-bit) | [FeiLiu_0.1.0_x64-setup.exe](https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/FeiLiu_0.1.0_x64-setup.exe) | ✅ via CI |
| iOS | [FeiLiu_0.1.0_ios.ipa](https://raw.githubusercontent.com/ricky-chenzhuquan/feiliuDownload/main/website/downloads/FeiLiu_0.1.0_ios.ipa) · development-signed | ⚠️ registered devices only · TestFlight 🔜 |
| Android | [FeiLiu_0.1.0_aarch64.apk](https://raw.githubusercontent.com/ricky-chenzhuquan/feiliuDownload/main/website/downloads/FeiLiu_0.1.0_aarch64.apk) · self-signed | ✅ arm64-v8a |

👉 All releases: [github.com/ricky-chenzhuquan/feiliuDownload/releases](https://github.com/ricky-chenzhuquan/feiliuDownload/releases)

## 🏗️ Project Structure

```
FeiLiu/
├── app/                        # Tauri 2 app (desktop + iOS)
│   ├── src/                    # React + TypeScript frontend
│   └── src-tauri/
│       └── src/downloader/     # Rust download engine
│           ├── probe.rs        # URL probing (size / ranges / filename)
│           ├── engine.rs       # Chunked engine (parallel queue / resume)
│           └── state.rs        # Chunk persistence
├── website/                    # Official website (static, EN/zh/ja/de)
├── docs/                       # READMEs in 简体中文 / 日本語 / Deutsch
├── scripts/                    # Icon & tooling scripts
└── .github/workflows/          # CI: desktop builds, website deploy
```

## 🛠️ Build from Source

### Desktop (macOS / Windows / Linux)

```bash
cd app
pnpm install
pnpm tauri dev        # development
pnpm tauri build      # .dmg / .msi / .exe
```

### iOS

Prerequisites: Xcode, Rust, plus `xcodegen`, `cocoapods` (`gem install cocoapods --user-install`) and `libimobiledevice` (`brew install libimobiledevice`).

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cd app
pnpm tauri ios init            # first time only
pnpm tauri ios build --target aarch64-sim
```

> iOS limitation: downloads pause when the app goes to background (system constraint).

### Tests

```bash
cd app/src-tauri
cargo test                    # unit tests
FEILIU_NET_TEST=1 cargo test # + real-network download/merge test
```

## 🔄 Auto-update (for maintainers)

The app checks `https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/latest.json`.

To publish signed update artifacts, add two repository secrets:

1. `TAURI_SIGNING_PRIVATE_KEY` — content of the minisign private key (generated with `pnpm tauri signer generate`)
2. `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — key password (empty string if none)

When both secrets exist, the CI build passes `--config '{"bundle":{"createUpdaterArtifacts":true}}'`, signs the update packages, and generates `latest.json` automatically in the GitHub Release.

## 🌐 Website

`website/` is a fully static site with a language switcher (English / 简体中文 / 日本語 / Deutsch). It deploys automatically to GitHub Pages via `.github/workflows/deploy-website.yml` on every push to `main`.

## License

MIT
