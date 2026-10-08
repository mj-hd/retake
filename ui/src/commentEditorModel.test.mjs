import assert from 'node:assert/strict';
import test from 'node:test';
import { shouldSubmitComment } from './commentEditorModel.ts';

const enter = {
  key: 'Enter',
  shiftKey: false,
  isComposing: false,
  keyCode: 13,
};

test('submits with Enter and keeps Shift plus Enter for a new line', () => {
  assert.equal(shouldSubmitComment(enter, false, false), true);
  assert.equal(shouldSubmitComment({ ...enter, shiftKey: true }, false, false), false);
});

test('never submits while an IME composition is active or being confirmed', () => {
  assert.equal(shouldSubmitComment({ ...enter, isComposing: true }, false, false), false);
  assert.equal(shouldSubmitComment({ ...enter, keyCode: 229 }, false, false), false);
  assert.equal(shouldSubmitComment(enter, true, false), false);
  assert.equal(shouldSubmitComment(enter, false, true), false);
});
