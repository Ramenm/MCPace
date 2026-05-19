#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import { repoRoot, deriveProjectName, deriveProjectVersion, readJson, readText } from './lib/project-metadata.mjs';

function parseArgs(argv) {
  const args = {
    json: false,
    write: 'reports/multi-client-runtime-audit-latest.json',
    markdown: 'reports/multi-client-runtime-audit-latest.md',
    help: false
  };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    const readValue = () => {
      const value = argv[index + 1];
      if (!value || value.startsWith('--')) throw new Error(`${token} requires a value`);
      index += 1;
      return value;
    };
    switch (token) {
      case '--json': args.json = true; break;
      case '--write': args.write = readValue(); break;
      case '--markdown': args.markdown = readValue(); break;
      case '--no-write': args.write = null; args.markdown = null; break;
      case '--help':
      case '-h': args.help = true; break;
      default: throw new Error(`unsupported multi-client-runtime-audit argument: ${token}`);
    }
  }
  return args;
}

function printHelp() {
  console.log(`Usage: node scripts/multi-client-runtime-audit.mjs [--json] [--write PATH] [--markdown PATH]\n\nChecks source-level multi-client/session isolation, upstream pool sharding,\nPlaywright multi-context coverage, and documented automatic limits.`);
}

function fileExists(relativePath) {
  return fs.existsSync(path.join(repoRoot, relativePath));
}

function readMaybe(relativePath) {
  return fileExists(relativePath) ? readText(relativePath) : '';
}

function numericConst(source, name) {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = String(source).match(new RegExp(`const\\s+${escaped}\\s*:\\s*usize\\s*=\\s*(\\d+)\\s*;`));
  return match ? Number.parseInt(match[1], 10) : null;
}

function hasAll(source, needles) {
  return needles.every((needle) => String(source).includes(needle));
}

function addCheck(checks, id, ok, severity, evidence, recommendation = '') {
  checks.push({ id, ok: Boolean(ok), severity, evidence, recommendation });
}

