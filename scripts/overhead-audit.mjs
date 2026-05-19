#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { repoRoot, deriveProjectName, deriveProjectVersion, readJson } from './lib/project-metadata.mjs';
import { resolveBinary } from '../packages/npm/cli/lib/resolve-binary.js';
import { cleanChildEnv } from './lib/safe-child-env.mjs';

const DEFAULT_RUNS = 7;
const DEFAULT_RESOLVE_RUNS = 250;

function monotonicMs() {
  return Number(process.hrtime.bigint()) / 1_000_000;
}

function parseArgs(argv) {
  const args = {
    json: false,
    write: 'reports/overhead-audit-latest.json',
    markdown: 'reports/overhead-audit-latest.md',
    runs: DEFAULT_RUNS,
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
      case '--runs': args.runs = parsePositiveInteger(readValue(), token); break;
      case '--help':
      case '-h': args.help = true; break;
      default: throw new Error(`unsupported overhead-audit argument: ${token}`);
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
  console.log(`Usage: node scripts/overhead-audit.mjs [--json] [--runs N]\n\nChecks runtime dependency footprint, dashboard source size, release manifest bloat,\nand Node launcher spawn overhead against the native binary on this host.`);
}

function fileSize(relativePath) {
  return fs.statSync(path.join(repoRoot, relativePath)).size;
}

function directorySize(relativePath) {
  const absolute = path.join(repoRoot, relativePath);
  if (!fs.existsSync(absolute)) return 0;
  let total = 0;
  const stack = [absolute];
  while (stack.length > 0) {
    const current = stack.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const child = path.join(current, entry.name);
      if (entry.isDirectory()) stack.push(child);
      else if (entry.isFile()) total += fs.statSync(child).size;
    }
  }
  return total;
}

function collectFileSizes() {
  const files = [
    'src/dashboard/index.html',
    'packages/npm/cli/bin/mcpace.js',
    'packages/npm/cli/lib/platform.js',
    'packages/npm/cli/lib/resolve-binary.js',
    'packages/npm/cli/lib/runtime.js',
    'packages/npm/cli/lib/targets.js',
    'scripts/playwright-dashboard-e2e.mjs',
    'tests/e2e/dashboard.playwright.spec.mjs',
    'tests/e2e/dashboard.parallel.playwright.spec.mjs'
  ].filter((relativePath) => fs.existsSync(path.join(repoRoot, relativePath)));
  return Object.fromEntries(files.map((relativePath) => [relativePath, fileSize(relativePath)]));
}

function runTimed(command, args, options = {}) {
  const started = monotonicMs();
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    encoding: 'utf8',
    timeout: 10_000,
    env: cleanChildEnv(),
    windowsHide: true,
    ...options
  });
  return {
    ok: !result.error && result.status === 0,
    status: result.status,
    error: result.error?.message || null,
    elapsedMs: monotonicMs() - started,
    stdout: String(result.stdout || '').trim().slice(-200),
    stderr: String(result.stderr || '').trim().slice(-200)
  };
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  if (sorted.length === 0) return null;
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

function percentile(values, percentileValue) {
  const sorted = [...values].sort((a, b) => a - b);
  if (sorted.length === 0) return null;
  const index = Math.min(sorted.length - 1, Math.ceil((percentileValue / 100) * sorted.length) - 1);
  return sorted[index];
}

function measureResolveBinaryOverhead(runs = DEFAULT_RESOLVE_RUNS) {
  const values = [];
  const failures = [];
  for (let index = 0; index < runs; index += 1) {
    const started = monotonicMs();
    try {
      resolveBinary();
      values.push((monotonicMs() - started) * 1000);
    } catch (error) {
      failures.push(error?.message || String(error));
    }
  }
  if (values.length === 0) {
    return { status: 'blocked', runs, failures: failures.length, reason: failures[0] || 'resolveBinary did not return a path' };
  }
  return {
    status: failures.length === 0 ? 'measured' : 'partial',
    runs,
    measuredRuns: values.length,
    failures: failures.length,
    medianUs: Number(median(values).toFixed(3)),
    p95Us: Number(percentile(values, 95).toFixed(3)),
    maxUs: Number(Math.max(...values).toFixed(3)),
    reason: failures[0] || null
  };
}

