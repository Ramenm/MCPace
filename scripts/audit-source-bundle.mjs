#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '..');
const args = new Set(process.argv.slice(2));
const jsonOutput = args.has('--json');

function rel(...parts) {
  return path.join(repoRoot, ...parts);
}

function read(relativePath) {
  return fs.readFileSync(rel(relativePath), 'utf8');
}

function exists(relativePath) {
  return fs.existsSync(rel(relativePath));
}

function walk(dir, predicate = () => true) {
  const root = rel(dir);
  if (!fs.existsSync(root)) return [];
  const found = [];
  const stack = [root];
  while (stack.length > 0) {
    const current = stack.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const fullPath = path.join(current, entry.name);
      const relativePath = path.relative(repoRoot, fullPath).split(path.sep).join('/');
      if (entry.isDirectory()) {
        stack.push(fullPath);
      } else if (predicate(relativePath, fullPath)) {
        found.push(relativePath);
      }
    }
  }
  return found.sort();
}

function matchCount(source, regex) {
  return [...source.matchAll(regex)].length;
}

function check(id, severity, ok, details, remediation = null) {
  return { id, severity, status: ok ? 'pass' : 'fail', details, ...(remediation ? { remediation } : {}) };
}

function warning(id, details, remediation = null) {
  return { id, severity: 'medium', status: 'warn', details, ...(remediation ? { remediation } : {}) };
}

const cargo = read('Cargo.toml');
const jsonRs = read('src/json.rs');
const dashboardRs = read('src/dashboard.rs');
const updateRs = read('src/update.rs');
const proxyUriRs = read('src/adapter/proxy_uri.rs');
const httpSessionRs = read('src/dashboard/http_session.rs');
const serviceRs = read('src/service.rs');
const rustFiles = walk('src', (relativePath) => relativePath.endsWith('.rs'));
const allRust = rustFiles.map((file) => read(file)).join('\n');

