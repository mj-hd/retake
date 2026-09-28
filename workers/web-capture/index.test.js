import assert from 'node:assert/strict';
import test from 'node:test';
import { renderMarkdown } from './index.js';

test('renders Mermaid fences as source-mapped diagram containers', () => {
  const html = renderMarkdown('# Flow\n\n```mermaid\nflowchart LR\n  A --> B\n```\n', 'flow.md', '.');
  assert.match(html, /class="mermaid" data-source-line="3"/);
  assert.match(html, /flowchart LR/);
  assert.doesNotMatch(html, /language-mermaid/);
});
