<div align="center">

# ⚡ FeiLiu Downloader · 飛流ダウンロード

**超高速マルチスレッドダウンローダー — 最大 64 並列接続、完全なレジューム対応。**

[English](../README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | [Deutsch](README.de.md)

[![Tauri 2](https://img.shields.io/badge/Tauri-2-blue)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-stable-orange)](https://rust-lang.org)
[![React 19](https://img.shields.io/badge/React-19-61dafb)](https://react.dev)
[![License](https://img.shields.io/badge/License-MIT-green)](#ライセンス)

**[🌐 公式ウェブサイト](https://ricky-chenzhuquan.github.io/feiliuDownload/)** — プラットフォームを選んでダウンロード。

</div>

---

## ✨ 機能

- 🚀 **マルチ接続アクセラレーション** — ファイルを 8MiB のチャンクに分割し、最大 **64 接続で並列ダウンロード**。速度制限のあるサーバーで劇的に高速化。
- 🔀 **マルチソース集約** — 同一ファイルのミラー URL を追加し、チャンクが複数 CDN を巡回して速度を積み上げます。
- ⏸️ **一時停止 / 再開 / 再起動対応** — チャンクの進捗はリアルタイムでディスクに保存され、中断したバイト位置から正確に再開。**起動時の自動復帰**にも対応。
- 🔍 **スマート プローブ** — ファイル名・サイズ・レンジ対応を自動判定。非対応サーバーは自動的に単一接続へフォールバック。
- 🗂️ **マルチタスク管理** — 並列タスクの速度 / 残り時間 / 接続数をリアルタイム表示。全て一時停止 / 全て再開もワンクリック。
- 🔄 **アプリ内自動アップデート** — 更新マニフェストは GitHub Releases で公開。更新パッケージは minisign 署名で検証してからインストール。
- 🛡️ **クリーン＆プライベート** — 広告なし、バンドルなし、テレメトリーなし。ローカルで動く Rust エンジン。
- 🪶 **超軽量** — Tauri 2 ベース、インストーラは約 4MB。

## 📥 ダウンロード

| プラットフォーム | リンク | 状態 |
|---|---|---|
| macOS（Apple Silicon） | [FeiLiu_0.1.0_aarch64.dmg](https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/FeiLiu_0.1.0_aarch64.dmg) | ✅ |
| Windows 10/11（64 ビット） | [FeiLiu_0.1.0_x64-setup.exe](https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/FeiLiu_0.1.0_x64-setup.exe) | ✅ CI が自動ビルド |
| iOS | [FeiLiu_0.1.0_ios.ipa](https://raw.githubusercontent.com/ricky-chenzhuquan/feiliuDownload/main/website/downloads/FeiLiu_0.1.0_ios.ipa) · 開発署名 | ⚠️ 登録済みデバイスのみ · TestFlight 🔜 |
| Android | [FeiLiu_0.1.0_aarch64.apk](https://raw.githubusercontent.com/ricky-chenzhuquan/feiliuDownload/main/website/downloads/FeiLiu_0.1.0_aarch64.apk) · 自己署名 | ✅ arm64-v8a |

👉 すべてのリリース：[github.com/ricky-chenzhuquan/feiliuDownload/releases](https://github.com/ricky-chenzhuquan/feiliuDownload/releases)

## 🏗️ プロジェクト構成

```
FeiLiu/
├── app/                        # Tauri 2 アプリ（デスクトップ + iOS）
│   ├── src/                    # React + TypeScript フロントエンド
│   └── src-tauri/
│       └── src/downloader/     # Rust ダウンロードエンジン
│           ├── probe.rs        # URL プローブ（サイズ / レンジ / ファイル名）
│           ├── engine.rs       # チャンクエンジン（並列キュー / レジューム）
│           └── state.rs        # チャンク永続化
├── website/                    # 公式サイト（静的、EN/zh/ja/de）
├── docs/                       # 多言語ドキュメント
├── scripts/                    # アイコン・ツールスクリプト
└── .github/workflows/          # CI：デスクトップビルド、サイトデプロイ
```

## 🛠️ ソースからビルド

### デスクトップ（macOS / Windows / Linux）

```bash
cd app
pnpm install
pnpm tauri dev        # 開発モード
pnpm tauri build      # .dmg / .msi / .exe を生成
```

### iOS

前提：Xcode、Rust、加えて `xcodegen`、`cocoapods`（`gem install cocoapods --user-install`）、`libimobiledevice`（`brew install libimobiledevice`）。

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cd app
pnpm tauri ios init            # 初回のみ
pnpm tauri ios build --target aarch64-sim
```

> iOS の制限：アプリがバックグラウンドに移るとダウンロードは一時停止します（OS の仕様）。

### テスト

```bash
cd app/src-tauri
cargo test                    # 単体テスト
FEILIU_NET_TEST=1 cargo test # 実ネットワークのダウンロード/マージテストを追加
```

## 🔄 自動アップデート（メンテナー向け）

アプリは `https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/latest.json` をチェックします。

署名付き更新アーティファクトを公開するには、リポジトリの Secrets に 2 つ追加してください：

1. `TAURI_SIGNING_PRIVATE_KEY` — minisign 秘密鍵の内容（`pnpm tauri signer generate` で生成）
2. `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — 秘密鍵のパスワード（未設定なら空文字列）

両方の Secret が存在する場合、CI は自動的に `--config '{"bundle":{"createUpdaterArtifacts":true}}'` を追加し、更新パッケージに署名して GitHub Release に `latest.json` を生成します。

## 🌐 ウェブサイト

`website/` は言語切り替え付き（English / 简体中文 / 日本語 / Deutsch）の完全な静的サイトです。`main` へのプッシュごとに `.github/workflows/deploy-website.yml` が GitHub Pages へ自動デプロイします。

## ライセンス

MIT
