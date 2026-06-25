#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { repoRoot } from './lib/project-metadata.mjs';

const SKIP_DIRS = new Set(['.git', 'node_modules', 'target', 'dist']);

function walk(dir, files = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (!SKIP_DIRS.has(entry.name)) walk(path.join(dir, entry.name), files);
      continue;
    }
    if (entry.isFile() && entry.name.endsWith('.rs')) files.push(path.join(dir, entry.name));
  }
  return files;
}

function relative(file) {
  return path.relative(repoRoot, file).split(path.sep).join('/');
}

function rustRawStringStart(source, index) {
  let cursor = index;
  if (source[cursor] === 'b' && source[cursor + 1] === 'r') cursor += 2;
  else if (source[cursor] === 'r') cursor += 1;
  else return null;
  let hashes = 0;
  while (source[cursor + hashes] === '#') hashes += 1;
  if (source[cursor + hashes] !== '"') return null;
  return { end: cursor + hashes + 1, hashes };
}

function looksLikeCharLiteral(source, index) {
  if (source[index] !== "'") return false;
  if (/^[A-Za-z_]$/.test(source[index + 1] || '') && source[index + 2] !== "'") return false;
  let cursor = index + 1;
  let escaped = false;
  while (cursor < source.length && cursor - index <= 12) {
    const ch = source[cursor];
    if (ch === '\n' || ch === '\r') return false;
    if (!escaped && ch === "'") return cursor > index + 1;
    escaped = !escaped && ch === '\\';
    if (ch !== '\\') escaped = false;
    cursor += 1;
  }
  return false;
}

function stripCommentsAndStrings(source) {
  let out = '';
  let i = 0;
  let state = 'code';
  let rawHashes = 0;
  while (i < source.length) {
    const ch = source[i];
    const next = source[i + 1];
    if (state === 'code') {
      const raw = rustRawStringStart(source, i);
      if (raw) {
        out += ' '.repeat(raw.end - i);
        rawHashes = raw.hashes;
        i = raw.end;
        state = 'raw-string';
        continue;
      }
      if (ch === '/' && next === '/') { state = 'line'; out += '  '; i += 2; continue; }
      if (ch === '/' && next === '*') { state = 'block'; out += '  '; i += 2; continue; }
      if ((ch === '"') || (ch === 'b' && next === '"')) {
        const consumed = ch === 'b' ? 2 : 1;
        state = 'string';
        out += ' '.repeat(consumed);
        i += consumed;
        continue;
      }
      if (looksLikeCharLiteral(source, i)) { state = 'char'; out += ' '; i += 1; continue; }
      out += ch;
      i += 1;
      continue;
    }
    if (state === 'line') {
      out += ch === '\n' ? '\n' : ' ';
      if (ch === '\n') state = 'code';
      i += 1;
      continue;
    }
    if (state === 'block') {
      out += ch === '\n' ? '\n' : ' ';
      if (ch === '*' && next === '/') { out += ' '; i += 2; state = 'code'; }
      else i += 1;
      continue;
    }
    if (state === 'string') {
      out += ch === '\n' ? '\n' : ' ';
      if (ch === '\\') { out += next === '\n' ? '\n' : ' '; i += 2; continue; }
      if (ch === '"') state = 'code';
      i += 1;
      continue;
    }
    if (state === 'char') {
      out += ch === '\n' ? '\n' : ' ';
      if (ch === '\\') { out += next === '\n' ? '\n' : ' '; i += 2; continue; }
      if (ch === "'") state = 'code';
      i += 1;
      continue;
    }
    if (state === 'raw-string') {
      out += ch === '\n' ? '\n' : ' ';
      if (ch === '"' && source.slice(i + 1, i + 1 + rawHashes) === '#'.repeat(rawHashes)) {
        out += ' '.repeat(rawHashes);
        i += rawHashes + 1;
        state = 'code';
      } else {
        i += 1;
      }
    }
  }
  return out;
}