function checkSources() {
  const started = performance.now();
  const resources = readMaybe('src/resources.rs');
  const dashboard = readMaybe('src/dashboard.rs');
  const httpSession = readMaybe('src/dashboard/http_session.rs');
  const mcpHttp = readMaybe('src/dashboard/mcp_http.rs');
  const toolRuntime = readMaybe('src/dashboard/tool_runtime.rs');
  const context = readMaybe('src/client/context.rs');
  const leases = readMaybe('src/hub/leases.rs');
  const playwrightSpec = readMaybe('tests/e2e/dashboard.parallel.playwright.spec.mjs');
  const playwrightConfig = readMaybe('tests/e2e/playwright.config.mjs');
  const playwrightWrapper = readMaybe('scripts/playwright-dashboard-e2e.mjs');
  const universalRuntimeDoc = readMaybe('docs/universal-runtime-policy.md');
  const browserDoc = readMaybe('docs/browser-e2e-and-external-tooling.md');
  const packageJson = readJson('package.json');

  const poolMax = numericConst(resources, 'AUTO_UPSTREAM_SESSION_POOL_MAX');
  const shardMax = numericConst(resources, 'AUTO_UPSTREAM_SESSION_SHARD_MAX');
  const checks = [];

  addCheck(
    checks,
    'http-streamable-session-id-is-generated-and-required',
    hasAll(httpSession, ['generated_mcp_http_session_id', 'getrandom::getrandom', 'mcp-session-id']) && /missing required mcp-session-id/i.test(httpSession),
    'critical',
    'src/dashboard/http_session.rs generates OS-random MCP HTTP session ids and rejects missing stateful session headers.',
    'Keep HTTP session ids server-issued and fail closed when a stateful request omits them.'
  );

  addCheck(
    checks,
    'http-upstream-context-keeps-client-session-project-identity',
    hasAll(toolRuntime, ['http_upstream_lease_context', 'mcp-session-id', 'x-mcp-session-id', 'x-mcp-client-id', 'project_root']) &&
      hasAll(mcpHttp, ['handle_mcp_http_request', 'MCP-Protocol-Version']),
    'critical',
    'src/dashboard/tool_runtime.rs builds upstream lease context from MCP/forwarded headers, metadata, and project roots.',
    'Do not collapse HTTP clients into anonymous upstream sessions after Streamable HTTP initialization.'
  );

  addCheck(
    checks,
    'upstream-pool-shards-by-client-session-project-transport',
    hasAll(dashboard, ['fn new_upstream_session_pools', 'fn upstream_pool_for_context', 'context.client_id.hash', 'context.session_id.hash', 'context.project_root.hash', 'context.transport.hash']),
    'high',
    'src/dashboard.rs hashes server, client id, session id, project root, and transport when selecting an upstream pool shard.',
    'Keep the shard selector aligned with the upstream session key fields.'
  );

  addCheck(
    checks,
    'default-upstream-pool-allows-bounded-multiclient-distribution',
    Number.isSafeInteger(poolMax) && poolMax >= 4 && Number.isSafeInteger(shardMax) && shardMax >= 2,
    'high',
    `src/resources.rs AUTO_UPSTREAM_SESSION_POOL_MAX=${poolMax ?? 'missing'}, AUTO_UPSTREAM_SESSION_SHARD_MAX=${shardMax ?? 'missing'}.`,
    'Use env overrides for host-specific high-concurrency tests; keep source defaults bounded.'
  );

  addCheck(
    checks,
    'stdio-fallback-limit-is-visible-not-silent',
    hasAll(context, ['resolve_session_lease', 'conversation_id', 'client_instance_id', 'transport_session_id']) &&
      /multiple live instances of the same client/i.test(context),
    'high',
    'src/client/context.rs derives stable planned leases but warns when no external session/conversation/client-instance/transport-session id exists.',
    'For strict stdio multi-client isolation, clients should pass --session-id, MCPACE_SESSION_ID, MCPACE_CLIENT_INSTANCE_ID, or metadata.'
  );

  addCheck(
    checks,
    'hub-leases-block-conflicting-client-work',
    hasAll(leases, ['find_conflict', 'requestMutexKey', 'capacityKey', 'parallelismLimit', 'takeover_allowed']) &&
      /session_lease_id|sessionLeaseId/.test(leases),
    'critical',
    'src/hub/leases.rs enforces request mutex/capacity lanes and same-session takeover rules.',
    'Keep server policy as the source of truth for mutable resource serialization.'
  );

  addCheck(
    checks,
    'playwright-covers-parallel-independent-client-contexts',
    hasAll(playwrightSpec, ['test.describe.configure({ mode: \'parallel\' })', 'browser.newContext', '__mcpaceClientSession', 'MCPACE_PLAYWRIGHT_STATE_DIR']) &&
      hasAll(playwrightConfig, ['fullyParallel: true', 'MCPACE_PLAYWRIGHT_WORKERS']) &&
      hasAll(playwrightWrapper, ['parallelState', 'workerCount', 'conflicts']),
    'medium',
    'Playwright lane uses separate BrowserContexts, parallel worker config, and recorded conflict evidence.',
    'Run this lane on CI with at least two workers and a real browser.'
  );

  addCheck(
    checks,
    'package-scripts-wire-multiclient-audit-into-experience',
    packageJson.scripts?.['verify:multi-client-runtime']?.includes('multi-client-runtime-audit.mjs') &&
      packageJson.scripts?.['verify:browser-experience']?.includes('verify:multi-client-runtime') &&
      packageJson.scripts?.['verify:experience']?.includes('verify:multi-client-runtime'),
    'medium',
    'package.json exposes verify:multi-client-runtime and includes it in browser/experience verification.',
    'Keep this source-only audit cheap enough to run in regular prepublish/source checks.'
  );

  addCheck(
    checks,
    'docs-explain-automatic-versus-required-client-identity',
    /not fully automatic/i.test(universalRuntimeDoc + browserDoc) &&
      /MCPACE_SESSION_ID/.test(universalRuntimeDoc + browserDoc) &&
      /browser.newContext/.test(browserDoc),
    'medium',
    'Docs distinguish HTTP automatic sessioning from stdio/client metadata requirements and Playwright context isolation.',
    'Do not imply MCPace can invent strict identity for clients that send no distinguishing signal.'
  );

  const failures = checks.filter((check) => !check.ok);
  const acceptedLimits = [
    {
      id: 'stdio-clients-without-any-session-signal',
      severity: 'medium',
      status: 'accepted-limit',
      summary: 'MCPace can derive a stable planned lease, but it cannot prove two same-client/same-project stdio processes are separate unless the client supplies a session/conversation/client-instance/transport-session signal.'
    },
    {
      id: 'source-audit-not-live-rust-concurrency-proof',
      severity: 'medium',
      status: 'not-proven-here',
      summary: 'This audit confirms source contracts and browser E2E wiring. Rust runtime parallel throughput still needs cargo build/test and live-host concurrency measurement.'
    }
  ];

  return {
    schema: 'mcpace.multiClientRuntimeAudit.v1',
    status: failures.length === 0 ? 'pass' : 'fail',
    generatedAt: new Date().toISOString(),
    elapsedMs: Math.round((performance.now() - started) * 1000) / 1000,
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    summary: {
      checks: checks.length,
      passed: checks.length - failures.length,
      failed: failures.length,
      poolMax,
      shardMax
    },
    checks,
    acceptedLimits,
    filesReviewed: [
      'src/resources.rs',
      'src/dashboard.rs',
      'src/dashboard/http_session.rs',
      'src/dashboard/mcp_http.rs',
      'src/dashboard/tool_runtime.rs',
      'src/client/context.rs',
      'src/hub/leases.rs',
      'tests/e2e/dashboard.parallel.playwright.spec.mjs',
      'tests/e2e/playwright.config.mjs',
      'scripts/playwright-dashboard-e2e.mjs',
      'docs/universal-runtime-policy.md',
      'docs/browser-e2e-and-external-tooling.md'
    ]
  };
}

