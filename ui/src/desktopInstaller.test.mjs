import assert from 'node:assert/strict';
import test from 'node:test';
import { actionableClients, normalizeClientId } from './desktopInstaller.ts';

const client = (id, state, overrides = {}) => ({
  id,
  state,
  config_path: '',
  version: null,
  skill_state: 'installed',
  restartable: false,
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

test('treats a missing Claude Desktop skill as the combined install action', () => {
  assert.deepEqual(
    actionableClients([
      client('claude_desktop', 'missing', { skill_state: 'missing' }),
      client('claude_desktop', 'installed', { skill_state: 'pending' }),
      client('claude_desktop', 'installed', { skill_state: 'installed' }),
    ]),
    ['claude_desktop'],
  );
});

test('normalizes the Rust open_code spelling to opencode', () => {
  assert.equal(normalizeClientId('open_code'), 'opencode');
  assert.equal(normalizeClientId('opencode'), 'opencode');
  assert.equal(normalizeClientId('claude_code'), 'claude_code');
});
