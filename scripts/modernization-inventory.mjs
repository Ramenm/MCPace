#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { repoRoot as defaultRepoRoot } from './lib/project-metadata.mjs';
import { cargoLockRefreshFindings, cargoLockRefreshMessage } from './lib/cargo-policy.mjs';

const SKIP_DIRS = new Set(['.git', 'node_modules', 'target', 'dist', '.artifacts']);

function parseArgs(argv) {
  const args = { json: false, repoRoot: defaultRepoRoot };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--json') args.json = true;
    else if (arg === '--repo') args.repoRoot = path.resolve(argv[++index]);
    else if (arg === '--help' || arg === '-h') {
      console.log('Usage: node scripts/modernization-inventory.mjs [--json] [--repo DIR]');
      process.exit(0);
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  return args;
}

function normalize(file) {
  return file.split(path.sep).join('/');
}

function walkFiles(root, predicate = () => true) {
  const files = [];
  const stack = [root];
  while (stack.length > 0) {
    const current = stack.pop();
    if (!fs.existsSync(current)) continue;
    for (const entry of fs.readdirSync(current, { withFileTypes: true }).sort((left, right) => left.name.localeCompare(right.name))) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        if (!SKIP_DIRS.has(entry.name)) stack.push(full);
      } else if (entry.isFile() && predicate(full)) {
        files.push(full);
      }
    }
  }
  return files.sort();
}

function finding(id, severity, title, files, recommendation, replacement = null) {
  return {
    id,
    severity,
    title,
    count: files.length,
    files: files.slice(0, 50),
    truncated: files.length > 50,
    replacement,
    recommendation,
  };
}

function text(file) {
  return fs.readFileSync(file, 'utf8');
}

function grep(files, pattern) {
  return files
    .filter((file) => pattern.test(text(file)))
    .map((file) => normalize(path.relative(currentRepoRoot, file)));
}

let currentRepoRoot = defaultRepoRoot;

function run() {
  const args = parseArgs(process.argv.slice(2));
  currentRepoRoot = args.repoRoot;
  const cargoToml = path.join(args.repoRoot, 'Cargo.toml');
  const rustFiles = walkFiles(path.join(args.repoRoot, 'src'), (file) => file.endsWith('.rs'));
  const jsFiles = walkFiles(path.join(args.repoRoot, 'scripts'), (file) => file.endsWith('.mjs'));
  const testFiles = walkFiles(path.join(args.repoRoot, 'tests'), (file) => file.endsWith('.mjs'));
  const allTextFiles = [...rustFiles, ...jsFiles, ...testFiles, cargoToml].filter((file) => fs.existsSync(file));
  const findings = [];

  const cargo = fs.existsSync(cargoToml) ? text(cargoToml) : '';
  const pathDependencies = [...cargo.matchAll(/^([A-Za-z0-9_-]+)\s*=\s*\{\s*path\s*=\s*"([^"]+)"/gm)]
    .map((match) => ({ crate: match[1], path: match[2] }));
  if (pathDependencies.length > 0) {
    findings.push({
      id: 'cargo-path-compat-dependencies',
      severity: 'high',
      title: 'Cargo.toml still uses local compatibility crates that shadow standard crates',
      count: pathDependencies.length,
      crates: pathDependencies,
      replacement: 'use upstream crates.io dependencies where possible',
      recommendation: 'Replace fake compat crates in small compile-verified PRs; keep a temporary facade only at MCPace module boundaries.',
    });
  }


  const cargoLockIssues = cargoLockRefreshFindings(args.repoRoot);
  if (cargoLockIssues.length > 0) {
    findings.push({
      id: 'cargo-lock-needs-refresh',
      severity: 'high',
      title: 'Cargo.lock is not refreshed after replacing compat crates with upstream crates',
      count: cargoLockIssues.length,
      crates: cargoLockIssues,
      replacement: 'cargo update / cargo check with the pinned Rust toolchain',
      recommendation: `${cargoLockRefreshMessage(cargoLockIssues)}. Regenerate Cargo.lock on a Rust-enabled host before treating dependency modernization as complete.`,
    });
  }

  findings.push(finding(
    'manual-cli-parsing',
    'medium',
    'Commands manually parse argv instead of a typed CLI definition',
    grep(rustFiles, /parse_args|while\s+index\s*<\s*args\.len\(\)/),
    'Move command definitions to clap derive in phases: new commands first, then setup/serve/client/server.',
    'clap derive',
  ));

  findings.push(finding(
    'stringly-errors',
    'medium',
    'Rust modules return String errors across subsystem boundaries',
    grep(rustFiles, /Result<[^>]+,\s*String>|->\s*Result<[^>]+,\s*String>/),
    'Introduce thiserror for domain errors and anyhow at CLI boundaries.',
    'thiserror + anyhow',
  ));

  findings.push(finding(
    'raw-http-tcp',
    'medium',
    'HTTP/TCP handling is implemented directly in subsystem code',
    grep(rustFiles, /TcpListener|TcpStream|read_to_end|write_all\(.*HTTP\//s),
    'Replace outbound HTTP first with ureq/reqwest; move dashboard server only under security contract tests.',
    'ureq/reqwest; later axum+tower-http',
  ));

  findings.push(finding(
    'manual-config-patching',
    'medium',
    'Client config patching contains hand-written TOML/YAML upsert logic',
    grep(rustFiles, /upsert_toml|find_toml|parse_yaml|upsert_yaml/),
    'Use toml_edit for lossless TOML edits; keep YAML narrow or migrate carefully to a maintained parser.',
    'toml_edit',
  ));

  findings.push(finding(
    'stdout-stderr-ad-hoc-logging',
    'low',
    'Runtime modules use ad-hoc writeln!/println! diagnostics',
    grep(rustFiles, /println!|eprintln!|writeln!\(/),
    'Introduce tracing for serve/agent/stdio/dashboard logs while preserving CLI stdout contracts.',
    'tracing',
  ));

  const appJs = path.join(args.repoRoot, 'src/dashboard/frontend/app.js');
  if (fs.existsSync(appJs)) {
    const lines = text(appJs).split(/\r?\n/).length;
    findings.push({
      id: 'large-dashboard-frontend-module',
      severity: lines > 2000 ? 'medium' : 'low',
      title: 'Dashboard frontend is a large single JS module',
      count: lines,
      files: ['src/dashboard/frontend/app.js'],
      replacement: 'Vite + TypeScript modules',
      recommendation: 'Split into typed modules before adopting any large UI framework.',
    });
  }

  const actionable = findings.filter((item) => item.count > 0);
  const report = {
    schema: 'mcpace.modernizationInventory.v1',
    generatedAt: new Date().toISOString(),
    repoRoot: '.',
    findings: actionable,
    summary: {
      findings: actionable.length,
      high: actionable.filter((item) => item.severity === 'high').length,
      medium: actionable.filter((item) => item.severity === 'medium').length,
      low: actionable.filter((item) => item.severity === 'low').length,
    },
  };

  if (args.json) console.log(JSON.stringify(report, null, 2));
  else {
    console.log(`${report.summary.findings} modernization findings (${report.summary.high} high, ${report.summary.medium} medium, ${report.summary.low} low)`);
    for (const item of report.findings) console.log(`- ${item.severity}: ${item.id} — ${item.title} (${item.count})`);
  }
}

try {
  run();
} catch (error) {
  console.error(error?.stack ?? String(error));
  process.exitCode = 1;
}
