# Architecture

English · [日本語](ARCHITECTURE.ja.md)

This document describes retake's runtime components, review lifecycle, and implementation details

## Runtime components

- `retake mcp` is a local MCP stdio server used by the agent
- The server starts an HTTP review UI on a random `127.0.0.1` port
- `ui/` contains the React review application
- `workers/web-capture/` captures web content with Playwright
- renderer crates capture supported targets into immutable review snapshots
- review metadata and captured assets are held in the MCP process's in-memory store

`RETAKE_UI_DIR` locates the built UI and `RETAKE_WEB_WORKER` locates the web capture worker. The server does not write review data into the reviewed project

## Review lifecycle

1. `open_review` captures the ordered target list, creates a review, starts the UI server, and optionally opens an owned Chromium window
2. The agent shares the returned review URL and immediately calls `wait_review` in the same turn
3. The user annotates snapshots and submits feedback
4. The agent receives annotations, reports progress with `review_message`, and applies the requested changes
5. `update_review` captures the same ordered targets as a new immutable revision and asks the owned review browser to come to the foreground
6. The open review window updates so the user can compare revisions and submit another round

Questions use `review_message` with `kind: "question"` and `wait_reply`. Reviews do not survive an MCP restart; reconnect and use `open_review` to start a new review

## Waiting and process lifetime

An MCP server cannot start a new agent turn after the agent has finished responding. The agent therefore calls `wait_review` in the same turn as `open_review`, repeats while the result is `pending`, and starts waiting again after each `update_review`

A wait lasts at most ten minutes. A `chat` result means the user chose to continue in the original agent conversation

The owned browser remains open after Submit so revisions can be compared. Cancel, **Close window**, closing the native window, or MCP shutdown closes it. Manually opened browser tabs are not owned by retake

## Snapshots and comparison

Each revision stores immutable snapshots. The review UI can compare a selected earlier revision with the latest revision side by side, with a slider, or using pixel difference blending when the renderer supports it

Snapshots are immutable only within the current MCP process. The in-memory store deliberately has no database, migrations, or resume path. This keeps reviewed project directories clean, but captured images and videos count toward process memory until shutdown

Web and HTML capture and rendered text use 2× images while keeping annotation and DOM coordinates in CSS pixels. Uploaded images retain their original resolution. Renderer-specific behavior and limits are documented in [RENDERERS.md](RENDERERS.md)

## Security boundaries

- The review server binds only to `127.0.0.1`
- Review URLs contain access tokens and must not be written to logs, source files, commits, or issue reports
- Machine-specific MCP configuration belongs in each agent's user configuration, not in a project repository
- MCP shutdown invalidates all review IDs, URL tokens, cookies, and captured assets
