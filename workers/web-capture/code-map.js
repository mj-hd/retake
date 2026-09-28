#!/usr/bin/env node
// Tree-sitter structure and highlight map for one source file.
// The Rust renderer owns extension support and passes a grammar name here.
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import Parser from 'tree-sitter';
import Rust from 'tree-sitter-rust';
import TypeScript from 'tree-sitter-typescript';

const grammarName = process.argv[2];
const file = process.argv[3];
const maxLines = Math.max(1, Number.parseInt(process.argv[4] ?? '8000', 10) || 8000);
if (!grammarName || !file) {
  console.error('usage: node code-map.js <rust|typescript|tsx> <file> [max-lines]');
  process.exit(1);
}

const languages = {
  rust: Rust,
  typescript: TypeScript.typescript,
  tsx: TypeScript.tsx,
};
const language = languages[grammarName];
if (!language) {
  console.error(`unsupported grammar: ${grammarName}`);
  process.exit(1);
}

const abs = path.resolve(file);
const raw = fs.readFileSync(abs, 'utf8');
if (raw.length > 256 * 1024) {
  console.error('source file too large');
  process.exit(1);
}
// Parse the same normalized text that Rust exposes to the UI.
const sourceLines = raw.split(/\r?\n/);
const source = sourceLines.join('\n');

const parser = new Parser();
parser.setLanguage(language);
const tree = parser.parse(source);

const imports = [];
const symbols = [];
const identifiers = [];
const seen = new Set();

function textOf(node) {
  return source.slice(node.startIndex, node.endIndex).replace(/\s+/g, ' ').slice(0, 160);
}

function nameOf(node) {
  const named = node.childForFieldName?.('name') || node.children?.find(child => child.type === 'identifier' || child.type === 'type_identifier');
  return named ? textOf(named) : '';
}

function pushId(node, kind) {
  if (!node || node.endIndex - node.startIndex > 80) return;
  const text = textOf(node);
  if (!text || !/^[\p{L}_$][\p{L}\p{N}_$]*$/u.test(text)) return;
  const key = `${node.startIndex}:${text}`;
  if (seen.has(key)) return;
  seen.add(key);
  identifiers.push({
    text,
    kind,
    start: node.startIndex,
    end: node.endIndex,
    line: node.startPosition.row + 1,
    column: node.startPosition.column,
  });
}

(function walk(node, depth) {
  const type = node.type;
  if (type === 'use_declaration' || type === 'import_statement') {
    const value = textOf(node);
    if (value.startsWith('use ')) {
      const body = value.slice(4).replace(/;$/, '');
      const cleaned = body.split('{')[0].replace(/::+$/, '').trim();
      if (cleaned) imports.push({ module: cleaned.slice(0, 80), line: node.startPosition.row + 1 });
    } else {
      const match = value.match(/from\s+['"]([^'"]+)/) || value.match(/require\(\s*['"]([^'"]+)/) || value.match(/import\s+['"]([^'"]+)/);
      if (match && !imports.some(item => item.module === match[1])) imports.push({ module: match[1].slice(0, 80), line: node.startPosition.row + 1 });
    }
  }
  if (/^(function_item|function_declaration|method_definition|impl_item|struct_item|enum_item|trait_item|class_declaration|interface_declaration|type_alias_declaration|mod_item)$/.test(type)) {
    const name = nameOf(node);
    if (name) symbols.push({ name, kind: type.replace(/_item|_declaration|_definition/g, ''), line: node.startPosition.row + 1, end_line: node.endPosition.row + 1, depth });
  }
  if (type === 'identifier' || type === 'type_identifier' || type === 'property_identifier' || type === 'field_identifier') pushId(node, type);
  for (const child of node.children) {
    if (symbols.length < 80) walk(child, depth + 1);
    else if (identifiers.length < 2500) walk(child, depth + 1);
    if (identifiers.length >= 2500) return;
  }
})(tree.rootNode, 0);

const tokenKinds = ['comment', 'string', 'number', 'keyword', 'type', 'function', 'property', 'constant', 'attribute', 'tag', 'parameter', 'variable', 'operator', 'punctuation'];
const captureKinds = {
  comment: 'comment', 'comment.documentation': 'comment',
  string: 'string', 'string.special': 'string', escape: 'string',
  number: 'number', 'constant.builtin': 'constant', constant: 'constant',
  keyword: 'keyword', label: 'keyword',
  type: 'type', 'type.builtin': 'type', constructor: 'type',
  function: 'function', 'function.method': 'function', 'function.macro': 'function',
  property: 'property', attribute: 'attribute', tag: 'tag',
  'variable.parameter': 'parameter', 'variable.builtin': 'variable', variable: 'variable',
  operator: 'operator',
  'punctuation.bracket': 'punctuation', 'punctuation.delimiter': 'punctuation', 'punctuation.special': 'punctuation',
};
const priorities = { comment: 14, string: 13, keyword: 12, number: 11, constant: 10, function: 9, type: 8, tag: 7, attribute: 6, parameter: 5, property: 4, variable: 3, operator: 2, punctuation: 1 };

function querySource() {
  const here = path.dirname(fileURLToPath(import.meta.url));
  const read = relative => fs.readFileSync(path.join(here, 'node_modules', relative), 'utf8');
  if (grammarName === 'rust') return read('tree-sitter-rust/queries/highlights.scm');
  const common = `${read('tree-sitter-javascript/queries/highlights.scm')}\n${read('tree-sitter-typescript/queries/highlights.scm')}`;
  return grammarName === 'tsx' ? `${common}\n${read('tree-sitter-javascript/queries/highlights-jsx.scm')}` : common;
}

function highlightTokens() {
  let query;
  try {
    query = new Parser.Query(language, querySource());
  } catch (error) {
    console.error(`highlight query disabled: ${error.message}`);
    return { kinds: tokenKinds, spans: [] };
  }
  // node-tree-sitter exposes startIndex/endIndex in JavaScript string
  // (UTF-16) offsets, which can be passed directly to React's slice calls.
  const limit = sourceLines.slice(0, maxLines).join('\n').length;
  const captures = query.captures(tree.rootNode)
    .map(capture => ({ node: capture.node, kind: captureKinds[capture.name] }))
    .filter(capture => capture.kind && capture.node.startPosition.row < maxLines)
    .map(capture => ({
      start: capture.node.startIndex,
      end: Math.min(limit, capture.node.endIndex),
      kind: capture.kind,
    }))
    .filter(capture => capture.end > capture.start)
    .sort((a, b) => a.start - b.start || priorities[b.kind] - priorities[a.kind] || (a.end - a.start) - (b.end - b.start));

  const spans = [];
  let cursor = 0;
  for (const capture of captures) {
    if (capture.start < cursor) continue;
    const kind = tokenKinds.indexOf(capture.kind);
    if (kind < 0) continue;
    spans.push(capture.start, capture.end, kind);
    cursor = capture.end;
    if (spans.length >= 72_000) break;
  }
  return { kinds: tokenKinds, spans };
}

process.stdout.write(JSON.stringify({
  path: abs,
  language: grammarName,
  lines: sourceLines.length,
  imports: imports.slice(0, 40),
  symbols: symbols.slice(0, 60),
  identifiers: identifiers.slice(0, 2500),
  tokens: highlightTokens(),
}));
