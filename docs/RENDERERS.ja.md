# レビュー対象とレンダラー

[English](RENDERERS.md) · 日本語

`open_review`で指定できる形式、キャプチャ方法、制限を説明します

## 対応対象

| 種類 | 入力 | レビュー表示 |
| --- | --- | --- |
| `web` | HTTP(S) URL | 固定ビューポートのPNGとキャプチャ時のDOMマップ |
| `html` | ローカルHTML | 固定ビューポートのPNGとキャプチャ時のDOMマップ |
| `markdown` | ローカルMarkdown | ソース行マッピング付きのスクロール可能な全文PNG |
| `image` | ローカルPNG、JPEG、WebP | 元の解像度の画像 |
| `text` | ローカルテキスト | バイト位置マッピング付きのレンダリング済みテキスト |
| `adb` | Android端末のserial | スクリーンショットとキャプチャ時のUIAutomator node |
| `code` | Rust、TypeScript、JavaScript | ソースとsymbolマッピング付きの無限キャンバス |
| `pdf` | ローカルPDF | ページとテキストを対応付けた縦長ドキュメント |
| `pencil` | Pencil MCPのPNG exportとnode metadata | nodeマッピング付きのexport済みframe |
| `video` | ローカル動画 | timecode付きのスクロール可能なstoryboard |
| `macos_window` | 起動中のmacOSアプリ名または`.app`パス | タイトルバーを含むウィンドウのPNG |

## WebとローカルHTML

```json
{ "targets": [{ "type": "web", "url": "https://example.com" }] }
```

```json
{ "targets": [{ "type": "html", "path": "/absolute/path/page.html" }] }
```

Playwright Chromiumを指定ビューポート（標準は1280×800）で実行し、2倍解像度のPNGと表示中DOM要素の境界をCSSピクセルで記録します。コメントの位置は近くのtag、text、XPathに解決されます

## Markdown

```json
{ "targets": [{ "type": "markdown", "path": "/absolute/path/README.md" }] }
```

- 見出し、リスト、コードブロックなどを含む全文を描画し、スクロールしてコメントできます
- Mermaidのコードブロックは図として描画します
- コメントした位置を、対応するMarkdownソースの行番号へ解決します。余白を選んだ場合は最も近いブロックの行番号を使います
- 生のソースを確認する場合は`text`を使います
- 上限は512 KiB、描画時の高さは12,000 CSSピクセルです

## 画像とプレーンテキスト

```json
{ "targets": [{ "type": "image", "path": "/absolute/path/screen.png" }] }
```

```json
{ "targets": [{ "type": "text", "path": "/absolute/path/notes.txt" }] }
```

画像は元の解像度を維持します。テキストは文字・バイト範囲との対応を保ったレビュー画像へ描画します

## Android（adb）

```json
{ "targets": [{ "type": "adb", "path": "<adb serial>" }] }
```

- `path`には接続中の端末を識別するserialを指定します。serialは`adb devices -l`で確認できます
- `screencap`で画面を取得し、キャプチャ時のUIAutomator treeから要素を対応付けます
- `adb`が`PATH`にない場合は`RETAKE_ADB`を指定します

## macOSウィンドウ

```json
{ "targets": [{ "type": "macos_window", "path": "Retake" }] }
```

- `path`には起動中のアプリ名、または`/Applications/Retake.app`のような`.app`パスを指定します
- 同じアプリに複数のウィンドウがある場合は、`"metadata": { "window_title": "Retake" }`でタイトルを絞り込めます。`window_id`を直接指定することもできます
- 最前面に近い一致ウィンドウを、タイトルバーを含みshadowを除いた固定PNGとして保存します
- 初回利用時、MCPを起動したアプリにmacOSの「画面収録とシステムオーディオ録音」権限が必要です
- ウィンドウIDは一時的なため、コメントはキャプチャ画像内の座標へ解決します。更新時はアプリ名とタイトルから再取得します

## ソースコード

```json
{ "targets": [{ "type": "code", "path": "/absolute/file.rs" }] }
```

- 1つの拡大縮小可能な無限キャンバスとして表示します。縮小時はfile frame、依存関係、symbol、call edge、拡大時はsymbol box内の実際のソースを表示します
- wheelまたはShift+dragで移動し、Ctrl/Cmd+wheelまたはtoolbarの−と＋で拡大縮小します
- 拡大時のコメントはidentifierと行・列、縮小時はtype、function、dependencyへ解決します
- Tree-sitterのhighlight queryでtoken種別と単語境界を取得します。現在の対応言語はRust、TypeScript、JavaScriptです
- セットアップ時にworkerの依存関係をインストールする必要があります

## PDF

```json
{ "targets": [{ "type": "pdf", "path": "/absolute/path/slides.pdf" }] }
```

ページを縦方向に並べます。選択範囲はページ番号と、取得できる場合は近くのテキストへ解決します

- 1〜40ページ、30 MiBまでです。すべてのページを最大12,000 CSSピクセルの論理高さへ収めます
- 画像だけのscanには抽出可能なtextがないため、コメントの位置はページ上の位置に解決されます

## Pencil MCP

`pencil`はPencil MCPから得たPNG exportとnode mapをレビューします。Pencilでデザインを開き、Pencil MCPからframe画像と、frame左上を基準とするnode ID、name、text、boundsを取得します

metadataの例:

```json
{
  "version": 1,
  "document": "design.pen",
  "frame_id": "frame-id",
  "frame_name": "Home",
  "width": 800,
  "height": 600,
  "scale": 2,
  "nodes": [
    { "id": "node-id", "name": "Title", "type": "text", "text": "Welcome", "path": ["Title"], "rect": { "x": 40, "y": 50, "width": 260, "height": 42 } }
  ]
}
```

exportしたPNGとmetadataを`open_review`へ渡します

```json
{ "targets": [{ "type": "pencil", "path": "/absolute/temporary/export/frame-id.png", "metadata": { "version": 1, "document": "design.pen", "frame_id": "frame-id", "frame_name": "Home", "width": 800, "height": 600, "scale": 2, "nodes": [] } }] }
```

幅、高さ、nodeの矩形は論理ピクセルです。PNGはその`scale`倍（1〜4）で描画します。コメントはnode ID、path、textへ解決でき、背景の選択はframeと位置を参照します。`png_path`を含むJSON manifestの絶対パスを`path`へ指定する方法もあります。

## 動画

ローカル動画から最大64frameを抽出し、スクロール可能なstoryboardにします。1frame上のコメントはtimecodeと元動画内の位置、複数frameにまたがる選択は時間範囲を参照します

```json
{ "targets": [{ "type": "video", "path": "/absolute/path/clip.mp4" }] }
```

- `ffmpeg`と`ffprobe`が必要です。macOSでは`brew install ffmpeg`で導入できます。場所を変える場合は`RETAKE_FFMPEG`と`RETAKE_FFPROBE`を指定します
- 上限は500 MiB、120分です。短い動画は約1秒ごと、長い動画は全体から均等に最大64frameを固定スナップショットとして抽出します
- 長い動画の一部を詳しく見るには`"metadata": { "start_seconds": 30, "end_seconds": 90 }`を指定します。timecodeは元動画基準です
- 連続再生や音声レビューには対応しません。抽出frame間の動きと音声は含まれません
