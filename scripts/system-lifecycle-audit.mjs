#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { deriveProjectName, deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';

const PRODUCTION_RUST_ALLOW_DIRECT_WRITE = new Set([
  'src/runtimepaths.rs',
]);

const TEST_RUST_PATH_PATTERNS = [
  /(^|\/)tests\.rs$/,
  /(^|\/)tests?\//,
];

const REQUIRED_DOC_TERMS = [
  /Durable user config/i,
  /Durable control-plane state/i,
  /Disposable cache/i,
  /Ephemeral runtime facts/i,
  /Install/i,
  /First start/i,
  /Runtime/i,
  /Restart/i,
  /Crash recovery/i,
  /Upgrade and reinstall/i,
  /Uninstall/i,
  /Diagnostics/i,
  /Release and publish/i,
  /Critical node map/i,
];

const REQUIRED_ATOMIC_PATHS = [
  ['src/runtimepaths.rs', /pub fn write_text_atomic\(path: &Path, contents: &str\) -> Result<\(\), String>/],
  ['src/init.rs', /runtimepaths::write_text_atomic\(path, &value\.to_pretty_string\(\)\)/],
  ['src/mcp_sources/write.rs', /runtimepaths::write_text_atomic\(&target_path, &serialized\)/],
  ['src/mcp_sources/import.rs', /runtimepaths::write_text_atomic\(target_path, &serialized\)/],
  ['src/client/actions.rs', /runtimepaths::write_text_atomic\(&self\.config_path, &update\.contents\)/],
  ['src/client/actions/backup.rs', /runtimepaths::write_text_atomic\(&config_path, &contents\)/],
  ['src/service.rs', /runtimepaths::write_text_atomic\(&script_path, &script\)/],
  ['src/serve.rs', /runtimepaths::write_text_atomic\(path, &contents\)/],
  ['src/hub/runtime.rs', /runtimepaths::write_text_atomic\(path, &contents\)/],
  ['src/upstream/tool_cache.rs', /runtimepaths::write_text_atomic\(&path, &envelope\.to_compact_string\(\)\)/],
];

const REQUIRED_CACHE_TERMS = [
  ['src/upstream/tool_cache.rs', /TOOL_LIST_DISK_CACHE_TTL/],
  ['src/upstream/tool_cache.rs', /mcpaceVersion/],
  ['src/upstream/tool_cache.rs', /mcpProtocolVersion/],
  ['src/upstream.rs', /TOOL_LIST_CACHE_TTL: Duration = Duration::from_secs\(30\)/],
  ['src/upstream.rs', /UPSTREAM_SESSION_IDLE_TTL: Duration = Duration::from_secs\(300\)/],
  ['src/dashboard/http_session.rs', /sessions: HashMap<String, McpHttpSession>/],
  ['src/dashboard/http_session.rs', /DEFAULT_MCP_HTTP_SESSION_TTL_MS/],
];

const REQUIRED_INSTALL_TERMS = [
  ['src/service.rs', /current_exe\(\)/],
  ['src/service.rs', /targetAppPath/],
  ['src/service.rs', /service <install\|status\|uninstall\|print>/],
  ['src/cleanup.rs', /cleanup_report/],
  ['src/app.rs', /"cleanup" => cleanup::run/],
  ['src/catalog.rs', /name: "cleanup"/],
  ['docs/system-lifecycle-hardening.md', /npm uninstall.*cannot be the only cleanup mechanism|Package-manager uninstall cannot be the only cleanup mechanism/s],
  ['packages/npm/cli/package.json', /optionalDependencies/],
];

const REQUIRED_RELEASE_TERMS = [
  ['scripts/verify-npm-pack.mjs', /executable/i],
  ['scripts/verify-platform-packages.mjs', /releaseTargets|release-targets|package/i],
  ['scripts/publish-decision.mjs', /rust|source|publish/i],
  ['docs/system-lifecycle-hardening.md', /real-client trace before strengthening runtime beta claims/i],
];

function rel(file) {
  return path.relative(repoRoot, file).replaceAll(path.sep, '/');
}

function read(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

function exists(relativePath) {
  return fs.existsSync(path.join(repoRoot, relativePath));
}

function walk(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'target' || entry.name === 'node_modules' || entry.name === '.git') continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, out);
    else out.push(full);
  }
  return out;
}

function addFinding(findings, severity, id, detail, file = null) {
  findings.push({ severity, id, detail, file });
}

function assertFileContains(findings, id, relativePath, pattern, severity = 'blocker') {
  if (!exists(relativePath)) {
    addFinding(findings, severity, id, `${relativePath} is missing`, relativePath);
    return;
  }
  const contents = read(relativePath);
  if (!pattern.test(contents)) {
    addFinding(findings, severity, id, `${relativePath} does not match ${pattern}`, relativePath);
  }
}

function auditDocs(findings) {
  const doc = 'docs/system-lifecycle-hardening.md';
  if (!exists(doc)) {
    addFinding(findings, 'blocker', 'missing-system-lifecycle-doc', `${doc} is missing`, doc);
    return;
  }
  const contents = read(doc);
  for (const pattern of REQUIRED_DOC_TERMS) {
    if (!pattern.test(contents)) addFinding(findings, 'blocker', 'system-lifecycle-doc-gap', `missing lifecycle term ${pattern}`, doc);
  }
  assertFileContains(findings, 'runtime-doc-links-system-contract', 'docs/runtime-state-cache-lifecycle.md', /system-lifecycle-hardening\.md/);
  assertFileContains(findings, 'architecture-links-system-contract', 'docs/architecture-boundaries.md', /system-lifecycle-hardening\.md/);
  assertFileContains(findings, 'docs-readme-links-system-contract', 'docs/README.md', /system-lifecycle-hardening\.md/);
}

