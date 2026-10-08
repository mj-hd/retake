import assert from 'node:assert/strict';
import test from 'node:test';
import { actionableClients, normalizeClientId } from './desktopInstaller.ts';

const client = (id, state, overrides = {}) => ({
  id,
  state,
  config_path: '',
  version: null,
  message: null,
  ...overrides,
});

test('offers install only for missing or outdated clients', () => {
  assert.deepEqual(
    actionableClients([
      client('claude_code', 'missing'),
      client('claude_desktop', 'installed'),
      client('codex', 'outdated'),
      client('opencode', 'unavailable'),
      client('gemini', 'invalid'),
    ]),
    ['claude_code', 'codex'],
  );
});

test('normalizes the Rust open_code spelling to opencode', () => {
  assert.equal(normalizeClientId('open_code'), 'opencode');
  assert.equal(normalizeClientId('opencode'), 'opencode');
  assert.equal(normalizeClientId('claude_code'), 'claude_code');
});
