import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';


function splitRustParams(params) {
  const parts = [];
  let depth = 0;
  let start = 0;
  for (let index = 0; index < params.length; index += 1) {
    const char = params[index];
    if ('<([{'.includes(char)) depth += 1;
    if ('>)]}'.includes(char)) depth = Math.max(0, depth - 1);
    if (char === ',' && depth === 0) {
      parts.push(params.slice(start, index));
      start = index + 1;
    }
  }
  parts.push(params.slice(start));
  return parts;
}

function paramName(param) {
  const cleaned = param.trim();
  if (!cleaned || cleaned === '&self' || cleaned === 'self' || cleaned === '&mut self') return null;
  const [name] = cleaned.split(':');
  return name
    .trim()
    .replace(/^mut\s+/, '')
    .replace(/^_+/, '')
    .trim() || null;
}

function walkRustFiles(dir) {
  const files = [];
  const stack = [dir];
  while (stack.length > 0) {
    const current = stack.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        stack.push(full);
      } else if (entry.isFile() && entry.name.endsWith('.rs')) {
        files.push(full);
      }
    }
  }
  return files.sort();
}

test('Rust source has no adjacent duplicate match arms', () => {
  const offenders = [];
  for (const file of walkRustFiles(path.join(repoRoot, 'src'))) {
    const lines = fs.readFileSync(file, 'utf8').split(/\r?\n/);
    for (let index = 0; index < lines.length - 1; index += 1) {
      const current = lines[index].trim();
      const next = lines[index + 1].trim();
      if (current === next && current.includes('=>')) {
        offenders.push(`${path.relative(repoRoot, file)}:${index + 1}:${current}`);
      }
    }
  }
  assert.deepEqual(offenders, []);
});


test('Rust function signatures do not repeat parameter names', () => {
  const offenders = [];
  const signaturePattern = /fn\s+([A-Za-z0-9_]+)\s*\(([\s\S]*?)\)\s*(?:->|\{|where\b)/g;
  for (const file of walkRustFiles(path.join(repoRoot, 'src'))) {
    const source = fs.readFileSync(file, 'utf8');
    for (const match of source.matchAll(signaturePattern)) {
      const names = splitRustParams(match[2]).map(paramName).filter(Boolean);
      const seen = new Set();
      for (const name of names) {
        if (seen.has(name)) {
          offenders.push(`${path.relative(repoRoot, file)}:${match[1]}:${name}`);
        }
        seen.add(name);
      }
    }
  }
  assert.deepEqual(offenders, []);
});
