---
name: retake
description: Run a retake browser review for designs, documents, code, images, PDFs, video, web pages, or Pencil exports, and keep waiting for feedback through every revision.
compatibility: Requires an MCP client connected to the retake server and access to its review tools.
---

# retake review workflow

Use the tools exposed by the `retake` MCP server whenever the user asks to review, annotate, compare, or revise a supported artifact in the retake review window. MCP clients may display a tool as `retake.open_review`, `mcp__retake__open_review`, or simply `open_review`; use the matching MCP tool rather than trying to run these names in a shell.

## Mandatory workflow

1. Resolve every local target to an absolute path. Keep the complete target list and its order for later revisions.
   - For a running macOS application window, use `type: "macos_window"` with its application name or absolute `.app` path in `path`. Add `metadata.window_title` when the app has multiple windows.
2. Call the retake `open_review` tool once. Unless the user requested otherwise, let it open the owned browser window.
3. Immediately tell the user that the review is ready and include the returned `review_url`. If `browser_opened` is true, do not open the URL a second time.
4. **In the same agent turn, call the retake `wait_review` tool with the returned review ID and `timeout_ms: 600000`. Never finish the turn after only calling `open_review`.**
5. If `wait_review` returns `pending`, call it again for the same review ID. Continue until it returns `submitted`, `cancelled`, or `chat`.
6. On `submitted`, inspect every annotation and apply the requested changes. Send concise progress updates with the retake `review_message` tool using `kind: "progress"`. Keep each progress message to one line and at most 60 characters. Do not end it with a period, full stop, or Japanese `。`.
7. If clarification is needed, send `review_message` with `kind: "question"`, then call `wait_reply` with its message ID. Repeat while pending. If it returns `chat`, stop waiting and ask the question in the original conversation.
8. After making changes, call `update_review` with the original review ID and **the same targets in the same order**. Do not create a separate review for verification.
9. Immediately after every successful `update_review`, tell the user the revised view is ready and **call `wait_review` again in the same agent turn with `timeout_ms: 600000`**. Never finish a turn after `update_review` without starting `wait_review`. If it returns `pending`, keep calling it again. Repeat the feedback/update loop until the review is cancelled or the user asks to stop.

## Lifecycle and safety

- Submit keeps the owned review window open for revision comparison and another feedback round.
- Cancel, the window's **Close window** action, closing its native window, or MCP shutdown closes the owned browser.
- Review state exists only for the lifetime of the MCP process. If it restarts, call `open_review` to start a new review.
- Never expose a review URL token in logs, files, commits, or issue reports.
- A completed agent turn cannot be awakened by MCP later. Keeping `wait_review` active is therefore required, not optional.