function auditAtomicWrites(findings) {
  for (const [file, pattern] of REQUIRED_ATOMIC_PATHS) {
    assertFileContains(findings, `atomic-write-${file}`, file, pattern);
  }

  const rustFiles = walk(path.join(repoRoot, 'src')).filter((file) => file.endsWith('.rs'));
  for (const file of rustFiles) {
    const relative = rel(file);
    if (PRODUCTION_RUST_ALLOW_DIRECT_WRITE.has(relative)) continue;
    if (TEST_RUST_PATH_PATTERNS.some((pattern) => pattern.test(relative))) continue;
    const contents = fs.readFileSync(file, 'utf8');
    const testStart = contents.search(/#\[cfg\(test\)\]|mod tests\s*\{/);
    const matches = [...contents.matchAll(/(?:std::)?fs::write\s*\(/g)];
    for (const match of matches) {
      if (testStart >= 0 && match.index >= testStart) continue;
      addFinding(findings, 'blocker', 'direct-production-fs-write', `${relative} uses direct fs::write outside test-only code; use runtimepaths::write_text_atomic or an explicit append-only log pattern`, relative);
    }
  }
}

function auditCachesAndSessions(findings) {
  for (const [file, pattern] of REQUIRED_CACHE_TERMS) assertFileContains(findings, `cache-session-${file}-${pattern}`, file, pattern);
}

function auditInstallReinstallUninstall(findings) {
  for (const [file, pattern] of REQUIRED_INSTALL_TERMS) assertFileContains(findings, `install-lifecycle-${file}-${pattern}`, file, pattern);
  const service = exists('src/service.rs') ? read('src/service.rs') : '';
  if (/max_connections: Option<usize>,\s*max_connections: Option<usize>,/.test(service)) {
    addFinding(findings, 'blocker', 'duplicate-service-parameter', 'service append_serve_resource_args duplicates max_connections and will not compile', 'src/service.rs');
  }
}

function auditReleaseGates(findings) {
  for (const [file, pattern] of REQUIRED_RELEASE_TERMS) assertFileContains(findings, `release-gate-${file}-${pattern}`, file, pattern, 'warn');
}

function collectSystemLifecycleAudit() {
  const findings = [];
  auditDocs(findings);
  auditAtomicWrites(findings);
  auditCachesAndSessions(findings);
  auditInstallReinstallUninstall(findings);
  auditReleaseGates(findings);

  const blockers = findings.filter((finding) => finding.severity === 'blocker');
  const warnings = findings.filter((finding) => finding.severity === 'warn');
  return {
    schema: 'mcpace.systemLifecycleAudit.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status: blockers.length ? 'blocked' : warnings.length ? 'pass-with-warnings' : 'pass',
    summary: {
      blockers: blockers.length,
      warnings: warnings.length,
      findings: findings.length,
    },
    checkedAreas: [
      'docs',
      'atomic-writes',
      'state-cache-session-contracts',
      'install-reinstall-uninstall',
      'release-proof-gates',
    ],
    findings,
  };
}

function renderMarkdown(report) {
  const lines = [
    '# MCPace system lifecycle audit',
    '',
    `Project: \`${report.project.name}\` v\`${report.project.version}\``,
    `Status: \`${report.status}\``,
    '',
    '| severity | id | file | detail |',
    '|---|---|---|---|',
  ];
  if (report.findings.length === 0) {
    lines.push('| pass | no-findings | — | All lifecycle checks passed. |');
  } else {
    for (const finding of report.findings) {
      lines.push(`| ${finding.severity} | ${finding.id} | ${finding.file || '—'} | ${String(finding.detail).replace(/\|/g, '\\|')} |`);
    }
  }
  lines.push('');
  return lines.join('\n');
}

function parseArgs(argv) {
  const out = { json: false, write: null, markdown: null, strict: false, help: false };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--json') out.json = true;
    else if (arg === '--write') out.write = argv[++i] || null;
    else if (arg === '--markdown' || arg === '--write-md') out.markdown = argv[++i] || null;
    else if (arg === '--strict') out.strict = true;
    else if (arg === '-h' || arg === '--help') out.help = true;
    else throw new Error(`unsupported system-lifecycle-audit argument: ${arg}`);
  }
  return out;
}

function writeFile(file, contents) {
  const target = path.resolve(repoRoot, file);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, contents, 'utf8');
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) {
    process.stdout.write('Usage: node scripts/system-lifecycle-audit.mjs [--json] [--write <path>] [--markdown <path>] [--strict]\n');
    return;
  }
  const report = collectSystemLifecycleAudit();
  if (opts.write) writeFile(opts.write, `${JSON.stringify(report, null, 2)}\n`);
  if (opts.markdown) writeFile(opts.markdown, renderMarkdown(report));
  process.stdout.write(opts.json ? `${JSON.stringify(report, null, 2)}\n` : renderMarkdown(report));
  if (opts.strict && report.status === 'blocked') process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(); } catch (error) { process.stderr.write(`${error.message || error}\n`); process.exitCode = 1; }
}

export { collectSystemLifecycleAudit, renderMarkdown };