function measureLauncherOverhead(runs) {
  let binaryPath = null;
  try {
    binaryPath = resolveBinary();
  } catch (error) {
    return { status: 'blocked', reason: error.message, binaryPath: null, direct: [], launcher: [], explicitLauncher: [], deltaMs: null };
  }

  const launcher = path.join(repoRoot, 'packages/npm/cli/bin/mcpace.js');
  const direct = [];
  const launcherRuns = [];
  const explicitLauncherRuns = [];
  for (let index = 0; index < runs; index += 1) {
    direct.push(runTimed(binaryPath, ['--version']));
    launcherRuns.push(runTimed(process.execPath, [launcher, '--version']));
    explicitLauncherRuns.push(runTimed(process.execPath, [launcher, '--version'], {
      env: cleanChildEnv({ MCPACE_BINARY_PATH: binaryPath })
    }));
  }
  const summarize = (entries) => {
    const elapsed = entries.filter((run) => run.ok).map((run) => run.elapsedMs);
    if (elapsed.length === 0) return null;
    return {
      medianMs: Number(median(elapsed).toFixed(2)),
      p95Ms: Number(percentile(elapsed, 95).toFixed(2)),
      failures: entries.length - elapsed.length
    };
  };
  const directSummary = summarize(direct);
  const launcherSummary = summarize(launcherRuns);
  const explicitSummary = summarize(explicitLauncherRuns);
  if (!directSummary || !launcherSummary) {
    return {
      status: 'blocked',
      reason: 'native binary or npm launcher did not execute successfully on this host',
      binaryPath,
      direct,
      launcher: launcherRuns,
      explicitLauncher: explicitLauncherRuns,
      deltaMs: null
    };
  }
  return {
    status: 'measured',
    reason: null,
    binaryPath,
    runs,
    directMedianMs: directSummary.medianMs,
    directP95Ms: directSummary.p95Ms,
    launcherMedianMs: launcherSummary.medianMs,
    launcherP95Ms: launcherSummary.p95Ms,
    explicitLauncherMedianMs: explicitSummary?.medianMs ?? null,
    explicitLauncherP95Ms: explicitSummary?.p95Ms ?? null,
    deltaMs: Number((launcherSummary.medianMs - directSummary.medianMs).toFixed(2)),
    explicitDeltaMs: explicitSummary ? Number((explicitSummary.medianMs - directSummary.medianMs).toFixed(2)) : null,
    directFailures: directSummary.failures,
    launcherFailures: launcherSummary.failures,
    explicitLauncherFailures: explicitSummary?.failures ?? explicitLauncherRuns.length,
    recommendation: launcherSummary.medianMs - directSummary.medianMs > 100
      ? 'Avoid spawning the npm/Node wrapper for per-tool hot paths; keep MCPace as a long-lived hub process and use the native binary path for tight host benchmarks.'
      : 'Launcher overhead is small on this host.'
  };
}

