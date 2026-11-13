/* 飞流下载官网：四语言 i18n（默认英语）+ 交互 */
(function () {
  "use strict";

  var DICT = {
    en: {
      "nav.features": "Features",
      "nav.download": "Download",
      "nav.faq": "FAQ",
      "nav.cta": "Get the App",
      "hero.badge": "⚡ Up to 64 parallel connections · Full resume support · Free",
      "hero.title": 'The <span class="grad">blazing fast</span> multi-threaded downloader',
      "hero.sub": "Server throttling your downloads? Huge files crawling at a snail's pace? FeiLiu splits files into chunks and downloads them in parallel — saturating your bandwidth, several times faster. For desktop and iOS.",
      "hero.cta1": "Download Free",
      "hero.cta2": "Explore Features",
      "stats.c1": "Max connections",
      "stats.c2": "Resume anywhere",
      "stats.c3": "Platforms",
      "stats.c4": "Ads or bundleware",
      "features.title": "Why FeiLiu Downloader",
      "features.f1.t": "Multi-connection acceleration",
      "features.f1.p": "Files are split into 8 MiB chunks and downloaded with up to 64 parallel connections. Dramatic speedups on throttled servers.",
      "features.f2.t": "Pause & resume",
      "features.f2.p": "Pause anytime and continue later. Interrupted downloads resume automatically on restart — not a byte wasted.",
      "features.f3.t": "Smart probing",
      "features.f3.p": "Paste a link and FeiLiu detects the filename, size and range support, falling back to a single connection when a server doesn't allow ranges.",
      "features.f4.t": "Task manager",
      "features.f4.p": "Download multiple files in parallel, with live speed, ETA and connection count at a glance.",
      "features.f5.t": "Clean & private",
      "features.f5.p": "No ads, no bundleware, no background uploads. A local Rust engine that collects nothing.",
      "features.f6.t": "Feather-light",
      "features.f6.p": "Built on Tauri — a ~4 MB installer that barely touches memory. Old machines welcome.",
      "download.title": "Download FeiLiu",
      "download.sub": "Choose your platform · v0.1.0",
      "dl.mac.name": "macOS",
      "dl.mac.req": "Apple Silicon (M series) · macOS 10.15+",
      "dl.mac.btn": "Download dmg",
      "dl.win.name": "Windows",
      "dl.win.req": "Windows 10/11 · 64-bit",
      "dl.win.btn": "Download installer",
      "dl.win.hint": "Built automatically by GitHub Actions",
      "dl.ios.name": "iOS",
      "dl.ios.req": "iOS 13+ · iPhone / iPad · arm64",
      "dl.ios.btn": "Download IPA",
      "dl.ios.hint": "Development-signed · installs on registered devices · TestFlight planned",
      "dl.ios.alert": "The iOS IPA is development-signed; it installs only on devices registered to the developer team. TestFlight/App Store release is planned.",
      "dl.android.name": "Android",
      "dl.android.req": "Android 8.0+ · arm64-v8a",
      "dl.android.btn": "Download APK",
      "dl.android.hint": "Self-signed APK · enable \"unknown sources\" to install",
      "download.all": "All releases & older versions ↗",
      "faq.title": "FAQ",
      "faq.q1": "Why is the download so much faster?",
      "faq.a1": "Many websites throttle each connection. FeiLiu splits the file into chunks and downloads different parts with up to 64 simultaneous connections, bypassing per-connection limits and saturating your bandwidth.",
      "faq.q2": "Does multi-connection work on every site?",
      "faq.a2": "It depends on whether the server supports byte ranges (Accept-Ranges). FeiLiu probes this automatically for every task and shows it in the UI; unsupported servers gracefully fall back to a single connection.",
      "faq.q3": "Will my download survive an app restart?",
      "faq.a3": "Yes. Every chunk's progress is stored on disk in real time. After restarting, click resume (or enable auto-resume) and it continues from the exact byte where it stopped.",
      "faq.q4": "When will the iOS version be available?",
      "faq.a4": "The iOS app is complete and being prepared for signing and release. It will be available via App Store / TestFlight.",
      "footer.brand": "FeiLiu Downloader",
      "footer.note": "Built with Tauri 2 & Rust · MIT License",
      _title: "FeiLiu Downloader — Multi-threaded High-speed Downloader",
      _lang: "en",
    },

    zh: {
      "nav.features": "功能",
      "nav.download": "下载",
      "nav.faq": "常见问题",
      "nav.cta": "立即下载",
      "hero.badge": "⚡ 最高 64 连接并行 · 断点续传 · 免费",
      "hero.title": '快如<span class="grad">飞流</span>的多线程下载器',
      "hero.sub": "服务器限速？大文件下载龟速？飞流下载用多连接分片技术把带宽吃满，速度提升数倍到数十倍。桌面版与 iOS 版同时支持。",
      "hero.cta1": "免费下载",
      "hero.cta2": "了解功能",
      "stats.c1": "最大连接数",
      "stats.c2": "断点续传",
      "stats.c3": "端同时支持",
      "stats.c4": "广告与捆绑",
      "features.title": "为什么选择飞流下载",
      "features.f1.t": "多连接加速",
      "features.f1.p": "自动将文件切分为 8MiB 分片，最高 64 连接并行下载。针对限速服务器效果拔群。",
      "features.f2.t": "暂停与续传",
      "features.f2.p": "随时暂停、继续；应用重启后自动从断点恢复，不浪费一字节流量。",
      "features.f3.t": "智能探测",
      "features.f3.p": "粘贴链接自动识别文件名与大小，探测服务器是否支持多线程并智能降级。",
      "features.f4.t": "多任务管理",
      "features.f4.p": "多个任务并行下载，实时速度、剩余时间、连接数一目了然。",
      "features.f5.t": "纯净无广告",
      "features.f5.p": "无广告、无捆绑、无后台上传。Rust 引擎本地运行，不收集任何数据。",
      "features.f6.t": "轻量极速",
      "features.f6.p": "Tauri 架构，安装包仅 4MB 左右，内存占用极低，老电脑也流畅。",
      "download.title": "下载飞流",
      "download.sub": "选择你的平台 · v0.1.0",
      "dl.mac.name": "macOS",
      "dl.mac.req": "Apple Silicon (M 系列) · macOS 10.15+",
      "dl.mac.btn": "下载 dmg",
      "dl.win.name": "Windows",
      "dl.win.req": "Windows 10/11 · 64 位",
      "dl.win.btn": "下载安装包",
      "dl.win.hint": "GitHub Actions 自动构建",
      "dl.ios.name": "iOS",
      "dl.ios.req": "iOS 13+ · iPhone / iPad · arm64",
      "dl.ios.btn": "下载 IPA",
      "dl.ios.hint": "开发签名 · 仅可安装到已注册设备 · TestFlight 即将开放",
      "dl.ios.alert": "iOS IPA 为开发签名，仅可安装到开发者团队已注册的设备。TestFlight / App Store 发布正在准备中。",
      "dl.android.name": "Android",
      "dl.android.req": "Android 8.0+ · arm64-v8a",
      "dl.android.btn": "下载 APK",
      "dl.android.hint": "自签名 APK · 安装时需允许「未知来源」",
      "download.all": "全部历史版本 ↗",
      "faq.title": "常见问题",
      "faq.q1": "为什么下载速度能提升这么多？",
      "faq.a1": "很多网站对单个连接限速。飞流下载会把文件切成多个分片，用最多 64 个连接同时下载不同部分，绕过单连接限速，把你的带宽吃满。",
      "faq.q2": "所有网站都支持多线程加速吗？",
      "faq.a2": "取决于服务器是否支持「断点续传」（Accept-Ranges）。新建任务时飞流会自动探测并在界面上提示；不支持的服务器会自动使用单连接，保证兼容性。",
      "faq.q3": "暂停后重启应用，下载会丢吗？",
      "faq.a3": "不会。每个分片的进度都实时保存在磁盘上，重启应用后点击「继续」（或开启自动恢复）即从断点恢复，已下载的部分不会重下。",
      "faq.q4": "iOS 版什么时候可以下载？",
      "faq.a4": "iOS 版应用已开发完成，正在准备签名与上架流程。届时将通过 App Store 或 TestFlight 提供。",
      "footer.brand": "飞流下载 FeiLiu",
      "footer.note": "基于 Tauri 2 与 Rust 构建 · MIT License",
      _title: "飞流下载 FeiLiu — 多线程高速下载器",
      _lang: "zh-CN",
    },

    ja: {
      "nav.features": "機能",
      "nav.download": "ダウンロード",
      "nav.faq": "よくある質問",
      "nav.cta": "アプリを入手",
      "hero.badge": "⚡ 最大 64 並列接続 · レジューム対応 · 無料",
      "hero.title": '<span class="grad">超高速</span>マルチスレッドダウンローダー',
      "hero.sub": "サーバーの速度制限？巨大ファイルで亀 speed？FeiLiu はファイルをチャンクに分割して並列ダウンロードし、帯域を余さず使い切ります。数倍〜数十倍の高速化。デスクトップと iOS に対応。",
      "hero.cta1": "無料でダウンロード",
      "hero.cta2": "機能を見る",
      "stats.c1": "最大接続数",
      "stats.c2": "レジューム",
      "stats.c3": "対応プラットフォーム",
      "stats.c4": "広告・バンドル",
      "features.title": "FeiLiu Downloader が選ばれる理由",
      "features.f1.t": "マルチ接続アクセラレーション",
      "features.f1.p": "ファイルを 8MiB のチャンクに分割し、最大 64 接続で並列ダウンロード。速度制限のあるサーバーで劇的に速くなります。",
      "features.f2.t": "一時停止＆レジューム",
      "features.f2.p": "いつでも一時停止・再開。アプリ再起動後も自動的に中断箇所から復帰し、1 バイトも無駄にしません。",
      "features.f3.t": "スマート プローブ",
      "features.f3.p": "リンクを貼り付けるだけでファイル名・サイズ・範囲リクエスト対応を自動判定。非対応サーバーは自動で単一接続に切り替えます。",
      "features.f4.t": "タスク管理",
      "features.f4.p": "複数ファイルを並列ダウンロード。速度・残り時間・接続数をリアルタイムに確認できます。",
      "features.f5.t": "クリーン＆プライベート",
      "features.f5.p": "広告なし、バンドルなし、バックグラウンドアップロードなし。ローカルで動く Rust エンジンがデータを収集しません。",
      "features.f6.t": "超軽量",
      "features.f6.p": "Tauri 製でインストーラは約 4MB。メモリ消費も極小で、古いマシンでも快適。",
      "download.title": "FeiLiu をダウンロード",
      "download.sub": "プラットフォームを選択 · v0.1.0",
      "dl.mac.name": "macOS",
      "dl.mac.req": "Apple Silicon（M シリーズ）· macOS 10.15+",
      "dl.mac.btn": "dmg をダウンロード",
      "dl.win.name": "Windows",
      "dl.win.req": "Windows 10/11 · 64 ビット",
      "dl.win.btn": "インストーラをダウンロード",
      "dl.win.hint": "GitHub Actions が自動ビルド",
      "dl.ios.name": "iOS",
      "dl.ios.req": "iOS 13+ · iPhone / iPad · arm64",
      "dl.ios.btn": "IPA をダウンロード",
      "dl.ios.hint": "開発署名 · 登録済みデバイスのみインストール可 · TestFlight 予定",
      "dl.ios.alert": "iOS IPA は開発署名のため、開発者チームに登録済みのデバイスにのみインストールできます。TestFlight / App Store を準備中です。",
      "dl.android.name": "Android",
      "dl.android.req": "Android 8.0+ · arm64-v8a",
      "dl.android.btn": "APK をダウンロード",
      "dl.android.hint": "自己署名 APK · 「提供元不明」を許可してインストール",
      "download.all": "すべてのリリースを見る ↗",
      "faq.title": "よくある質問",
      "faq.q1": "なぜこんなに速くダウンロードできるの？",
      "faq.a1": "多くのサイトは接続ごとに速度制限をしています。FeiLiu はファイルを分割し、最大 64 の同時接続で別々の部分をダウンロード。制限を回避して帯域を使い切ります。",
      "faq.q2": "すべてのサイトでマルチ接続が使えますか？",
      "faq.a2": "サーバーがレンジリクエスト（Accept-Ranges）に対応しているか次第です。タスク作成時に自動判定して UI に表示し、非対応サーバーでは自動的に単一接続に切り替えます。",
      "faq.q3": "アプリを再起動してもダウンロードは失われませんか？",
      "faq.a3": "失われません。各チャンクの進捗はリアルタイムでディスクに保存され、再起動後に「続行」（または自動復帰）を押せば止まったバイト位置から再開します。",
      "faq.q4": "iOS 版はいつ使えますか？",
      "faq.a4": "iOS アプリは完成しており、署名とリリース準備中です。App Store / TestFlight から提供予定です。",
      "footer.brand": "FeiLiu Downloader",
      "footer.note": "Tauri 2 & Rust で構築 · MIT License",
      _title: "FeiLiu Downloader — マルチスレッド高速ダウンローダー",
      _lang: "ja",
    },

    de: {
      "nav.features": "Funktionen",
      "nav.download": "Download",
      "nav.faq": "FAQ",
      "nav.cta": "App holen",
      "hero.badge": "⚡ Bis zu 64 parallele Verbindungen · Resume-Support · Kostenlos",
      "hero.title": 'Der <span class="grad">blitzschnelle</span> Multi-Thread-Downloader',
      "hero.sub": "Server drosseln Ihre Downloads? Riesige Dateien kriechen nur dahin? FeiLiu teilt Dateien in Chunks und lädt sie parallel herunter — Ihr volles Bandbreiten-Potenzial, ein Vielfaches schneller. Für Desktop und iOS.",
      "hero.cta1": "Kostenlos laden",
      "hero.cta2": "Funktionen ansehen",
      "stats.c1": "Max. Verbindungen",
      "stats.c2": "Resume überall",
      "stats.c3": "Plattformen",
      "stats.c4": "Werbung & Beileichen",
      "features.title": "Warum FeiLiu Downloader",
      "features.f1.t": "Multi-Connection-Beschleunigung",
      "features.f1.p": "Dateien werden in 8-MiB-Chunks aufgeteilt und mit bis zu 64 parallelen Verbindungen geladen. Dramatisch schneller bei gedrosselten Servern.",
      "features.f2.t": "Pause & Fortsetzen",
      "features.f2.p": "Jederzeit pausieren und später fortfahren. Unterbrochene Downloads werden beim Neustart automatisch fortgesetzt — kein Byte verschenkt.",
      "features.f3.t": "Intelligente Erkennung",
      "features.f3.p": "Link einfügen — FeiLiu erkennt Dateiname, Größe und Range-Unterstützung und weicht automatisch auf eine einzelne Verbindung aus, falls der Server keine Bereiche zulässt.",
      "features.f4.t": "Task-Verwaltung",
      "features.f4.p": "Mehrere Dateien parallel laden — mit Live-Geschwindigkeit, Restzeit und Verbindungsanzahl auf einen Blick.",
      "features.f5.t": "Sauber & privat",
      "features.f5.p": "Keine Werbung, kein Bundleware, keine Hintergrund-Uploads. Eine lokale Rust-Engine, die nichts sammelt.",
      "features.f6.t": "Federleicht",
      "features.f6.p": "Auf Tauri basis — ein ~4-MB-Installer mit minimalem Speicherbedarf. Auch für ältere Rechner.",
      "download.title": "FeiLiu herunterladen",
      "download.sub": "Plattform wählen · v0.1.0",
      "dl.mac.name": "macOS",
      "dl.mac.req": "Apple Silicon (M-Serie) · macOS 10.15+",
      "dl.mac.btn": "dmg herunterladen",
      "dl.win.name": "Windows",
      "dl.win.req": "Windows 10/11 · 64-Bit",
      "dl.win.btn": "Installer herunterladen",
      "dl.win.hint": "Automatisch von GitHub Actions gebaut",
      "dl.ios.name": "iOS",
      "dl.ios.req": "iOS 13+ · iPhone / iPad · arm64",
      "dl.ios.btn": "IPA herunterladen",
      "dl.ios.hint": "Entwickler-signiert · nur auf registrierten Geräten · TestFlight geplant",
      "dl.ios.alert": "Das iOS-IPA ist entwickler-signiert und lässt sich nur auf beim Team registrierten Geräten installieren. TestFlight / App Store ist in Vorbereitung.",
      "dl.android.name": "Android",
      "dl.android.req": "Android 8.0+ · arm64-v8a",
      "dl.android.btn": "APK herunterladen",
      "dl.android.hint": "Selbst-signiertes APK · Installation mit \"unbekannten Quellen\" erlauben",
      "download.all": "Alle Releases & ältere Versionen ↗",
      "faq.title": "Häufige Fragen",
      "faq.q1": "Warum ist der Download so viel schneller?",
      "faq.a1": "Viele Websites drosseln jede einzelne Verbindung. FeiLiu teilt die Datei in Chunks und lädt unterschiedliche Teile mit bis zu 64 gleichzeitigen Verbindungen — Drosselung umgangen, Bandbreite voll ausgereizt.",
      "faq.q2": "Funktioniert Multi-Connection auf jeder Website?",
      "faq.a2": "Das hängt davon ab, ob der Server Byte-Bereiche unterstützt (Accept-Ranges). FeiLiu prüft das bei jeder Aufgabe automatisch und zeigt es an; nicht unterstützte Server nutzen automatisch eine einzelne Verbindung.",
      "faq.q3": "Überlebt mein Download einen App-Neustart?",
      "faq.a3": "Ja. Der Fortschritt jedes Chunks wird in Echtzeit auf der Festplatte gespeichert. Nach dem Neustart „Fortsetzen“ klicken (oder Auto-Resume aktivieren) — es wird exakt an der letzten Stelle fortgesetzt.",
      "faq.q4": "Wann erscheint die iOS-Version?",
      "faq.a4": "Die iOS-App ist fertig und wird gerade signiert und vorbereitet. Sie wird über den App Store / TestFlight erscheinen.",
      "footer.brand": "FeiLiu Downloader",
      "footer.note": "Gebaut mit Tauri 2 & Rust · MIT-Lizenz",
      _title: "FeiLiu Downloader — Multi-Thread Highspeed-Downloader",
      _lang: "de",
    },
  };

  var i18n = {
    t: function (key) {
      var d = DICT[this.lang] || DICT.en;
      return d[key] != null ? d[key] : (DICT.en[key] || key);
    },
    lang: "en",
  };
  window.i18n = i18n;

  function detectLang() {
    var saved = null;
    try {
      saved = localStorage.getItem("feiliu-lang");
    } catch (e) {}
    if (saved && DICT[saved]) return saved;
    return "en"; // 默认英语
  }

  function apply(lang) {
    if (!DICT[lang]) lang = "en";
    i18n.lang = lang;
    try {
      localStorage.setItem("feiliu-lang", lang);
    } catch (e) {}

    var dict = DICT[lang];
    document.documentElement.lang = dict._lang;
    document.title = dict._title;

    document.querySelectorAll("[data-i18n]").forEach(function (el) {
      var key = el.getAttribute("data-i18n");
      if (dict[key] != null) el.textContent = dict[key];
    });
    document.querySelectorAll("[data-i18n-html]").forEach(function (el) {
      var key = el.getAttribute("data-i18n-html");
      if (dict[key] != null) el.innerHTML = dict[key];
    });

    document.querySelectorAll("#langSwitch button").forEach(function (b) {
      b.classList.toggle("active", b.getAttribute("data-lang") === lang);
    });
  }

  document.addEventListener("DOMContentLoaded", function () {
    apply(detectLang());

    document.querySelectorAll("#langSwitch button").forEach(function (b) {
      b.addEventListener("click", function () {
        apply(b.getAttribute("data-lang"));
      });
    });

    // 平滑滚动
    document.querySelectorAll('a[href^="#"]').forEach(function (a) {
      a.addEventListener("click", function (e) {
        var id = a.getAttribute("href").slice(1);
        var el = id ? document.getElementById(id) : null;
        if (el) {
          e.preventDefault();
          el.scrollIntoView({ behavior: "smooth" });
        }
      });
    });
  });
})();
