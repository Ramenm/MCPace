#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { repoRoot as defaultRepoRoot } from './lib/project-metadata.mjs';
import { cargoLockRefreshFindings } from './lib/cargo-policy.mjs';

const SKIP_DIRS = new Set(['.git', 'node_modules', 'target', 'dist', '.artifacts']);

function parseArgs(argv) {
  const args = { json: false, repoRoot: defaultRepoRoot };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--json') args.json = true;
    else if (arg === '--repo') args.repoRoot = path.resolve(argv[++index]);
    else if (arg === '--help' || arg === '-h') {
      console.log('Usage: node scripts/legacy-subsystem-map.mjs [--json] [--repo DIR]');
      process.exit(0);
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  return args;
}

function normalize(value) {
  return value.split(path.sep).join('/');
}

function walkFiles(root, predicate = () => true) {
  const files = [];
  const stack = [root];
  while (stack.length > 0) {
    const current = stack.pop();
    if (!fs.existsSync(current)) continue;
    for (const entry of fs.readdirSync(current, { withFileTypes: true }).sort((left, right) => left.name.localeCompare(right.name))) {
      const full = path.join(current, entry.name);
      const relative = normalize(path.relative(root, full));
      if (entry.isDirectory()) {
        if (!SKIP_DIRS.has(entry.name) && !relative.split('/').some((part) => SKIP_DIRS.has(part))) stack.push(full);
      } else if (entry.isFile() && predicate(full)) {
        files.push(full);
      }
    }
  }
  return files.sort();
}

function readText(file) {
  return fs.existsSync(file) ? fs.readFileSync(file, 'utf8') : '';
}

function rel(repoRoot, files) {
  return files.map((file) => normalize(path.relative(repoRoot, file))).sort();
}

function grep(repoRoot, files, pattern) {
  return rel(repoRoot, files.filter((file) => pattern.test(readText(file))));
}

function finding({ id, subsystem, status, title, evidence = [], replacement, next }) {
  return {
    id,
    subsystem,
    status,
    title,
    evidence: evidence.slice(0, 40),
    evidenceCount: evidence.length,
    truncated: evidence.length > 40,
    replacement,
    next,
  };
}

function run() {
  const args = parseArgs(process.argv.slice(2));
  const repoRoot = args.repoRoot;
  const srcRoot = path.join(repoRoot, 'src');
  const scriptsRoot = path.join(repoRoot, 'scripts');
  const rustFiles = walkFiles(srcRoot, (file) => file.endsWith('.rs'));
  const jsFiles = walkFiles(scriptsRoot, (file) => file.endsWith('.mjs'));
  const evalPartials = walkFiles(path.join(repoRoot, 'eval'), (file) => file.endsWith('.partial.jsonl'));
  const cargoToml = readText(path.join(repoRoot, 'Cargo.toml'));
  const compatDeps = [...cargoToml.matchAll(/^\s*([A-Za-z0-9_-]+)\s*=\s*\{\s*path\s*=\s*"crates\/compat\//gm)].map((match) => match[1]);
  const cargoLockIssues = cargoLockRefreshFindings(repoRoot);

  const manualCli = grep(repoRoot, rustFiles, /parse_args|while\s+index\s*<\s*args\.len\(\)/);
  const configPatchers = grep(repoRoot, rustFiles, /upsert_toml|find_toml|parse_yaml|upsert_yaml/);
  const rawHttp = grep(repoRoot, rustFiles, /TcpListener|TcpStream|read_to_end|write_all\(.*HTTP\//s);
  const adHocLogging = grep(repoRoot, rustFiles, /println!|eprintln!|writeln!\(/);
  const zipWriter = fs.existsSync(path.join(repoRoot, 'scripts/lib/zip-writer.mjs')) ? ['scripts/lib/zip-writer.mjs'] : [];
  const appJs = path.join(repoRoot, 'src/dashboard/frontend/app.js');
  const appJsLines = fs.existsSync(appJs) ? readText(appJs).split(/\r?\n/).length : 0;
  const stdioShim = readText(path.join(repoRoot, 'src/stdio_shim.rs'));

  const findings = [
    finding({
      id: 'dependencies.compat-crates',
      subsystem: 'dependencies',
      status: compatDeps.length === 0 ? 'done' : 'blocked',
      title: 'Local compatibility crates no longer shadow standard crates',
      evidence: compatDeps,
      replacement: 'upstream crates.io dependencies',
      next: compatDeps.length === 0 ? 'Keep dependency-policy guard enabled.' : 'Replace path dependencies under crates/compat with upstream crates.',
    }),
    finding({
      id: 'dependencies.cargo-lock-refresh',
      subsystem: 'dependencies',
      status: cargoLockIssues.length === 0 ? 'done' : 'blocked',
      title: 'Cargo.lock must be refreshed after upstream dependency migration',
      evidence: cargoLockIssues.map((item) => `${item.crate}: locked=${item.lock ?? '<missing>'}, required=${item.dependency}`),
      replacement: 'cargo update / cargo check on the pinned Rust toolchain',
      next: cargoLockIssues.length === 0 ? 'Keep strict release gate enabled.' : 'Run cargo update and cargo check/test on a Rust-enabled host.',
    }),
    finding({
      id: 'cli.manual-argv',
      subsystem: 'cli',
      status: manualCli.length === 0 ? 'done' : 'open',
      title: 'Command parsing still has handwritten argv scanners',
      evidence: manualCli,
      replacement: 'clap derive',
      next: 'Migrate setup/serve/client/server after agent/autostart/studio command boundaries are stable.',
    }),
    finding({
      id: 'config.lossless-editing',
      subsystem: 'client-config',
      status: configPatchers.length === 0 ? 'done' : 'open',
      title: 'Client config editing still has hand-written TOML/YAML upserts',
      evidence: configPatchers,
      replacement: 'toml_edit for TOML; narrow maintained YAML handling only where required',
      next: 'Start with TOML targets because lossless comment/order preservation is well supported.',
    }),
    finding({
      id: 'mcp.stdio-preview',
      subsystem: 'mcp-runtime',
      status: /Live MCP stdio message forwarding is not implemented yet/.test(stdioShim) ? 'open' : 'done',
      title: 'Public mcpace stdio command exists but live forwarding remains preview',
      evidence: ['src/stdio_shim.rs'],
      replacement: 'real mcpace stdio launcher/proxy, then rmcp spike',
      next: 'Implement stdout-only JSON-RPC forwarding and keep diagnostic logs on stderr/file.',
    }),
    finding({
      id: 'http.raw-tcp',
      subsystem: 'networking',
      status: rawHttp.length === 0 ? 'done' : 'open',
      title: 'Some HTTP/TCP handling is still implemented directly',
      evidence: rawHttp,
      replacement: 'ureq/reqwest for outbound HTTP; later axum+tower-http under security tests',
      next: 'Replace upstream outbound HTTP before dashboard server migration.',
    }),
    finding({
      id: 'observability.ad-hoc-logging',
      subsystem: 'observability',
      status: adHocLogging.length === 0 ? 'done' : 'open',
      title: 'Runtime logging is still scattered across stdout/stderr writes',
      evidence: adHocLogging,
      replacement: 'tracing + tracing-subscriber',
      next: 'Start with agent/serve/stdio so MCP stdout contracts stay clean.',
    }),
    finding({
      id: 'frontend.large-module',
      subsystem: 'dashboard',
      status: appJsLines > 2000 ? 'open' : 'done',
      title: 'Dashboard frontend remains a large single JavaScript module',
      evidence: appJsLines > 0 ? [`src/dashboard/frontend/app.js:${appJsLines} lines`] : [],
      replacement: 'Vite + TypeScript modules, framework only if needed later',
      next: 'Split API/client/state/render modules before considering a UI framework.',
    }),
    finding({
      id: 'release.zip-writer',
      subsystem: 'release-engineering',
      status: zipWriter.length === 0 ? 'done' : 'open',
      title: 'Release tooling still owns a ZIP writer implementation',
      evidence: zipWriter,
      replacement: 'zip crate or checked npm ZIP library; cargo-packager/cargo-dist spike for installer graph',
      next: 'Keep current tests as contract, then replace ZIP serialization with a maintained library.',
    }),
    finding({
      id: 'source.generated-partials',
      subsystem: 'source-hygiene',
      status: evalPartials.length === 0 ? 'done' : 'blocked',
      title: 'Checked-in eval partial streams are removed from the clean source tree',
      evidence: rel(repoRoot, evalPartials),
      replacement: 'final JSON/CSV fixtures only; partial streams are runtime artifacts',
      next: evalPartials.length === 0 ? 'Keep source-archive policy pattern enabled.' : 'Delete *.partial.jsonl or move to ignored runtime output.',
    }),
  ];

  const report = {
    schema: 'mcpace.legacySubsystemMap.v1',
    generatedAt: new Date().toISOString(),
    repoRoot: '.',
    rustFiles: rustFiles.length,
    scriptFiles: jsFiles.length,
    summary: {
      total: findings.length,
      done: findings.filter((item) => item.status === 'done').length,
      open: findings.filter((item) => item.status === 'open').length,
      blocked: findings.filter((item) => item.status === 'blocked').length,
    },
    findings,
  };

  if (args.json) console.log(JSON.stringify(report, null, 2));
  else {
    console.log(`${report.summary.done}/${report.summary.total} subsystem modernization items done; ${report.summary.blocked} blocked`);
    for (const item of findings) console.log(`- ${item.status}: ${item.id} — ${item.title}`);
  }
}

try {
  run();
} catch (error) {
  console.error(error?.stack ?? String(error));
  process.exitCode = 1;
}