function makeReport(args) {
  const rootPackage = readJson('package.json');
  const cliPackage = readJson('packages/npm/cli/package.json');
  const manifest = readJson('release-manifest.json');
  const fileSizes = collectFileSizes();
  const launcherOverhead = measureLauncherOverhead(args.runs);
  const resolveBinaryOverhead = measureResolveBinaryOverhead();
  const rootDeps = [
    ...Object.keys(rootPackage.dependencies || {}),
    ...Object.keys(rootPackage.devDependencies || {})
  ];
  const cliRuntimeDeps = Object.keys(cliPackage.dependencies || {});
  const optionalDeps = Object.keys(cliPackage.optionalDependencies || {});
  const manifestText = JSON.stringify(manifest);
  const checks = [
    {
      id: 'root-workspace-has-no-runtime-or-dev-dependency-bloat',
      ok: rootDeps.length === 0,
      evidence: `${rootDeps.length} dependencies/devDependencies in root package.json`
    },
    {
      id: 'npm-cli-has-no-runtime-dependencies',
      ok: cliRuntimeDeps.length === 0,
      evidence: `${cliRuntimeDeps.length} dependencies in packages/npm/cli/package.json`
    },
    {
      id: 'optional-platform-dependencies-only',
      ok: optionalDeps.length > 0 && optionalDeps.every((name) => name.startsWith('@mcpace/cli-')),
      evidence: optionalDeps.join(', ')
    },
    {
      id: 'playwright-is-test-only-temp-install',
      ok: !manifestText.includes('node_modules') && !JSON.stringify(rootPackage).includes('@playwright/test') && fs.readFileSync(path.join(repoRoot, 'scripts/playwright-dashboard-e2e.mjs'), 'utf8').includes('temporary npm install'),
      evidence: 'Playwright is not a runtime dependency and release manifest excludes node_modules'
    },
    {
      id: 'dashboard-source-footprint-under-100kb',
      ok: fileSizes['src/dashboard/index.html'] < 100_000,
      evidence: `${fileSizes['src/dashboard/index.html']} bytes`
    },
    {
      id: 'npm-launcher-source-footprint-under-20kb',
      ok: directorySize('packages/npm/cli/lib') + fileSize('packages/npm/cli/bin/mcpace.js') < 20_000,
      evidence: `${directorySize('packages/npm/cli/lib') + fileSize('packages/npm/cli/bin/mcpace.js')} bytes`
    },
    {
      id: 'launcher-overhead-measured-or-blocked-explicitly',
      ok: launcherOverhead.status === 'measured' || launcherOverhead.status === 'blocked',
      evidence: launcherOverhead.status === 'measured' ? `median delta ${launcherOverhead.deltaMs}ms` : launcherOverhead.reason
    },
    {
      id: 'resolve-binary-in-process-overhead-under-5ms-p95',
      ok: resolveBinaryOverhead.status === 'blocked' || resolveBinaryOverhead.p95Us < 5000,
      evidence: resolveBinaryOverhead.status === 'blocked' ? resolveBinaryOverhead.reason : `p95 ${resolveBinaryOverhead.p95Us}µs over ${resolveBinaryOverhead.measuredRuns} runs`
    },
    {
      id: 'launcher-overhead-not-severe-on-this-host',
      ok: launcherOverhead.status !== 'measured' || (launcherOverhead.deltaMs < 1000 && launcherOverhead.launcherMedianMs < 1500),
      evidence: launcherOverhead.status === 'measured' ? `launcher median ${launcherOverhead.launcherMedianMs}ms, delta ${launcherOverhead.deltaMs}ms` : 'not measured on this host'
    },
    {
      id: 'bounded-top-k-helper-shared',
      ok: fs.existsSync(path.join(repoRoot, 'scripts/lib/bounded-top-k.mjs'))
        && fs.readFileSync(path.join(repoRoot, 'scripts/simulate-tool-scale.mjs'), 'utf8').includes('./lib/bounded-top-k.mjs')
        && fs.readFileSync(path.join(repoRoot, 'scripts/simulate-mixed-upstreams.mjs'), 'utf8').includes('./lib/bounded-top-k.mjs'),
      evidence: 'Large tool-scale simulations use a shared bounded top-k helper instead of per-match full candidate sorting.'
    },
    {
      id: 'overhead-classifier-shared-policy',
      ok: fs.readFileSync(path.join(repoRoot, 'scripts/mcp-overhead-benchmark.mjs'), 'utf8').includes('./lib/mcp-signal-policy.mjs')
        && fs.readFileSync(path.join(repoRoot, 'scripts/mcp-overhead-stress.mjs'), 'utf8').includes('./lib/mcp-signal-policy.mjs'),
      evidence: 'Overhead benchmark/stress share the same signal policy library as package survey/profiling.'
    }
  ];
  const status = checks.every((check) => check.ok) ? 'pass' : 'fail';
  return {
    schema: 'mcpace.overheadAudit.v1',
    status,
    generatedAt: new Date().toISOString(),
    project: deriveProjectName(),
    version: deriveProjectVersion(),
    fileSizes,
    packageFootprint: {
      rootDependencyCount: rootDeps.length,
      cliRuntimeDependencyCount: cliRuntimeDeps.length,
      optionalPlatformDependencyCount: optionalDeps.length,
      cliSourceBytes: directorySize('packages/npm/cli/lib') + fileSize('packages/npm/cli/bin/mcpace.js'),
      vendoredBinaryBytes: directorySize('packages/npm/cli/vendor')
    },
    resolveBinaryOverhead,
    launcherOverhead,
    checks
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
  return `# Overhead audit

- Status: ${report.status}
- Generated: ${report.generatedAt}
- Project: ${report.project} ${report.version}
- CLI source bytes: ${report.packageFootprint.cliSourceBytes}
- Vendored binary bytes: ${report.packageFootprint.vendoredBinaryBytes}
- Launcher overhead: ${report.launcherOverhead.status === 'measured' ? `${report.launcherOverhead.deltaMs}ms median delta` : report.launcherOverhead.reason}
- Explicit binary launcher overhead: ${report.launcherOverhead.status === 'measured' ? `${report.launcherOverhead.explicitDeltaMs}ms median delta` : 'not measured'}
- In-process binary resolution p95: ${report.resolveBinaryOverhead.status === 'measured' ? `${report.resolveBinaryOverhead.p95Us}µs` : report.resolveBinaryOverhead.reason}

## Checks

| Check | OK | Evidence |
|---|---:|---|
${report.checks.map((check) => `| ${check.id} | ${check.ok ? 'yes' : 'no'} | ${String(check.evidence || '').replace(/\n/g, ' ')} |`).join('\n')}
`;
}

function main() {
  try {
    const args = parseArgs(process.argv.slice(2));
    if (args.help) {
      printHelp();
      return;
    }
    const report = makeReport(args);
    writeReport(report, args);
    if (args.json) console.log(JSON.stringify(report, null, 2));
    if (report.status !== 'pass') process.exitCode = 1;
  } catch (error) {
    console.error(error.message || error);
    process.exitCode = 1;
  }
}

main();
