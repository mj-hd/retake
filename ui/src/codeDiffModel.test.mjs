import assert from 'node:assert/strict';
import test from 'node:test';
import { compareLines } from './codeDiffModel.ts';

const messages = { moreRemoved: (count) => `${count} removed`, moreAdded: (count) => `${count} added` };

test('compares changed lines with surrounding context', () => {
  assert.deepEqual(compareLines(['a', 'old', 'z'], ['a', 'new', 'z'], messages), [
    { text: 'a', kind: 'same' },
    { text: 'old', kind: 'removed' },
    { text: 'new', kind: 'added' },
    { text: 'z', kind: 'same' },
  ]);
});

test('caps very large rewrite output', () => {
  const before = Array.from({ length: 1001 }, (_, index) => `old ${index}`);
  const after = Array.from({ length: 1001 }, (_, index) => `new ${index}`);
  const lines = compareLines(before, after, messages);
  assert.equal(lines.length, 602);
  assert.deepEqual(lines[300], { text: '701 removed', kind: 'same' });
  assert.deepEqual(lines.at(-1), { text: '701 added', kind: 'same' });
});
