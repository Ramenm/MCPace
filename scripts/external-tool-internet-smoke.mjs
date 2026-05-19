#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import { repoRoot, deriveProjectName, deriveProjectVersion } from './lib/project-metadata.mjs';

const DEFAULT_TIMEOUT_MS = 10_000;

const TOOL_SCENARIOS = [
  {
    id: 'local-filesystem',
    category: 'local-only',
    examples: ['filesystem MCP server', 'local git MCP server'],
    internet: 'none required after the binary/package is present',
    risks: ['local data access', 'path traversal policy', 'large file scanning'],
    expectedDefault: 'register disabled or least-scope path allowlist'
  },
  {
    id: 'npx-package-launch',
    category: 'package-manager',
    examples: ['@modelcontextprotocol/server-filesystem', 'brave-search-mcp-server'],
    internet: 'npm registry may be used on first run or version change',
    risks: ['dependency chain abuse', 'postinstall scripts', 'version drift', 'registry outage'],
    expectedDefault: 'dry-run registration, pinned package, no tool call during install'
  },
  {
    id: 'uvx-python-launch',
    category: 'package-manager',
    examples: ['Python SDK-based MCP servers'],
    internet: 'PyPI may be used on first run or version change',
    risks: ['version drift', 'native wheel availability', 'Python environment mismatch'],
    expectedDefault: 'treat launch as runtime install, not MCPace install'
  },
  {
    id: 'docker-image-launch',
    category: 'container-runtime',
    examples: ['containerized MCP server'],
    internet: 'container registry may be used on first pull',
    risks: ['image tag drift', 'privileged mounts', 'network and filesystem blast radius'],
    expectedDefault: 'pin digest and keep mounts read-only by default'
  },
  {
    id: 'github-api',
    category: 'external-api',
    examples: ['GitHub MCP server'],
    internet: 'requires GitHub API reachability and usually a token',
    risks: ['token scope', 'rate limits', 'private repository data exposure'],
    expectedDefault: 'disabled until token + repo scope reviewed'
  },
  {
    id: 'brave-search-api',
    category: 'external-api',
    examples: ['Brave Search MCP server'],
    internet: 'requires Brave Search API reachability and API key for real queries',
    risks: ['paid quota', 'API key leakage', 'unexpected search costs'],
    expectedDefault: 'disabled until API key and budget reviewed'
  },
  {
    id: 'fetch-web',
    category: 'external-web',
    examples: ['Fetch/webpage MCP server'],
    internet: 'requires arbitrary web access',
    risks: ['SSRF-like behavior', 'unexpected large downloads', 'untrusted content ingestion'],
    expectedDefault: 'host/domain allowlist and response-size limit'
  },
  {
    id: 'remote-streamable-http',
    category: 'remote-mcp',
    examples: ['third-party Streamable HTTP MCP server'],
    internet: 'requires remote domain reachability and often auth',
    risks: ['domain ownership confusion', 'auth token audience mismatch', 'remote downtime'],
    expectedDefault: 'explicit owned/not-owned labeling and auth review'
  }
];

const LIVE_ENDPOINTS = [
  {
    id: 'mcp-docs',
    url: 'https://modelcontextprotocol.io/examples',
    required: true,
    expectedStatuses: [200]
  },
  {
    id: 'npm-registry-mcp-sdk',
    url: 'https://registry.npmjs.org/@modelcontextprotocol%2Fsdk',
    required: true,
    expectedStatuses: [200]
  },
  {
    id: 'pypi-mcp',
    url: 'https://pypi.org/pypi/mcp/json',
    required: false,
    expectedStatuses: [200, 404]
  },
  {
    id: 'github-api',
    url: 'https://api.github.com/rate_limit',
    required: true,
    expectedStatuses: [200, 403]
  },
  {
    id: 'github-mcp-servers-repo',
    url: 'https://github.com/modelcontextprotocol/servers',
    required: false,
    expectedStatuses: [200]
  }
];

