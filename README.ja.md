<h1 align="center"><img src="assets/brand/retake-readme-logo.png" alt="Retake" width="875"></h1>

[English](README.md) · 日本語

AIエージェントが生成したデザイン、文書、コードを、ブラウザ上で確認して修正依頼できるレビュー環境です

https://github.com/user-attachments/assets/ed6a4a67-906e-4bab-bbe7-f69023da4999

## 機能

- 画面上の範囲や位置を指定してコメント
- エージェントによる修正状況をリアルタイムに確認
- 過去リビジョンと最新版を左右比較、スライダー、ピクセル差分で確認
- Web、HTML、Markdown、画像、テキスト、Android、macOSウィンドウ、コード、PDF、Pencil、動画をレビュー

対応形式と必要なツールは [レビュー対象とレンダラー](docs/RENDERERS.ja.md) を参照してください

## インストール

### 選択肢1：macOSデスクトップアプリ（推奨）

1. [GitHub Releasesの最新版](https://github.com/mj-hd/retake/releases/latest)からMacに合うDMGをダウンロードします
   - Apple Silicon（`arm64`）：`Retake_*_macos-aarch64.dmg`
   - Intel（`x86_64`）：`Retake_*_macos-x86_64.dmg`
2. DMGを開き、`Retake.app`を「アプリケーション」フォルダへコピーします
3. 「アプリケーション」からRetakeを起動し、利用するエージェントの「インストール」、または「すべてインストール」をクリックします
4. インストールしたエージェントを再起動します

### 選択肢2：CLI（macOS / Linux）

CLI版のインストールにはNode.js 20以降、npm、curlが必要です。

#### 1. ビルド済みバイナリをインストール

GitHub ReleasesからOS・CPUに合うビルドを取得し、`~/.local/share/retake`へインストールします。macOS（Apple Silicon / Intel）とLinux（arm64 / x86_64）に対応しています

```bash
curl -fsSL https://raw.githubusercontent.com/mj-hd/retake/main/install.sh | sh
```

インストーラーはworkerの依存関係とPlaywright Chromiumもセットアップし、`~/.local/bin/retake`へリンクを作ります。特定バージョンを入れる場合は、例えば`RETAKE_VERSION=v0.1.0`を指定します

```bash
curl -fsSL https://raw.githubusercontent.com/mj-hd/retake/main/install.sh | RETAKE_VERSION=v0.1.0 sh
```

`~/.local/bin`が`PATH`に含まれていない場合は追加してください

<details>
<summary>ソースからビルドする場合</summary>

### ソースビルド

利用するレンダラーに対応するツールが必要です。詳細は [レビュー対象とレンダラー](docs/RENDERERS.ja.md) を参照してください

```bash
git clone https://github.com/mj-hd/retake.git
cd retake
export RETAKE_ROOT="$(pwd -P)"

cargo build --release -p retake-server
(cd ui && npm ci && npm run build)
(cd workers/web-capture && npm ci --ignore-scripts --legacy-peer-deps && npx playwright install chromium)
```

</details>

#### 2. Skillのインストール

同梱のスキルは [Agent Skills](https://agentskills.io/) 形式です。複数のエージェントへまとめて導入する場合は [`skills`](https://github.com/vercel-labs/skills) が簡単です

```bash
npx skills add mj-hd/retake --skill retake --global \
  --agent claude-code --agent codex --agent gemini-cli --agent opencode
```

更新には `npx skills update -g retake`、インストール前の確認には `npx skills add mj-hd/retake --list` を使えます

Claude Codeではプラグインとして導入することもできます

```bash
claude plugin marketplace add mj-hd/retake
claude plugin install retake@retake
```

Claude Codeに対しては、`skills`とプラグインのどちらか一方を選んでください

#### 3. MCPを登録

以下はビルド済みバイナリ版のコマンドです。ソースビルドの場合は、コマンド中の`$HOME/.local/bin/retake`を`$RETAKE_ROOT/target/release/retake`へ置き換えてください。UIとworkerは実行ファイルの位置から自動検出されます

#### Claude Code

```bash
claude mcp add --scope user \
  retake -- "$HOME/.local/bin/retake" mcp
claude mcp list
```

#### Codex CLI

```bash
codex mcp add retake \
  -- "$HOME/.local/bin/retake" mcp
codex mcp list
```

#### Gemini CLI

```bash
gemini mcp add --scope user \
  retake "$HOME/.local/bin/retake" mcp
gemini mcp list
```

#### OpenCode V1

ユーザー設定 `~/.config/opencode/opencode.json` の `mcp` に追加します

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "retake": {
      "type": "local",
      "command": ["retake", "mcp"],
      "enabled": true
    }
  }
}
```

#### OpenCode V2

ユーザー設定 `~/.config/opencode/opencode.json` の `mcp.servers` に追加します

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "servers": {
      "retake": {
        "type": "local",
        "command": ["retake", "mcp"]
      }
    }
  }
}
```

