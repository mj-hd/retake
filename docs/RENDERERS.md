# Review targets and renderers

English · [日本語](RENDERERS.ja.md)

This document describes the target formats accepted by `open_review`, their capture behavior, and their limits.

## Target overview

| Type | Input | Review representation |
| --- | --- | --- |
| `web` | HTTP(S) URL | Fixed viewport PNG plus a capture-time DOM map |
| `html` | Local HTML file | Fixed viewport PNG plus a capture-time DOM map |
| `markdown` | Local Markdown file | Rendered, scrollable full-document PNG with source-line mapping |
| `image` | Local PNG, JPEG, or WebP | Original-resolution image |
| `text` | Local text file | Rendered text with byte-offset mapping |
| `adb` | Android device serial | Screenshot plus capture-time UIAutomator nodes |
| `code` | Rust, TypeScript, or JavaScript source | Interactive infinite field with source and symbol mapping |
| `pdf` | Local PDF | Vertical page document with page/text mapping |
| `pencil` | Pencil MCP PNG export and node metadata | Exported frame with Pencil node mapping |
| `video` | Local video file | Scrollable sampled storyboard with timecodes |
| `macos_window` | Running macOS app name or `.app` path | Window PNG including its title bar |

Web and local HTML targets are currently immutable image snapshots in the review UI, not live pages. Interactive archived HTML is feasible, but it requires a sandboxed second origin and stored page assets so untrusted page scripts never execute with access to the review API. Live rendering should remain an optional inspection mode; the frozen capture must remain the canonical revision, annotation, and diff surface.

## Web and local HTML

```json
{ "targets": [{ "type": "web", "url": "https://example.com" }] }
```

```json
{ "targets": [{ "type": "html", "path": "/absolute/path/page.html" }] }
```

The renderer uses Playwright Chromium at the requested viewport (1280×800 by default), stores a 2× PNG, and records visible DOM element bounds in CSS pixels. A comment can therefore resolve to nearby tags, text, and XPath-like references without contacting the page again at submission time.

## Markdown

Use `type: "markdown"` for a rendered document:

```json
{ "targets": [{ "type": "markdown", "path": "/absolute/path/README.md" }] }
```

- Renders headings, lists, code blocks, and other Markdown over the full document; users can scroll and comment.
- Renders Mermaid code blocks as diagrams.
- Resolves comments to the corresponding Markdown source line, or to the nearest block when whitespace is selected.
- Use `type: "text"` instead to review the raw source.
- Limits: 512 KiB and 12,000 CSS pixels in height.

## Image and plain text

```json
{ "targets": [{ "type": "image", "path": "/absolute/path/screen.png" }] }
```

```json
{ "targets": [{ "type": "text", "path": "/absolute/path/notes.txt" }] }
```

Images retain their original resolution. Text is rendered into a review image while preserving character/byte ranges for resolved comments.

## Android (adb)

Use `type: "adb"` for a connected Android device:

```json
{ "targets": [{ "type": "adb", "path": "<adb serial>" }] }
```

- Get the serial from `adb devices -l`.
- Captures the screen with `screencap` and maps elements with the UIAutomator tree at capture time.
- The device is not contacted again when the review is submitted.
- Set `RETAKE_ADB` when `adb` is not available on `PATH`.

## macOS window

```json
{ "targets": [{ "type": "macos_window", "path": "Retake" }] }
```

- Set `path` to a running application name or a path such as `/Applications/Retake.app`.
- When an app has multiple windows, narrow the match with `"metadata": { "window_title": "Retake" }`. You can also provide a numeric `window_id`.
- Captures the frontmost matching window as a frozen PNG, including the title bar and excluding its shadow.
- On first use, macOS requires Screen & System Audio Recording permission for the application hosting the MCP process.
- Window IDs are transient. Comments resolve to capture-image coordinates, and revisions find the window again by application name and title.

## Source code

Use `type: "code"` to analyze a source file with Tree-sitter:

```json
{ "targets": [{ "type": "code", "path": "/absolute/file.rs" }] }
```

- A single zoomable **infinite field**: zoom out for file frames, dependencies, symbols, and call edges; zoom in for syntax-highlighted source inside the symbol boxes.
- Pan with the wheel or Shift-drag. Zoom with Ctrl/Cmd-wheel or the toolbar's − and + controls.
- Comments resolve to an identifier and line/column when zoomed in, or to a type, function, or dependency when zoomed out.
- Supported languages: Rust, TypeScript, and JavaScript (requires the worker dependencies installed during setup).

## PDF

PDF pages appear in a vertical document. Selections resolve to a page number and nearby extractable text when available.

```json
{ "targets": [{ "type": "pdf", "path": "/absolute/path/slides.pdf" }] }
```

- Limits: 1–40 pages and 30 MiB. All pages are fitted within a maximum logical height of 12,000 CSS pixels.
- Image-only scans have no extractable text, so comments refer to a position on a page instead.

## Pencil MCP

`type: "pencil"` reviews a **PNG export and node map from Pencil MCP**. retake never reads encrypted `.pen` files directly. Open the design in Pencil and use Pencil MCP to export the frame and read node IDs, names, text, and bounds relative to the frame's top-left corner.

Example metadata:

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

Pass the exported PNG and metadata to `open_review`:

```json
{ "targets": [{ "type": "pencil", "path": "/absolute/temporary/export/frame-id.png", "metadata": { "version": 1, "document": "design.pen", "frame_id": "frame-id", "frame_name": "Home", "width": 800, "height": 600, "scale": 2, "nodes": [] } }] }
```

The width, height, and node rectangles use logical pixels. The PNG is rendered at `scale` times that size (1–4). Comments can resolve to a node ID, path, and text; background selections refer to the frame and position. retake stores the PNG and metadata at capture time and does not reconnect to Pencil on submission. Alternatively, write a JSON manifest containing `png_path` and pass its absolute path as `path`.

## Video storyboard

Local video is sampled into a scrollable storyboard of at most 64 frames. A comment on one frame refers to its timecode and source-video position; a selection across frames refers to a time range.

```json
{ "targets": [{ "type": "video", "path": "/absolute/path/clip.mp4" }] }
```

- Requires `ffmpeg` and `ffprobe` (`brew install ffmpeg` on macOS). Set `RETAKE_FFMPEG` and `RETAKE_FFPROBE` to override their paths.
- Limits: 500 MiB and 120 minutes. Short clips use roughly one frame per second; longer clips are sampled evenly and stored as a fixed snapshot of at most 64 frames.
- For a denser look at part of a long video, pass `"metadata": { "start_seconds": 30, "end_seconds": 90 }`. Timecodes still refer to the original video.
- This is not continuous playback or audio review; motion and sound between sampled frames are not included.
