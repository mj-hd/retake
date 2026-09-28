import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const directory = path.dirname(fileURLToPath(import.meta.url));

function mapSource(grammar, extension, source) {
  const temporary = mkdtempSync(path.join(tmpdir(), 'retake-code-map-'));
  const sourcePath = path.join(temporary, `sample.${extension}`);
  writeFileSync(sourcePath, source);
  try {
    const result = spawnSync(process.execPath, [path.join(directory, 'code-map.js'), grammar, sourcePath, '100'], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return JSON.parse(result.stdout);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

function capturedText(source, map, kind) {
  const kindIndex = map.tokens.kinds.indexOf(kind);
  const matches = [];
  for (let index = 0; index < map.tokens.spans.length; index += 3) {
    if (map.tokens.spans[index + 2] === kindIndex) {
      matches.push(source.slice(map.tokens.spans[index], map.tokens.spans[index + 1]));
    }
  }
  return matches;
}

test('emits Tree-sitter tokens at UTF-16 offsets', () => {
  const source = 'const greeting: string = "こんにちは"; // 注釈\n';
  const map = mapSource('typescript', 'ts', source);
  assert.ok(capturedText(source, map, 'keyword').includes('const'));
  assert.ok(capturedText(source, map, 'string').includes('"こんにちは"'));
  assert.ok(capturedText(source, map, 'comment').includes('// 注釈'));
});

test('uses the Rust highlight query', () => {
  const source = 'pub fn greet() -> &\'static str { "hello" }\n';
  const map = mapSource('rust', 'rs', source);
  assert.ok(capturedText(source, map, 'keyword').includes('fn'));
  assert.ok(capturedText(source, map, 'function').includes('greet'));
});
