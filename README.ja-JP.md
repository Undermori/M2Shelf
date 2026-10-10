# M²Shelf

**MORI MEDIA SHELF**

[简体中文](./README.md) · [English](./README.en-US.md) · [한국어](./README.ko-KR.md)

M²Shelf は Windows 向けのローカルメディア管理アプリです。アニメ、映画・ドラマ、漫画、電子書籍、同人誌、設定資料集に対応。内蔵・外付けドライブや NAS のマップ済みフォルダーを追加して、閲覧・検索・再生・読書ができます。

## 主な機能

- 複数ライブラリの再帰・差分スキャン。フォルダー単位とファイル単位の識別、書籍のスマート混合識別によるシリーズ・単独書籍・カテゴリー整理。
- ポスター／リスト表示、ライブラリ横断閲覧、種類・タグの絞り込み、並べ替え。ローカル名、ファイル名、Bangumi の多言語タイトルを検索。
- 種類に応じた Bangumi の自動・手動紐付け。実写映画は TMDb に対応し、二言語タイトル・公開年・一般的な配布名を解析。ポスターは原語版を優先。紐付け、表示名、表紙は手動修正可能。
- タグ、名前付きお気に入り、一括編集、非表示と復元。
- 外部プレーヤー起動、付属ファイル閲覧、エクスプローラーでの表示。
- 漫画・文字書籍リーダー、読書進捗、しおり、最近開いた項目。
- ローカル表紙キャッシュとバックグラウンドのサムネイル生成。設定で進捗確認・再試行。
- ライト／ダーク／システムテーマ、中国語・英語・日本語・韓国語。ウィンドウサイズ、並べ替え、閲覧位置を記憶。

## 読書

画像フォルダー、CBZ、PDF、EPUB、TXT、MOBI、AZW3 に対応。画像は JPG、JPEG、PNG、WebP、GIF、AVIF、BMP。

- 漫画：単ページ・見開き、横方向のページ送り、連続スクロール、Webtoon、読む方向、ズーム、フィット、背景色、全画面。
- 文字：ページ送り・スクロール、目次、フォント・文字サイズ・太さ、行間・段落間隔、本文幅、余白、配色。
- 読書進捗としおりを保存。起動に成功した動画と書籍を「最近開いた項目」に表示。

MOBI/AZW3 は暗号化されていない MOBI6/KF8 に対応。DRM、通常の ZIP、RAR、7Z は内蔵読書の対象外です。

## ダウンロード

**M²Shelf 0.5.13 · Windows x64**

- [インストーラー](https://github.com/Undermori/M2Shelf/releases/download/v0.5.13/M2Shelf-Setup-0.5.13-x64.exe)
- [Portable ZIP](https://github.com/Undermori/M2Shelf/releases/download/v0.5.13/M2Shelf-Portable-0.5.13-x64.zip)
- [最新 Release](https://github.com/Undermori/M2Shelf/releases/latest) · [全バージョン](https://github.com/Undermori/M2Shelf/releases)

Portable は全体を解凍して `M2Shelf.exe` を起動してください。データベース・設定・既定キャッシュは Windows のアプリデータに保存されます。旧五ファイル構成からの更新も ZIP 全体を解凍してください。

0.5.13 は手動でダウンロードして更新してください。このバージョンはアプリ内更新に対応していません。

Microsoft Edge WebView2 Runtime が必要です。SmartScreen の警告が出る場合があります。

## ローカルデータ

元のメディアは読み取り専用です。索引、表示名、紐付け、タグ、お気に入り、履歴、設定はアプリの SQLite、表紙はキャッシュに保存します。

Bangumi/TMDb の検索と表紙取得には通信が必要です。ローカル閲覧・読書には不要で、メディアをアップロードしません。サーバーやクラウドアカウントも不要です。動画は外部プレーヤーを使用し、オンライン配信、変換、端末間同期、動画の続き再生は提供しません。

## 開発・資料

Tauri 2 · Rust · React 19 · TypeScript · Vite · SQLite

```powershell
npm ci
npm run tauri dev
```

`npm run check` と [AGENTS.md](./AGENTS.md) の Rust 検証を実行します。Windows ビルドは `scripts/build_windows_release.ps1`。

- [動作ガイド（中国語）](./docs/RUNTIME_GUIDE_zh-CN.md)
- [製品仕様](./docs/PRODUCT_SPEC.md)
- [現在の実装](./docs/PROJECT_CONTEXT.md)
- [設計上の決定](./docs/DECISIONS.md)
- [プロジェクト概要](./PROJECT_DOCUMENTATION.md)
- [Windows 開発](./docs/WINDOWS_DEVELOPMENT.md)
- [リリース手順](./docs/UPDATE_RELEASE_PROCESS.md)

## 作者

[森下Undermori · Bilibili](https://space.bilibili.com/2903441)
