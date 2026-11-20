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

## 🌍 Supported Sites

| Type | Sites | Parse | Download |
|---|---|---|---|
| Video | Bilibili (incl. one-click collection/series batch) · Douyin (watermark-free) · Kuaishou · Xiaohongshu · Weibo · generic pages | ✅ direct link | ✅ multi-thread |
| Music | NetEase Cloud Music | ✅ song/playlist | ✅ free songs (VIP → notice), playlist batch |
| Music | 82DJ | ✅ song page | ✅ WebView-resolved stream + Referer |
| Video | Twitter / X video posts | ✅ direct link (highest bitrate) | ✅ multi-thread |
| Dev | GitHub Releases | ✅ page → assets | ✅ single-asset direct (anonymous API) |
| Archive | Internet Archive item pages | ✅ direct link (public, no login) | ✅ multi-thread |
| Music | Bandcamp album / track pages | ✅ direct link (mp3 stream) | ✅ multi-thread |
| Torrent | magnet / .torrent | ✅ | ✅ BT engine (DHT) |
| Cloud | Lanzou Cloud (single file direct; folder one-click batch) | ✅ direct link | ✅ in-app WebView assist |
| Cloud | Aliyun Drive · Baidu Netdisk · CSDN | ✅ structure | ⚠️ platform walls (account login / anti-bot) |

## ✨ Features
## ✨ Features

- 🚀 **Multi-connection acceleration** — files are split into 8 MiB chunks and downloaded with up to **64 parallel connections**. Dramatic speedups on throttled servers.
- 🔀 **Multi-source (mirror) downloading** — paste extra mirror URLs for the same file; chunks rotate across CDNs so throughput stacks.
- 📺 **Site extraction (18+ platforms)** — Bilibili (video / collection / series / bangumi with one-click batch), Douyin (watermark-free), Kuaishou, Xiaohongshu, Weibo, Twitter/X, NetEase Cloud Music (free songs direct + playlist batch), 82DJ, Lanzou Cloud (folder batch), GitHub Releases, Internet Archive (public-domain media), Bandcamp (albums one-click batch), Pixiv (single image / multi-page p0 / ugoira frame ZIP), GoFile (gofile.io/d/ shares — session cookie handled automatically), TikTok (video pages, watermark-free); cloud-drive/CSDN shares are listed with honest guidance.
- 🧲 **BitTorrent** — magnet links & .torrent files powered by librqbit (DHT + trackers).
- ⏸️ **Pause / resume / restart-safe** — chunk progress lives on disk in real time; interrupted downloads resume from the exact byte. Optional **auto-resume on launch**.
- 🔍 **Smart probing** — filename, size and range-support are detected automatically; servers without byte-range support fall back to a single connection gracefully.
- 🗂️ **Multi-task manager** — parallel tasks with live speed, ETA and connection count. Pause-all / resume-all in one click.
- 📋 **Quick add** — clipboard watch and drag-and-drop links onto the window pre-fill the new-task dialog.
- 🖱️ **Manager extras** — right-click menu (copy link / filename / reveal folder), expandable task details (incl. the original share link for resolved tasks), filename search filter, clear-completed, **lifetime totals** that survive clearing, and **adjustable batch limit** (default 30).
- 🧬 **Cross-form duplicate detection** — the same video pasted as a short link (b23.tv / v.douyin / xhslink), full URL, or share text is recognized as one item; paid/unavailable content gets an honest notice instead of a fake download.
- 🔄 **In-app auto-update** — update manifests are published via GitHub Releases; update packages are minisign-verified before install.
- 🛡️ **Clean & private** — no ads, no bundleware, no telemetry. A local Rust engine.
- 🪶 **Feather-light** — built on Tauri 2: a ~4 MB installer.

## 📥 Download

| Platform | Link | Status |
|---|---|---|
| macOS (Apple Silicon) | [FeiLiu_0.1.1_aarch64.dmg](https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/FeiLiu_0.1.1_aarch64.dmg) | ✅ |
| Windows 10/11 (64-bit) | [FeiLiu_0.1.1_x64-setup.exe](https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/FeiLiu_0.1.1_x64-setup.exe) | ✅ via CI |
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
FEILIU_NET_TEST=1 cargo test -- --test-threads=1  # + real-network tests

# Real-network tests hit live CDNs: run them serially, otherwise parallel
# requests can trip per-site rate limiting and fail intermittently.
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