function parseArgs(argv) {
  const args = {
    json: false,
    liveInternet: false,
    write: 'reports/external-tool-internet-latest.json',
    markdown: 'reports/external-tool-internet-latest.md',
    timeoutMs: DEFAULT_TIMEOUT_MS,
    noWrite: false,
    help: false,
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
      case '--live-internet': args.liveInternet = true; break;
      case '--write': args.write = readValue(); break;
      case '--markdown': args.markdown = readValue(); break;
      case '--timeout-ms': args.timeoutMs = parsePositiveInteger(readValue(), token); break;
      case '--no-write': args.write = null; args.markdown = null; break;
      case '--help':
      case '-h': args.help = true; break;
      default: throw new Error(`unsupported external-tool-internet-smoke argument: ${token}`);
    }
  }
  return args;
}

function parsePositiveInteger(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${label} must be a positive integer`);
  return parsed;
}

function printHelp() {
  console.log(`Usage: node scripts/external-tool-internet-smoke.mjs [options]

Builds a scenario matrix for popular MCP launch/tool classes. By default it is
source-only and never contacts external services. Pass --live-internet to check
basic DNS/HTTPS/API reachability for public docs/registry/API endpoints without
running third-party MCP packages or sending credentials.

Options:
  --live-internet        Perform live HTTPS reachability checks.
  --timeout-ms <ms>      Per-endpoint timeout. Default ${DEFAULT_TIMEOUT_MS}
  --write <path>         JSON report path.
  --markdown <path>      Markdown report path.
  --no-write             Do not write reports.
  --json                 Print JSON report.
`);
}

async function fetchEndpoint(endpoint, timeoutMs) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  const started = performance.now();
  try {
    const response = await fetch(endpoint.url, {
      method: 'GET',
      signal: controller.signal,
      headers: {
        'user-agent': 'mcpace-external-tool-internet-smoke/1.0',
        accept: 'application/json,text/html;q=0.8,*/*;q=0.5'
      }
    });
    const elapsedMs = performance.now() - started;
    await response.body?.cancel?.();
    return {
      id: endpoint.id,
      url: endpoint.url,
      required: endpoint.required,
      statusCode: response.status,
      ok: endpoint.expectedStatuses.includes(response.status),
      expectedStatuses: endpoint.expectedStatuses,
      elapsedMs: Number(elapsedMs.toFixed(2)),
    };
  } catch (error) {
    const elapsedMs = performance.now() - started;
    return {
      id: endpoint.id,
      url: endpoint.url,
      required: endpoint.required,
      statusCode: null,
      ok: false,
      expectedStatuses: endpoint.expectedStatuses,
      elapsedMs: Number(elapsedMs.toFixed(2)),
      error: error?.name === 'AbortError' ? 'timeout' : (error?.message || String(error))
    };
  } finally {
    clearTimeout(timeout);
  }
}

function scenarioChecks() {
  const categories = new Set(TOOL_SCENARIOS.map((item) => item.category));
  return [
    {
      id: 'covers-local-only-tools',
      ok: categories.has('local-only'),
      evidence: 'filesystem/git-style local tools'
    },
    {
      id: 'covers-package-manager-launchers',
      ok: categories.has('package-manager') && categories.has('container-runtime'),
      evidence: 'npx, uvx, docker'
    },
    {
      id: 'covers-external-api-tools',
      ok: categories.has('external-api') && categories.has('external-web'),
      evidence: 'GitHub, Brave Search, Fetch/web'
    },
    {
      id: 'covers-remote-mcp-transport',
      ok: categories.has('remote-mcp'),
      evidence: 'Streamable HTTP third-party domain'
    },
    {
      id: 'does-not-execute-third-party-packages',
      ok: true,
      evidence: 'matrix + optional HTTPS reachability only; no npx/uvx/docker MCP package is launched'
    }
  ];
}

async function makeReport(args) {
  const liveResults = args.liveInternet
    ? await Promise.all(LIVE_ENDPOINTS.map((endpoint) => fetchEndpoint(endpoint, args.timeoutMs)))
    : [];
  const requiredLiveResults = liveResults.filter((result) => result.required);
  const liveRequiredOk = requiredLiveResults.every((result) => result.ok);
  const liveAllRequiredNetworkBlocked = args.liveInternet
    && requiredLiveResults.length > 0
    && requiredLiveResults.every((result) => !result.ok && /fetch failed|timeout|getaddrinfo|ENOTFOUND|ECONN|network/i.test(result.error || ''));
  const checks = [
    ...scenarioChecks(),
    {
      id: 'live-internet-mode',
      ok: args.liveInternet ? liveRequiredOk : true,
      evidence: args.liveInternet ? 'required endpoints checked' : 'not requested; pass --live-internet for DNS/HTTPS checks'
    }
  ];
  const status = checks.every((check) => check.ok) ? 'pass' : liveAllRequiredNetworkBlocked ? 'blocked' : 'fail';
  return {
    schema: 'mcpace.externalToolInternetSmoke.v1',
    status,
    generatedAt: new Date().toISOString(),
    project: deriveProjectName(),
    version: deriveProjectVersion(),
    mode: args.liveInternet ? 'live-internet' : 'source-only',
    scenarios: TOOL_SCENARIOS,
    liveEndpoints: LIVE_ENDPOINTS,
    liveResults,
    checks,
    notes: [
      'This smoke does not execute third-party MCP packages.',
      'Paid/API-key tools must remain disabled until credentials, quota, and owner/domain are reviewed.',
      'Package manager launchers are runtime install surfaces, not pure MCPace registration.',
      ...(liveAllRequiredNetworkBlocked ? ['Direct live internet appears blocked by the current host/network policy.'] : [])
    ]
  };
}

function writeReport(report, args) {
  if (args.write) {
    const output = path.join(repoRoot, args.write);
    fs.mkdirSync(path.dirname(output), { recursive: true });
    fs.writeFileSync(output, JSON.stringify(report, null, 2) + '\n');
  }
  if (args.markdown) {
    const output = path.join(repoRoot, args.markdown);
    fs.mkdirSync(path.dirname(output), { recursive: true });
    fs.writeFileSync(output, renderMarkdown(report));
  }
}

function renderMarkdown(report) {
  return `# External MCP tool and internet smoke

- Status: ${report.status}
- Mode: ${report.mode}
- Generated: ${report.generatedAt}
- Project: ${report.project} ${report.version}

## Scenario matrix

| ID | Category | Internet | Risks | Default posture |
|---|---|---|---|---|
${report.scenarios.map((scenario) => `| ${scenario.id} | ${scenario.category} | ${scenario.internet} | ${scenario.risks.join('; ')} | ${scenario.expectedDefault} |`).join('\n')}

## Live results

${report.liveResults.length ? `| Endpoint | Required | Status | OK | Elapsed |
|---|---:|---:|---:|---:|
${report.liveResults.map((result) => `| ${result.id} | ${result.required ? 'yes' : 'no'} | ${result.statusCode ?? result.error ?? 'error'} | ${result.ok ? 'yes' : 'no'} | ${result.elapsedMs}ms |`).join('\n')}` : 'Live internet checks were not requested. Run `npm run verify:external-tool-internet:live` to check public docs/registry/API reachability.'}

## Checks

| Check | OK | Evidence |
|---|---:|---|
${report.checks.map((check) => `| ${check.id} | ${check.ok ? 'yes' : 'no'} | ${check.evidence} |`).join('\n')}
`;
}

async function main() {
  try {
    const args = parseArgs(process.argv.slice(2));
    if (args.help) {
      printHelp();
      return;
    }
    const report = await makeReport(args);
    writeReport(report, args);
    if (args.json) console.log(JSON.stringify(report, null, 2));
    if (report.status !== 'pass') process.exitCode = report.status === 'blocked' ? 2 : 1;
  } catch (error) {
    console.error(error.message || error);
    process.exitCode = 1;
  }
}

main();