const clapParserFiles = rustFiles.filter((file) => /parse_cli_args|cli_args::command/.test(read(file)));
const parseArgFiles = rustFiles.filter((file) => {
  const text = read(file);
  return /fn\s+parse_args\b/.test(text) && !/cli_args::command/.test(text);
});
const httpClientImplementationFiles = ['src/serve.rs', 'src/setup.rs', 'src/upstream/http_runtime.rs'];
const httpRawClientFiles = httpClientImplementationFiles
  .filter((file) => exists(file))
  .filter((file) => /TcpStream|ToSocketAddrs|Shutdown::Write|HTTP\/1\.1|Connection: close|Content-Length:|split_once\(["']\\r\\n\\r\\n["']\)/.test(read(file)));
const userFacingFlagFiles = [
  'src/client/args.rs',
  'src/setup.rs',
  'CHANGELOG.md',
  'reports/summary.md',
].filter(exists);
const legacyClientModeFlags = userFacingFlagFiles
  .flatMap((file) => {
    const text = read(file);
    return [...text.matchAll(/--(?:exclusive|exclusive-client|only-mcpace|replace-existing|replace-existing-client-servers)\b/g)]
      .map((match) => `${file}:${match[0]}`);
  });

const checks = [
  check(
    'compat-crates-removed',
    'high',
    !exists('crates/compat'),
    'No local crates/compat shim tree is shipped.'
  ),
  check(
    'no-compat-path-dependencies',
    'high',
    !/path\s*=\s*["']crates\/compat/.test(cargo),
    'Cargo.toml does not point dependencies at removed compat shim crates.'
  ),
  check(
    'json-delegates-to-serde-json',
    'high',
    cargo.includes('serde_json =') && /serde_json::from_str/.test(jsonRs) && !/struct\s+Parser|enum\s+Token|fn\s+parse_number/.test(jsonRs),
    'JSON parsing/serialization is delegated to serde_json through src/json.rs compatibility wrappers.'
  ),
  check(
    'dashboard-server-uses-tiny-http',
    'high',
    cargo.includes('tiny_http =') && /tiny_http::Server::from_listener/.test(dashboardRs),
    'Dashboard request intake is backed by tiny_http instead of a raw request parser.'
  ),
  check(
    'http-client-helpers-use-ureq',
    'medium',
    cargo.includes('ureq =') && /http_client::get_text|http_client::post_json_text/.test(allRust) && httpRawClientFiles.length === 0,
    'Local health/setup/upstream HTTP client calls are backed by ureq instead of raw TcpStream request strings.'
  ),
  check(
    'auto-launch-uses-crate',
    'medium',
    cargo.includes('auto-launch =') && /AutoLaunchBuilder/.test(serviceRs) && !/wscript|\.vbs|VBScript/i.test(serviceRs),
    'Autostart setup uses the auto-launch crate and no VBScript launcher remains.'
  ),
  check(
    'semver-uses-crate',
    'low',
    cargo.includes('semver =') && /semver::Version::parse/.test(updateRs),
    'Version comparison uses semver::Version instead of tuple-only comparison.'
  ),
  check(
    'hex-uses-crate',
    'low',
    cargo.includes('hex =') && /hex::encode/.test(proxyUriRs) && /hex::encode/.test(httpSessionRs) && !/fn\s+hex_encode/.test(allRust),
    'Hex encoding/decoding is centralized on the hex crate.'
  ),
  check(
    'which-uses-crate',
    'low',
    cargo.includes('which =') && /which::which/.test(allRust),
    'PATH lookup delegates to the which crate.'
  ),
  check(
    'randomness-uses-getrandom',
    'low',
    cargo.includes('getrandom =') && /getrandom::getrandom/.test(allRust),
    'OS randomness delegates to getrandom.'
  ),
  check(
    'target-metadata-generator-present',
    'medium',
    exists('scripts/sync-platform-packages.mjs') && /Generated from release-targets\.json/.test(read('packages/npm/cli/lib/targets.js')),
    'Generated npm target metadata has a checked-in generator.'
  ),
  check(
    'cli-parsing-uses-clap',
    'medium',
    cargo.includes('clap =') && exists('src/cli_args.rs') && clapParserFiles.length >= 10,
    `Root/common CLI parsing is backed by clap for ${clapParserFiles.length} Rust modules while legacy single-dash aliases stay normalized.`
  ),
  check(
    'single-managed-client-mode',
    'medium',
    legacyClientModeFlags.length === 0,
    legacyClientModeFlags.length === 0
      ? 'Client install exposes one managed mode by default; no extra exclusive/replace flags are documented or parsed.'
      : `Legacy client mode flags still appear: ${legacyClientModeFlags.join(', ')}`,
    'Remove extra client wiring modes and keep MCPace as the only managed entrypoint in supported JSON clients.'
  )
];

const warnings = [];
if (parseArgFiles.length > 0) {
  warnings.push(warning(
    'manual-cli-parsers-remaining',
    `${parseArgFiles.length} Rust files still contain manual parse_args/index-loop CLI parsing: ${parseArgFiles.join(', ')}`,
    'Next safe cleanup after Rust compile coverage: migrate repeated parsers to clap for full UX or lexopt for minimal dependency footprint.'
  ));
}
if (httpRawClientFiles.length > 0) {
  warnings.push(warning(
    'raw-http-client-helpers-remaining',
    `${httpRawClientFiles.length} non-test HTTP client implementation files still contain raw TcpStream/request-string helpers: ${httpRawClientFiles.join(', ')}`,
    'Replace remaining raw HTTP client helpers with src/http_client.rs / ureq.'
  ));
}

const failed = checks.filter((item) => item.status === 'fail');
const result = {
  schema: 'mcpace.sourceAudit.v1',
  generatedAt: new Date().toISOString(),
  root: repoRoot,
  status: failed.length === 0 ? 'pass' : 'fail',
  checks,
  warnings,
  counts: {
    rustFiles: rustFiles.length,
    manualCliParserFiles: parseArgFiles.length,
    clapParserFiles: clapParserFiles.length,
    rawHttpClientFiles: httpRawClientFiles.length,
    legacyClientModeFlags: legacyClientModeFlags.length
  }
};

if (jsonOutput) {
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
} else {
  process.stdout.write(`MCPace source audit: ${result.status}\n`);
  for (const item of checks) {
    process.stdout.write(`[${item.status}] ${item.id}: ${item.details}\n`);
  }
  for (const item of warnings) {
    process.stdout.write(`[warn] ${item.id}: ${item.details}\n`);
  }
}

if (failed.length > 0) {
  process.exitCode = 1;
}
