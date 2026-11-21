<div align="center">

# ⚡ FeiLiu Downloader · 飞流下载

**Ein blitzschneller Multi-Thread-Downloader — bis zu 64 parallele Verbindungen, vollständige Resume-Unterstützung.**

[English](../README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | [Deutsch](README.de.md)

[![Tauri 2](https://img.shields.io/badge/Tauri-2-blue)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-stable-orange)](https://rust-lang.org)
[![React 19](https://img.shields.io/badge/React-19-61dafb)](https://react.dev)
[![License](https://img.shields.io/badge/License-MIT-green)](#lizenz)

**[🌐 Offizielle Website](https://ricky-chenzhuquan.github.io/feiliuDownload/)** — Plattform wählen und herunterladen.

</div>

---

## 🌍 Unterstützte Seiten

| Typ | Seiten | Parsing | Download |
|---|---|---|---|
| Video | Bilibili (inkl. Sammlungs-/Serien-Stapel) · Douyin (ohne WM) · Kuaishou · Xiaohongshu · Weibo · allgemein | ✅ Direktlink | ✅ Multi-Thread |
| Musik | NetEase Cloud Music | ✅ Song/Playlist | ✅ freie Songs (VIP → Hinweis), Playlist-Stapel |
| Musik | 82DJ | ✅ Songseite | ✅ WebView-Auflösung + Referer |
| Video | Twitter / X Video-Tweets | ✅ Direktlink (höchste Bitrate) | ✅ Multi-Thread |
| Dev | GitHub Releases | ✅ Seite→Assets | ✅ Einzel-Asset direkt (anonyme API) |
| Archiv | Internet Archive Item-Seiten | ✅ direkter Link (öffentlich, kein Login) | ✅ Multi-Thread |
| Musik | Bandcamp Album-/Track-Seiten | ✅ direkter Link (mp3-Stream) | ✅ Multi-Thread |
| Bild | Pixiv Werkseiten | ✅ direkter Link (Original / Ugoira-Frame-ZIP) | ✅ Multi-Thread |
| Cloud | GoFile gofile.io/d/-Shares | ✅ direkter Link (WebView, Sitzungs-Cookie) | ✅ Multi-Thread + Ordner-Stapel |
| Video | TikTok Videoseiten (inkl. vt/vm-Kurzlinks) | ✅ direkter Link (ohne WM) | ✅ Multi-Thread |
| Cloud | UC Netdisk drive.uc.cn / fast.uc.cn Shares | ✅ direkter Link (Sitzungs-Cookie automatisch) | ✅ Multi-Thread + Ordner-Stapel |
| Cloud | Wenshushu wenshushu.cn / c.wss.ink Shares | ✅ direkter Link (anonyme Token-Kette) | ✅ Multi-Thread |
| Cloud | Feijipan share.feijipan.com / feijix.com Shares | ✅ direkter Link (AES-Signatur nativ) | ✅ Multi-Thread |
| Cloud | CTFile ctfile.com Shares | ✅ direkter Link (Passcode; In-App-WebView löst CAPTCHA) | ✅ Multi-Thread |
| Torrent | Magnet / .torrent | ✅ | ✅ BT-Engine (DHT) |
| Cloud | Lanzou Cloud (Einzeldatei direkt; Ordner-Stapel) | ✅ Direktlink | ✅ In-App-WebView-Assistenz |
| Cloud | Aliyun Drive · Baidu Netdisk · CSDN | ✅ Struktur | ⚠️ Plattform-Sperren |

## ✨ Funktionen
## ✨ Funktionen

- 🚀 **Multi-Connection-Beschleunigung** — Dateien werden in 8-MiB-Chunks aufgeteilt und mit bis zu **64 parallelen Verbindungen** geladen. Dramatisch schneller bei gedrosselten Servern.
- 🔀 **Multi-Source-Aggregation** — zusätzliche Mirror-URLs für dieselbe Datei einfügen; Chunks rotieren über CDNs und die Durchsätze addieren sich.
- 📺 **Site-Extraktion (21+ Plattformen)** — Bilibili (Video / Sammlung / Serie / Bangumi mit Stapel-Download), Douyin (ohne WM), Kuaishou, Xiaohongshu, Weibo, Twitter/X, NetEase Cloud Music (freie Songs direkt + Playlist-Stapel), 82DJ, Lanzou Cloud (Ordner-Stapel), GitHub Releases, Internet Archive (Public-Domain-Medien), Bandcamp (Album-Stapel-Download), Pixiv (Einzeln / Multi-Page p0 / Ugoira-Frame-ZIP), GoFile (gofile.io/d/-Shares — direkt + Ordner-Stapel, Sitzungs-Cookie automatisch), TikTok (Videoseiten, ohne WM), UC Netdisk (drive.uc.cn / fast.uc.cn Shares — anonyme API-Kette mit Sitzungs-Cookie + Ordner-Stapel), Wenshushu (wenshushu.cn / c.wss.ink Shares — anonyme Token-Kette), Feijipan (share.feijipan.com / feijix.com Shares — AES-Signatur nativ + Ordner-Stapel), CTFile (ctfile.com Shares — Passcode, In-App-WebView löst CAPTCHA); Cloud-Drive-/CSDN-Shares werden ehrlich aufgelistet.
- 🧲 **BitTorrent** — Magnet-Links & .torrent-Dateien dank librqbit (DHT + Tracker).
- ⏸️ **Pause / Fortsetzen / Neustart-sicher** — der Chunk-Fortschritt liegt in Echtzeit auf der Festplatte; unterbrochene Downloads setzen exakt am letzten Byte fort. Optionales **Auto-Resume beim Start**.
- 🔍 **Intelligente Erkennung** — Dateiname, Größe und Range-Unterstützung werden automatisch erkannt; Server ohne Byte-Range-Support weichen elegant auf eine einzelne Verbindung aus.
- 🗂️ **Task-Verwaltung** — parallele Aufgaben mit Live-Geschwindigkeit, Restzeit und Verbindungsanzahl. Alles pausieren / alles fortsetzen mit einem Klick.
- 📋 **Schnelles Hinzufügen** — Clipboard-Überwachung und Drag & Drop von Links ins Fenster füllen den Dialog automatisch vor.
- 🖱️ **Verwaltung Plus** — Kontextmenü (Kopieren/Ordner anzeigen), aufklappbare Task-Details (inkl. Original-Share-Link bei analysierten Aufgaben), Dateinamen-Filter, „Fertige löschen", **lebenslange Statistik** (übersteht Löschen) und **anpassbares Stapel-Limit** (Standard 30).
- 🧬 **Cross-Form-Duplikaterkennung** — dasselbe Video als Kurzlink (b23.tv / v.douyin / xhslink / vt.tiktok), volle URL oder Share-Text wird als ein Eintrag erkannt; Bezahl-/nicht verfügbare Inhalte erhalten ehrliche Hinweise.
- 🔄 **In-App-Auto-Update** — Update-Manifeste werden über GitHub Releases veröffentlicht; Update-Pakete werden vor der Installation per Minisign verifiziert.
- 🛡️ **Sauber & privat** — keine Werbung, kein Bundleware, keine Telemetrie. Eine lokale Rust-Engine.
- 🪶 **Federleicht** — auf Tauri 2 basis: ein ~4-MB-Installer.

## 📥 Download

| Plattform | Link | Status |
|---|---|---|
| macOS (Apple Silicon) | [FeiLiu_0.1.1_aarch64.dmg](https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/FeiLiu_0.1.1_aarch64.dmg) | ✅ |
| Windows 10/11 (64-Bit) | [FeiLiu_0.1.1_x64-setup.exe](https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/FeiLiu_0.1.1_x64-setup.exe) | ✅ via CI |
| iOS | [FeiLiu_0.1.0_ios.ipa](https://raw.githubusercontent.com/ricky-chenzhuquan/feiliuDownload/main/website/downloads/FeiLiu_0.1.0_ios.ipa) · Entwickler-signiert | ⚠️ nur registrierte Geräte · TestFlight 🔜 |
| Android | [FeiLiu_0.1.0_aarch64.apk](https://raw.githubusercontent.com/ricky-chenzhuquan/feiliuDownload/main/website/downloads/FeiLiu_0.1.0_aarch64.apk) · Selbst-signiert | ✅ arm64-v8a |

👉 Alle Releases: [github.com/ricky-chenzhuquan/feiliuDownload/releases](https://github.com/ricky-chenzhuquan/feiliuDownload/releases)

## 🏗️ Projektstruktur

```
FeiLiu/
├── app/                        # Tauri-2-App (Desktop + iOS)
│   ├── src/                    # React + TypeScript Frontend
│   └── src-tauri/
│       └── src/downloader/     # Rust-Download-Engine
│           ├── probe.rs        # URL-Erkennung (Größe / Ranges / Dateiname)
│           ├── engine.rs       # Chunk-Engine (parallele Queue / Resume)
│           └── state.rs        # Chunk-Persistenz
├── website/                    # Offizielle Website (statisch, EN/zh/ja/de)
├── docs/                       # Dokumentation in 简体中文 / 日本語 / Deutsch
├── scripts/                    # Icon- & Tooling-Skripte
└── .github/workflows/          # CI: Desktop-Builds, Website-Deploy
```

## 🛠️ Aus dem Quellcode bauen

### Desktop (macOS / Windows / Linux)

```bash
cd app
pnpm install
pnpm tauri dev        # Entwicklungsmodus
pnpm tauri build      # erzeugt .dmg / .msi / .exe
```

### iOS

Voraussetzungen: Xcode, Rust, dazu `xcodegen`, `cocoapods` (`gem install cocoapods --user-install`) und `libimobiledevice` (`brew install libimobiledevice`).

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cd app
pnpm tauri ios init            # nur beim ersten Mal
pnpm tauri ios build --target aarch64-sim
```

> iOS-Einschränkung: Downloads pausieren, wenn die App in den Hintergrund wechselt (Systembeschränkung).

### Tests

```bash
cd app/src-tauri
cargo test                    # Unit-Tests
FEILIU_NET_TEST=1 cargo test -- --test-threads=1  # + echte Netzwerk-Tests

# Die Netzwerk-Tests treffen die echten CDNs der Seiten — immer seriell
# ausführen, sonst lösen parallele Requests Drosselungen aus.
```

## 🔄 Auto-Update (für Maintainer)

Die App prüft `https://github.com/ricky-chenzhuquan/feiliuDownload/releases/latest/download/latest.json`.

Um signierte Update-Artefakte zu veröffentlichen, zwei Repository-Secrets anlegen:

1. `TAURI_SIGNING_PRIVATE_KEY` — Inhalt des Minisign-Private-Keys (erzeugt mit `pnpm tauri signer generate`)
2. `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — Passwort des Keys (leere Zeichenkette, falls keines)

Wenn beide Secrets vorhanden sind, übergibt der CI-Build automatisch `--config '{"bundle":{"createUpdaterArtifacts":true}}'`, signiert die Update-Pakete und erzeugt `latest.json` im GitHub Release.

## 🌐 Website

`website/` ist eine komplett statische Website mit Sprachumschalter (English / 简体中文 / 日本語 / Deutsch). Sie wird bei jedem Push auf `main` automatisch über `.github/workflows/deploy-website.yml` auf GitHub Pages bereitgestellt.

## Lizenz

MIT
