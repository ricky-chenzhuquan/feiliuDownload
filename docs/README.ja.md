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

## 🌍 対応サイト

| 種類 | サイト | 解析 | DL |
|---|---|---|---|
| 動画 | ビリビリ（コレクション/シリーズ一括）· Douyin（WMなし）· 快手 · 小紅書 · Weibo · 汎用 | ✅ 直リンク | ✅ マルチスレッド |
| 音楽 | 網易雲音楽 | ✅ 曲/プレイリスト | ✅ 無料曲（VIP は案内）、一括追加 |
| 音楽 | 82DJ | ✅ 曲ページ | ✅ WebView 解決 + Referer |
| 動画 | Twitter / X 動画ツイート | ✅ 直リンク（最高ビットレート） | ✅ マルチスレッド |
| 開発 | GitHub Releases | ✅ ページ→資産 | ✅ 単一資産直接 DL（匿名 API） |
| 公有領域 | Internet Archive アイテムページ | ✅ 直リンク（公開・ログイン不要） | ✅ マルチスレッド |
| 音楽 | Bandcamp アルバム/トラックページ | ✅ 直リンク（mp3 ストリーム） | ✅ マルチスレッド |
| 画像 | Pixiv 作品ページ | ✅ 直リンク（原画 / うごイラframe ZIP） | ✅ マルチスレッド |
| クラウド | GoFile gofile.io/d/ 共有 | ✅ 直リンク（WebView + セッションcookie） | ✅ マルチスレッド + フォルダ一括 |
| 動画 | TikTok 動画ページ（vt/vm 短縮リンク対応） | ✅ 直リンク（WMなし） | ✅ マルチスレッド |
| クラウド | UCクラウド drive.uc.cn / fast.uc.cn 共有 | ✅ 直リンク（セッション cookie 自動） | ✅ マルチスレッド + フォルダ一括 |
| クラウド | Wenshushu wenshushu.cn / c.wss.ink 共有 | ✅ 直リンク（匿名 token チェーン） | ✅ マルチスレッド |
| クラウド | Feijipan share.feijipan.com / feijix.com 共有 | ✅ 直リンク（AES 署名ネイティブ） | ✅ マルチスレッド |
| クラウド | CTFile ctfile.com 共有 | ✅ 直リンク（パスコード対応；アプリ内 WebView が CAPTCHA 処理） | ✅ マルチスレッド + フォルダ一括 |
| BT | マグネット / .torrent | ✅ | ✅ BT エンジン（DHT） |
| クラウド | 蓝奏云（単一ファイル直リンク；フォルダ一括） | ✅ 直リンク | ✅ アプリ内 WebView 代取 |
| クラウド | Aliyun Drive · 百度網盤 · CSDN | ✅ 一覧 | ⚠️ プラットフォーム制限 |

## ✨ 機能
## ✨ 機能

- 🚀 **マルチ接続アクセラレーション** — ファイルを 8MiB のチャンクに分割し、最大 **64 接続で並列ダウンロード**。速度制限のあるサーバーで劇的に高速化。
- 🔀 **マルチソース集約** — 同一ファイルのミラー URL を追加し、チャンクが複数 CDN を巡回して速度を積み上げます。
- 📺 **サイト解析（21+ プラットフォーム）** — ビリビリ（動画 / コレクション / シリーズ / アニメ一括）、Douyin（WMなし）、快手、小紅書、Weibo、Twitter/X、網易雲音楽（無料曲直リンク + プレイリスト一括）、82DJ、蓝奏云（フォルダ一括）、GitHub Releases、Internet Archive（パブリックドメイン音動画）、Bandcamp（アルバム一括）、Pixiv（単発 / 漫画p0 / うごイラフレームZIP）、GoFile（gofile.io/d/ 共有直 DL + フォルダ一括、セッション cookie 自動）、TikTok（動画ページ直 DL、WMなし）、UCクラウド（drive.uc.cn / fast.uc.cn 共有——匿名 API 全链 + セッション cookie + フォルダ一括）、Wenshushu（wenshushu.cn / c.wss.ink 共有——匿名 token チェーン）、Feijipan（share.feijipan.com / feijix.com 共有——AES 署名ネイティブ + フォルダ一括）、CTFile（ctfile.com 共有——パスコード対応、アプリ内 WebView が CAPTCHA 処理）；クラウド/CSDN 共有は正直な案内で一覧表示。
- 🧲 **BitTorrent** — マグネット＆.torrent に対応（librqbit エンジン、DHT + Tracker）。
- ⏸️ **一時停止 / 再開 / 再起動対応** — チャンクの進捗はリアルタイムでディスクに保存され、中断したバイト位置から正確に再開。**起動時の自動復帰**にも対応。
- 🔍 **スマート プローブ** — ファイル名・サイズ・レンジ対応を自動判定。非対応サーバーは自動的に単一接続へフォールバック。
- 🗂️ **マルチタスク管理** — 並列タスクの速度 / 残り時間 / 接続数をリアルタイム表示。全て一時停止 / 全て再開もワンクリック。
- 📋 **クイック追加** — クリップボード監視とウィンドウへのリンク ドラッグ＆ドロップで新規タスクに自動入力。
- 🖱️ **管理機能強化** — 右クリックメニュー（コピー/フォルダ表示）、タスク詳細展開（解析タスクの元リンク表示）、ファイル名検索フィルタ、完了済み一括削除、**クリア後も残る累計統計**、**一括上限は設定可**（既定 30）。
- 🧬 **クロス形式重複検知** — 同一動画の短縮リンク（b23.tv / v.douyin / xhslink / vt.tiktok）/完全 URL/共有テキストを同一項目として認識；有料・不可アクセス内容は正直な通知を表示。
- 🔄 **アプリ内自動アップデート** — 更新マニフェストは GitHub Releases で公開。更新パッケージは minisign 署名で検証してからインストール。
- 🛡️ **クリーン＆プライベート** — 広告なし、バンドルなし、テレメトリーなし。ローカルで動く Rust エンジン。
- 🪶 **超軽量** — Tauri 2 ベース、インストーラは約 4MB。

## 📥 ダウンロード

| プラットフォーム | リンク | 状態 |
|---|---|---|
| macOS（Apple Silicon） | [FeiLiu_0.1.1_aarch64.dmg](https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/FeiLiu_0.1.1_aarch64.dmg) | ✅ |
| Windows 10/11（64 ビット） | [FeiLiu_0.1.1_x64-setup.exe](https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/FeiLiu_0.1.1_x64-setup.exe) | ✅ CI が自動ビルド |
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
FEILIU_NET_TEST=1 cargo test -- --test-threads=1  # 実ネットワークテストを追加

# 実ネットワークテストは各サイトの本番 CDN にアクセスします。
# 並列実行だとレート制限に引っかかるため必ず直列で実行してください。
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