function renderMarkdown(report) {
  const lines = [];
  lines.push(`# Multi-client runtime audit`);
  lines.push('');
  lines.push(`Generated: ${report.generatedAt}`);
  lines.push('');
  lines.push(`Status: **${report.status}**`);
  lines.push('');
  lines.push(`Project: ${report.project.name} v${report.project.version}`);
  lines.push('');
  lines.push(`Default upstream pool max: ${report.summary.poolMax ?? 'unknown'}`);
  lines.push(`Default upstream shard max: ${report.summary.shardMax ?? 'unknown'}`);
  lines.push('');
  lines.push('| Check | Severity | Result | Evidence |');
  lines.push('|---|---:|---:|---|');
  for (const check of report.checks) {
    lines.push(`| ${check.id} | ${check.severity} | ${check.ok ? 'pass' : 'fail'} | ${check.evidence.replace(/\|/g, '\\|')} |`);
  }
  lines.push('');
  lines.push('## Accepted limits / not proven here');
  lines.push('');
  for (const item of report.acceptedLimits) {
    lines.push(`- **${item.id}** (${item.severity}, ${item.status}): ${item.summary}`);
  }
  lines.push('');
  lines.push('## Files reviewed');
  lines.push('');
  for (const file of report.filesReviewed) lines.push(`- ${file}`);
  lines.push('');
  return `${lines.join('\n')}\n`;
}

function writeFile(relativePath, content) {
  const absolute = path.join(repoRoot, relativePath);
  fs.mkdirSync(path.dirname(absolute), { recursive: true });
  fs.writeFileSync(absolute, content);
}

try {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printHelp();
  } else {
    const report = checkSources();
    if (args.write) writeFile(args.write, `${JSON.stringify(report, null, 2)}\n`);
    if (args.markdown) writeFile(args.markdown, renderMarkdown(report));
    if (args.json) console.log(JSON.stringify(report, null, 2));
    else console.log(`multi-client runtime audit: ${report.status} (${report.summary.passed}/${report.summary.checks})`);
    process.exitCode = report.status === 'pass' ? 0 : 1;
  }
} catch (error) {
  const payload = {
    schema: 'mcpace.multiClientRuntimeAudit.v1',
    status: 'error',
    generatedAt: new Date().toISOString(),
    error: error instanceof Error ? error.message : String(error)
  };
  console.error(JSON.stringify(payload, null, 2));
  process.exit(1);
}
