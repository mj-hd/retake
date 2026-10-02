<h1 align="center"><img src="assets/brand/retake-readme-logo.png" alt="Retake" width="875"></h1>

English · [日本語](README.ja.md)

A browser-based review workspace for commenting on and revising designs, documents, and code produced by AI agents

https://github.com/user-attachments/assets/cf0c7a48-ad9f-43ce-8cbc-8a5a90331442

## Features

- Attach comments to selected regions or points
- Follow an agent's revision progress in real time
- Compare an earlier revision with the latest side by side, with a slider, or as a pixel diff
- Review web pages, HTML, Markdown, images, text, Android screens, code, PDFs, Pencil exports, and video

See [Review targets and renderers](docs/RENDERERS.md) for supported formats and their required tools

## Installation

Node.js 20 or later, npm, and curl are required

### 1. Install a prebuilt binary (recommended)

The installer downloads the build for your OS and CPU from GitHub Releases and installs it under `~/.local/share/retake`. macOS (Apple Silicon and Intel) and Linux (arm64 and x86_64) are supported

```bash
curl -fsSL https://raw.githubusercontent.com/mj-hd/retake/main/install.sh | sh
```

It also installs the worker dependencies and Playwright Chromium, then links the executable at `~/.local/bin/retake`. To install a specific version, set `RETAKE_VERSION`, for example:

```bash
curl -fsSL https://raw.githubusercontent.com/mj-hd/retake/main/install.sh | RETAKE_VERSION=v0.1.0 sh
```

Add `~/.local/bin` to `PATH` if it is not already included

<details>
<summary>Build from source instead</summary>

### Source build

Install the tools required by the renderers you intend to use. See [Review targets and renderers](docs/RENDERERS.md) for details

```bash
git clone https://github.com/mj-hd/retake.git
cd retake
export RETAKE_ROOT="$(pwd -P)"

cargo build --release -p retake-server
(cd ui && npm ci && npm run build)
(cd workers/web-capture && npm ci --ignore-scripts --legacy-peer-deps && npx playwright install chromium)
```

</details>

### 2. Install the Skill (recommended)

The bundled skill follows the [Agent Skills](https://agentskills.io/) standard. The easiest multi-agent installation uses [`skills`](https://github.com/vercel-labs/skills):

```bash
npx skills add mj-hd/retake --skill retake --global \
  --agent claude-code --agent codex --agent gemini-cli --agent opencode
```

Run `npx skills update -g retake` to update it later. You can also list the skill before installing with `npx skills add mj-hd/retake --list`.

Claude Code users may install the same skill as a plugin instead:

```bash
claude plugin marketplace add mj-hd/retake
claude plugin install retake@retake
```

Choose either `skills` or the Claude plugin for Claude Code; installing both is unnecessary.

### 3. Register the MCP server

The commands below use the prebuilt binary. For a source build, replace `$HOME/.local/bin/retake` in the command with `$RETAKE_ROOT/target/release/retake`. The UI and worker are discovered automatically from the executable location

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

Add retake under `mcp` in the user config at `~/.config/opencode/opencode.json`

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

Add retake under `mcp.servers` in the user config at `~/.config/opencode/opencode.json`

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

If `retake` is not found, use an absolute path such as `~/.local/bin/retake`. Keep machine-specific paths only in the user config and never in a project's `opencode.json`

## Usage

Start your agent in the project you want to review, then name the target

```text
Retake README.md
```

1. A dedicated review window opens
2. Select a region or point, add comments, and submit
3. After the agent makes changes, inspect the new revision in the same window

## Development

See [Architecture](docs/ARCHITECTURE.md) for internals and the review lifecycle

### Link the local Skill

To make local edits available immediately, create user-level symlinks:

```bash
mkdir -p "$HOME/.claude/skills" "$HOME/.codex/skills" \
  "$HOME/.gemini/skills" "$HOME/.config/opencode/skills"
ln -sfn "$RETAKE_ROOT/.agents/skills/retake" "$HOME/.claude/skills/retake"
ln -sfn "$RETAKE_ROOT/.agents/skills/retake" "$HOME/.codex/skills/retake"
ln -sfn "$RETAKE_ROOT/.agents/skills/retake" "$HOME/.gemini/skills/retake"
ln -sfn "$RETAKE_ROOT/.agents/skills/retake" "$HOME/.config/opencode/skills/retake"
```

### Validation

```bash
cargo fmt -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd ui && npm run build
cd ../workers/web-capture && npm test
```

### Releasing

Pushing a tag beginning with `v` makes GitHub Actions build archives and SHA-256 checksums for each supported platform and publish a GitHub Release

```bash
git tag v0.1.0
git push origin v0.1.0
```
