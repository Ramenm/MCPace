import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

function readText(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

test('import code accepts common MCP URL aliases and normalizes remote type', () => {
  const source = readText('src/mcp_sources/import.rs');
  for (const key of ['serverUrl', 'httpUrl', 'endpoint']) {
    assert.match(source, new RegExp(`"${key}"`), `missing URL alias support for ${key}`);
  }
  assert.match(source, /source_type::infer_public_source_type/, 'remote URL imports should use the shared source-type normalizer');
  assert.match(source, /"servers"/, 'VS Code-style top-level servers object should remain supported');
  assert.match(source, /"mcpServers"/, 'mcpServers object should remain supported');
});

test('home import does not add default upstream servers and skips MCPace loops', () => {
  const setup = readText('src/setup.rs');
  assert.match(setup, /did not add a default filesystem server/i);
  assert.match(setup, /auto-imported-home\.json/);
  assert.match(setup, /normalized_name == "mcpace" \|\| normalized_name == "mcp-pace"/);
  assert.doesNotMatch(setup, /normalized_name == "mcp-ace"/);
});

test('public help stays compact and install type inference remains documented', () => {
  const app = readText('src/app.rs');
  const help = app.slice(app.indexOf('fn write_help'));
  const usageLines = [...help.matchAll(/writeln!\(stdout, "  mcpace /g)].length;
  assert.ok(usageLines <= 8, `help should keep visible commands compact, saw ${usageLines} mcpace lines`);
  assert.match(help, /Server type is inferred/);
  assert.match(help, /It does not add a default upstream server/);
});
