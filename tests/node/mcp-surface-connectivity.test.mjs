import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { repoRoot } from '../../scripts/lib/project-metadata.mjs';

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function sortedUnique(values) {
  return [...new Set(values)].sort((a, b) => a.localeCompare(b));
}

function httpToolDefinitions() {
  const source = read('src/dashboard/http_tools.rs');
  return sortedUnique(
    [...source.matchAll(/http_tool(?:_with_schema)?\(\s*"([a-z][a-z0-9_]*)"/g)].map((match) => match[1]),
  );
}

function stdioToolDefinitions() {
  const source = read('src/mcp_server/tool_surface.rs');
  return sortedUnique([...source.matchAll(/name:\s*"([a-z][a-z0-9_]*)"/g)].map((match) => match[1]));
}

test('HTTP MCP tool annotations do not reference dead tools', () => {
  const definitions = new Set(httpToolDefinitions());
  const source = read('src/dashboard/http_tools.rs');
  const annotationsStart = source.indexOf('fn http_tool_annotations');
  const annotationsEnd = source.indexOf('fn http_tool_with_schema');
  assert.ok(annotationsStart >= 0 && annotationsEnd > annotationsStart, 'annotations function should be present');
  const annotations = source.slice(annotationsStart, annotationsEnd);
  const annotated = sortedUnique(
    [...annotations.matchAll(/"([a-z][a-z0-9_]*?)"/g)].map((match) => match[1]),
  );
  const dead = annotated.filter((name) => !definitions.has(name));
  assert.deepEqual(dead, [], `annotated HTTP tools must be present in tools/list definitions: ${dead.join(', ')}`);
});

test('HTTP and stdio MCP tools are both dispatched by their runtime surfaces', () => {
  const httpRuntime = read('src/dashboard/tool_runtime.rs');
  const stdioRuntime = read('src/mcp_server.rs');
  const missingHttp = httpToolDefinitions().filter((name) => !httpRuntime.includes(`"${name}"`));
  const missingStdio = stdioToolDefinitions().filter((name) => !stdioRuntime.includes(`"${name}"`));
  assert.deepEqual(missingHttp, [], `HTTP tools without dispatch: ${missingHttp.join(', ')}`);
  assert.deepEqual(missingStdio, [], `stdio tools without dispatch: ${missingStdio.join(', ')}`);
});

test('runtime flow inventory records the intentional HTTP/stdio surface delta', () => {
  const inventory = JSON.parse(read('reports/internal-inventory.json'));
  assert.equal(inventory.runtimeFlow.schema, 'mcpace.runtimeFlowAudit.v1');
  assert.deepEqual(inventory.runtimeFlow.dispatchCoverage.missingHttpDispatch, []);
  assert.deepEqual(inventory.runtimeFlow.dispatchCoverage.missingStdioDispatch, []);
  assert.deepEqual(inventory.runtimeFlow.surfaces.onlyStdio, [
    'runtime_acquire',
    'runtime_release',
    'runtime_renew',
  ]);
  assert.deepEqual(inventory.runtimeFlow.surfaces.onlyHttp, [
    'hub_repair',
    'runtime_diagnostics',
  ]);
});
