import assert from 'node:assert/strict';
import test from 'node:test';
import { findCurrentProgress, findOpenQuestion } from './reviewMessages.ts';

const question = (overrides) => ({
  id: 'question',
  kind: 'question',
  text: 'Which one?',
  reply: null,
  handed_off_at: null,
  created_at: '2026-01-01T00:00:00Z',
  ...overrides,
});

test('returns the latest unanswered in-review question', () => {
  const latest = question({ id: 'latest' });
  assert.equal(findOpenQuestion([question({ id: 'first' }), latest]), latest);
});

test('does not reopen replied or chat-handoff questions', () => {
  assert.equal(findOpenQuestion([question({ reply: 'Done' })]), undefined);
  assert.equal(findOpenQuestion([question({ handed_off_at: '2026-01-01T00:01:00Z' })]), undefined);
});

test('returns progress posted after the latest revision', () => {
  const progress = { ...question({ kind: 'progress', text: 'Updating' }), created_at: '2026-01-01T00:02:00Z' };
  assert.equal(findCurrentProgress([progress], '2026-01-01T00:01:00Z'), progress);
  assert.equal(findCurrentProgress([progress], '2026-01-01T00:03:00Z'), undefined);
});