`retake`が見つからない場合は`~/.local/bin/retake`のような絶対パスを指定してください。端末固有のパスを含む設定は、プロジェクトの`opencode.json`ではなくユーザー設定だけに置きます

## 使い方

レビュー対象のプロジェクトでエージェントを起動し、対象を指定して依頼します

```text
README.ja.mdをretakeして
```

1. 専用のレビュー画面が開く
2. 範囲や位置を選んでコメントし、送信する
3. エージェントの修正後、同じ画面で新しいリビジョンを確認する

## 開発者向け

内部構成とレビューのライフサイクルは [アーキテクチャ](docs/ARCHITECTURE.ja.md) を参照してください

### ローカルSkillをリンク

ローカルの変更をすぐ反映したい場合は、ユーザーレベルのSkillへシンボリックリンクを作成します

```bash
mkdir -p "$HOME/.claude/skills" "$HOME/.codex/skills" \
  "$HOME/.gemini/skills" "$HOME/.config/opencode/skills"
ln -sfn "$RETAKE_ROOT/.agents/skills/retake" "$HOME/.claude/skills/retake"
ln -sfn "$RETAKE_ROOT/.agents/skills/retake" "$HOME/.codex/skills/retake"
ln -sfn "$RETAKE_ROOT/.agents/skills/retake" "$HOME/.gemini/skills/retake"
ln -sfn "$RETAKE_ROOT/.agents/skills/retake" "$HOME/.config/opencode/skills/retake"
```

### Tauriデスクトッププレビュー（macOS）

既存のReact UIをTauriウィンドウで動かすデスクトップPoCは`ui/src-tauri`にあります

```bash
cd ui
npm ci
npm run desktop:build
open src-tauri/target/release/bundle/macos/Retake.app
```

`desktop:build`はPlaywrightがインストールしたheadless Chromiumをアプリの非公開リソースとして同梱します。見つからない場合は、先に`workers/web-capture`で`npx playwright install chromium-headless-shell`を実行してください

開発中のMCPサーバーでChromeの代わりにビルド済みRetake.appを使うには、アプリ内の実行ファイルを指定します

```bash
export RETAKE_DESKTOP_BIN="$RETAKE_ROOT/ui/src-tauri/target/release/bundle/macos/Retake.app/Contents/MacOS/retake-desktop"
```

デスクトップアプリはレビューURLをプロセス引数へ出さず、既存のbrowser workerと同じstdinプロトコルで受け取ります。環境変数を指定しない場合は、従来のPlaywrightウィンドウへフォールバックします

### 検証

```bash
cargo fmt -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd ui && npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cd ../workers/web-capture && npm test
```

### リリース

`v`で始まるタグをpushすると、GitHub ActionsがCLI archiveとSHA-256 checksumに加え、Developer IDで署名・Apple公証したmacOS版DMG（Apple Silicon / Intel）を作成し、GitHub Releaseへ公開します

```bash
git tag v0.1.0
git push origin v0.1.0
```
