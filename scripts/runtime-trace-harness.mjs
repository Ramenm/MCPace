#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { deriveProjectName, deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';

function parseArgs(argv) {
  const parsed = { json: false, write: null, markdown: null, binary: null, endpoint: null, strict: false, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--write': parsed.write = argv[++index] || null; if (!parsed.write) throw new Error('runtime-trace-harness requires a path after --write'); break;
      case '--markdown':
      case '--write-md': parsed.markdown = argv[++index] || null; if (!parsed.markdown) throw new Error('runtime-trace-harness requires a path after --markdown'); break;
      case '--binary': parsed.binary = argv[++index] || null; if (!parsed.binary) throw new Error('runtime-trace-harness requires a path after --binary'); break;
      case '--endpoint': parsed.endpoint = argv[++index] || null; if (!parsed.endpoint) throw new Error('runtime-trace-harness requires a URL after --endpoint'); break;
      case '--strict': parsed.strict = true; break;
      case '-h':
      case '--help': parsed.help = true; break;
      default: throw new Error(`unsupported runtime-trace-harness argument: ${token}`);
    }
  }
  return parsed;
}

function printHelp() {
  process.stdout.write('Usage: node scripts/runtime-trace-harness.mjs [--json] [--write <path>] [--markdown <path>] [--binary <path>] [--endpoint <url>] [--strict]\n\nCreates the runtime proof checklist and records why the real MCP client -> /mcp -> upstream tool trace is pass/blocked.\n');
}

function exists(relativePath) {
  return fs.existsSync(path.join(repoRoot, relativePath));
}

function defaultBinaryCandidates() {
  const exe = process.platform === 'win32' ? 'mcpace.exe' : 'mcpace';
  return [
    path.join(repoRoot, 'target', 'release', exe),
    path.join(repoRoot, 'packages', 'npm', 'cli', 'vendor', exe),
  ];
}

function buildReport(options) {
  const candidates = options.binary ? [path.resolve(options.binary)] : defaultBinaryCandidates();
  const presentBinary = candidates.find((candidate) => fs.existsSync(candidate)) || null;
  const endpoint = options.endpoint || 'http://127.0.0.1:39022/mcp';
  const tinyFixturePresent = exists('tests/fixtures/tiny-mcp-stdio-server.mjs') || exists('tests/fixtures/tiny-stdio-mcp-server.mjs') || exists('examples/tiny-stdio-mcp-server.mjs');
  const steps = [
    { id: 'binary', required: true, status: presentBinary ? 'pass' : 'blocked', evidence: presentBinary || candidates.map((candidate) => path.relative(repoRoot, candidate).split(path.sep).join('/')).join(', ') },
    { id: 'tiny-upstream-fixture', required: true, status: tinyFixturePresent ? 'pass' : 'blocked', evidence: 'tiny stdio MCP fixture for deterministic upstream_tools/upstream_call proof' },
    { id: 'serve-endpoint', required: true, status: 'manual', evidence: endpoint },
    { id: 'initialize', required: true, status: 'manual', evidence: 'POST JSON-RPC initialize with Accept: application/json, text/event-stream' },
    { id: 'tools-list', required: true, status: 'manual', evidence: 'POST JSON-RPC tools/list through MCPace' },
    { id: 'upstream-call', required: true, status: 'manual', evidence: 'POST JSON-RPC tools/call -> upstream_call against tiny stdio server' },
  ];
  const blockers = steps.filter((step) => step.status === 'blocked').map((step) => `${step.id}: ${step.evidence}`);
  return {
    schema: 'mcpace.runtimeTraceHarness.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status: blockers.length === 0 ? 'ready-to-run' : 'blocked',
    endpoint,
    binary: presentBinary ? path.relative(repoRoot, presentBinary).split(path.sep).join('/') : null,
    blockers,
    steps,
    nextCommands: [
      'cargo build --release --locked',
      'node scripts/runtime-trace-harness.mjs --json --write reports/runtime-trace-latest.json --markdown reports/runtime-trace-latest.md',
      './target/release/mcpace serve --port 39022',
      'Use MCP Inspector or a real client to run initialize -> tools/list -> tools/call -> upstream_call.',
    ],
  };
}

function renderMarkdown(report) {
  const lines = ['# MCPace runtime trace harness', '', `Project: \`${report.project.name}\` v\`${report.project.version}\``, `Status: \`${report.status}\``, '', '## Steps', '', '| step | status | evidence |', '|---|---:|---|'];
  for (const step of report.steps) lines.push(`| ${step.id} | ${step.status} | ${String(step.evidence).replace(/\|/g, '\\|')} |`);
  if (report.blockers.length > 0) {
    lines.push('', '## Blockers', '');
    for (const blocker of report.blockers) lines.push(`- ${blocker}`);
  }
  lines.push('', '## Next commands', '');
  for (const command of report.nextCommands) lines.push(`- \`${command}\``);
  lines.push('');
  return lines.join('\n');
}

function writeFileEnsuringDir(filePath, contents) {
  const target = path.resolve(repoRoot, filePath);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, contents, 'utf8');
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    if (parsed.help) { printHelp(); return; }
    const report = buildReport(parsed);
    if (parsed.write) writeFileEnsuringDir(parsed.write, `${JSON.stringify(report, null, 2)}\n`);
    if (parsed.markdown) writeFileEnsuringDir(parsed.markdown, renderMarkdown(report));
    if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else process.stdout.write(`[mcpace runtime-trace] ${report.status}\n`);
    if (parsed.strict && report.status !== 'ready-to-run') process.exit(1);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();