function checkBalancedBraces(file, source, failures) {
  const clean = stripCommentsAndStrings(source);
  const stack = [];
  for (let i = 0, line = 1, col = 0; i < clean.length; i += 1) {
    const ch = clean[i];
    if (ch === '\n') { line += 1; col = 0; continue; }
    col += 1;
    if (ch === '{' || ch === '(' || ch === '[') stack.push({ ch, line, col });
    if (ch === '}' || ch === ')' || ch === ']') {
      const top = stack.pop();
      const expected = ch === '}' ? '{' : ch === ')' ? '(' : '[';
      if (!top || top.ch !== expected) {
        failures.push({ file, line, message: `unbalanced delimiter '${ch}'` });
        return;
      }
    }
  }
  if (stack.length) {
    const top = stack[stack.length - 1];
    failures.push({ file, line: top.line, message: `unclosed delimiter '${top.ch}'` });
  }
}

function findStructBodies(source) {
  const clean = stripCommentsAndStrings(source);
  const bodies = [];
  const re = /struct\s+([A-Za-z0-9_]+)\s*\{/g;
  let match;
  while ((match = re.exec(clean))) {
    const name = match[1];
    let depth = 1;
    let index = re.lastIndex;
    while (index < clean.length && depth > 0) {
      if (clean[index] === '{') depth += 1;
      else if (clean[index] === '}') depth -= 1;
      index += 1;
    }
    if (depth === 0) bodies.push({ name, start: re.lastIndex, end: index - 1 });
  }
  return bodies;
}

function lineForOffset(source, offset) {
  return source.slice(0, offset).split('\n').length;
}

function checkDuplicateStructFields(file, source, failures) {
  for (const body of findStructBodies(source)) {
    const text = source.slice(body.start, body.end);
    const seen = new Map();
    for (const match of text.matchAll(/^\s*(?:pub(?:\([^)]*\))?\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*:/gm)) {
      const field = match[1];
      const absolute = body.start + match.index;
      if (seen.has(field)) {
        failures.push({ file, line: lineForOffset(source, absolute), message: `duplicate field '${field}' in struct ${body.name}` });
      } else {
        seen.set(field, absolute);
      }
    }
  }
}

function checkPatchRegressionPatterns(file, source, failures) {
  const mistakenAlias = 'mcp' + 'ays';
  if (source.includes(mistakenAlias)) {
    failures.push({ file, line: 1, message: 'speech-to-text alias regression: mistaken CLI alias must not exist' });
  }
  const lines = source.split('\n');
  for (let index = 1; index < lines.length; index += 1) {
    const previous = lines[index - 1].trim();
    const current = lines[index].trim();
    if (!previous || previous.length < 8 || previous === current && /^[{}()[\],;]+$/.test(current)) continue;
    if (previous === current && /=>\s*\{|return\s+|\.clone\(\)|\.max\(|\.min\(|JsonValue::/.test(current)) {
      failures.push({ file, line: index + 1, message: `suspicious duplicated Rust line: ${current.slice(0, 100)}` });
    }
  }
}

const json = process.argv.includes('--json');
const files = walk(path.join(repoRoot, 'src')).sort();
const failures = [];
for (const absolute of files) {
  const file = relative(absolute);
  const source = fs.readFileSync(absolute, 'utf8');
  checkBalancedBraces(file, source, failures);
  checkDuplicateStructFields(file, source, failures);
  checkPatchRegressionPatterns(file, source, failures);
}
const report = {
  schema: 'mcpace.rustStaticGuard.v1',
  generatedAt: new Date().toISOString(),
  checkedFiles: files.length,
  ok: failures.length === 0,
  failures,
};
if (json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
else if (report.ok) process.stdout.write(`PASS rust static guard: ${files.length} files\n`);
else process.stderr.write(failures.map((item) => `${item.file}:${item.line}: ${item.message}`).join('\n') + '\n');
process.exit(report.ok ? 0 : 1);
