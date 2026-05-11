#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { deriveProjectName, deriveProjectVersion, readJson, repoRoot } from './lib/project-metadata.mjs';

const EXCLUDED_DIRS = new Set(['.git', 'target', 'node_modules', '.next', 'dist', 'coverage', 'vendor']);
const TEXT_EXTENSIONS = new Set(['.rs', '.js', '.mjs', '.json', '.md', '.yml', '.yaml', '.toml', '.cff']);

function parseArgs(argv) {
  const parsed = { json: false, write: null, markdown: null, strict: false, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json':
        parsed.json = true;
        break;
      case '--write':
        parsed.write = argv[++index] || null;
        if (!parsed.write) throw new Error('bug-sweep requires a path after --write');
        break;
      case '--markdown':
      case '--write-md':
        parsed.markdown = argv[++index] || null;
        if (!parsed.markdown) throw new Error('bug-sweep requires a path after --markdown');
        break;
      case '--strict':
        parsed.strict = true;
        break;
      case '-h':
      case '--help':
        parsed.help = true;
        break;
      default:
        throw new Error(`unsupported bug-sweep argument: ${token}`);
    }
  }
  return parsed;
}

function printHelp() {
  process.stdout.write('Usage: node scripts/bug-sweep.mjs [--json] [--write <path>] [--markdown <path>] [--strict]\n\nChecks bug-fix hygiene, high-risk source patterns, and MCP HTTP boundary invariants.\n');
}

function relativePath(absolutePath) {
  return path.relative(repoRoot, absolutePath).split(path.sep).join('/');
}

function absolute(relative) {
  return path.join(repoRoot, relative);
}

function readText(relative) {
  try {
    return fs.readFileSync(absolute(relative), 'utf8');
  } catch {
    return null;
  }
}

function exists(relative) {
  return fs.existsSync(absolute(relative));
}

function walk(dir = repoRoot, files = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name.startsWith('.') && entry.name !== '.github') continue;
    if (entry.isDirectory()) {
      if (EXCLUDED_DIRS.has(entry.name)) continue;
      walk(path.join(dir, entry.name), files);
      continue;
    }
    if (!entry.isFile()) continue;
    const ext = path.extname(entry.name);
    if (TEXT_EXTENSIONS.has(ext)) files.push(path.join(dir, entry.name));
  }
  return files;
}

function makeCheck(id, severity, status, evidence, nextAction) {
  return { id, severity, status, evidence, nextAction };
}

function pass(id, evidence, nextAction = 'Keep this invariant covered while changing nearby code.') {
  return makeCheck(id, 'info', 'pass', evidence, nextAction);
}

function warn(id, evidence, nextAction) {
  return makeCheck(id, 'warn', 'warn', evidence, nextAction);
}

function block(id, evidence, nextAction) {
  return makeCheck(id, 'blocker', 'blocked', evidence, nextAction);
}

function requireFileContains(relative, purpose, markers) {
  const text = readText(relative);
  if (text === null) return block(`file:${relative}`, 'missing', `Add ${relative} for ${purpose}.`);
  const missing = markers.filter((marker) => !text.includes(marker));
  if (missing.length > 0) {
    return block(`file:${relative}`, `missing expected markers: ${missing.join(', ')}`, `Update ${relative} so it covers ${purpose}.`);
  }
  return pass(`file:${relative}`, `${purpose}; ${text.split(/\r?\n/).length} lines`);
}

function listIssueTemplates() {
  const dir = absolute('.github/ISSUE_TEMPLATE');
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir).filter((entry) => /\.ya?ml$/.test(entry)).sort();
}

function scanRiskyRustMacros() {
  const findings = [];
  for (const absolutePath of walk()) {
    const relative = relativePath(absolutePath);
    if (!relative.startsWith('src/') || !relative.endsWith('.rs') || relative.endsWith('/tests.rs')) continue;
    const text = fs.readFileSync(absolutePath, 'utf8');
    const lines = text.split(/\r?\n/);
    for (const [index, line] of lines.entries()) {
      if (/\b(todo!|unimplemented!|dbg!)\s*\(/.test(line)) {
        findings.push({ file: relative, line: index + 1, text: line.trim() });
      }
    }
  }
  return findings;
}

function scanGeneratedReports() {
  const reports = [];
  for (const relative of ['reports/product-practice-latest.json', 'reports/runtime-trace-latest.json', 'reports/rust-quality-latest.json']) {
    if (!exists(relative)) {
      reports.push({ relative, status: 'missing' });
      continue;
    }
    try {
      const report = readJson(relative);
      reports.push({ relative, status: report.status || 'unknown', generatedAt: report.generatedAt || null, version: report.project?.version || report.package?.version || null });
    } catch (error) {
      reports.push({ relative, status: 'unreadable', error: String(error.message || error) });
    }
  }
  return reports;
}

function buildReport() {
  const pkg = readJson('package.json');
  const scripts = pkg.scripts || {};
  const checks = [];

  checks.push(requireFileContains('docs/bug-hunting-and-fix-playbook.md', 'the reproducible bug-fix lifecycle', ['Reproduce', 'Root cause', 'Regression test', 'Roll out safely']));
  checks.push(requireFileContains('docs/defect-taxonomy-and-labels.md', 'maintainer triage labels and severity routing', ['type:regression', 'area:mcp-http', 'severity:p0', 'status:needs-runtime-trace']));
  checks.push(requireFileContains('docs/maintainer-debugging-guide.md', 'area-specific debugging commands', ['Runtime HTTP checklist', 'Upstream stdio checklist', 'Flaky test checklist']));

  if (!scripts['verify:bug-sweep']) {
    checks.push(block('script:verify:bug-sweep', 'missing package.json script', 'Add a package script so bug hygiene can run locally and in CI.'));
  } else {
    checks.push(pass('script:verify:bug-sweep', scripts['verify:bug-sweep']));
  }

  const ci = readText('.github/workflows/ci.yml') || '';
  if (!ci.includes('verify:bug-sweep')) {
    checks.push(block('workflow:ci-bug-sweep', 'ci.yml does not run verify:bug-sweep', 'Run verify:bug-sweep in the fast source validation lane.'));
  } else {
    checks.push(pass('workflow:ci-bug-sweep', 'CI runs verify:bug-sweep in the source validation lane.'));
  }

  const prTemplate = readText('.github/pull_request_template.md') || '';
  const prMarkers = ['Root cause', 'Regression test', 'Not-tested'];
  const missingPrMarkers = prMarkers.filter((marker) => !prTemplate.includes(marker));
  if (missingPrMarkers.length > 0) {
    checks.push(block('template:pr-bug-fix-discipline', `missing PR markers: ${missingPrMarkers.join(', ')}`, 'Keep root cause, regression, and not-tested fields in the PR template.'));
  } else {
    checks.push(pass('template:pr-bug-fix-discipline', 'PR template requires root cause, regression proof, and not-tested disclosure.'));
  }

  const bugTemplate = readText('.github/ISSUE_TEMPLATE/bug_report.yml') || '';
  const bugMarkers = ['Reproduction', 'Expected behavior', 'Actual behavior', 'Last known good version'];
  const missingBugMarkers = bugMarkers.filter((marker) => !bugTemplate.includes(marker));
  if (missingBugMarkers.length > 0) {
    checks.push(block('template:bug-report-repro', `missing bug report markers: ${missingBugMarkers.join(', ')}`, 'Require version, reproduction, expected/actual, and regression context in bug reports.'));
  } else {
    checks.push(pass('template:bug-report-repro', 'Bug template captures reproduction and regression context.'));
  }

  const templates = listIssueTemplates();
  if (!templates.includes('flaky-test.yml')) {
    checks.push(warn('template:flaky-test', 'missing flaky-test.yml', 'Add a dedicated flaky-test issue form once CI starts collecting public flakes.'));
  } else {
    checks.push(pass('template:flaky-test', 'Dedicated flaky-test issue form is present.'));
  }

  const riskyRust = scanRiskyRustMacros();
  if (riskyRust.length > 0) {
    checks.push(block('source:prod-rust-stub-macros', riskyRust.slice(0, 5).map((item) => `${item.file}:${item.line}`).join(', '), 'Remove todo!/unimplemented!/dbg! from production Rust paths or move them behind tests.'));
  } else {
    checks.push(pass('source:prod-rust-stub-macros', 'No todo!, unimplemented!, or dbg! macros found in production Rust files.'));
  }

  const boundary = readText('src/dashboard/http_boundary.rs') || '';
  if (!boundary.includes('is_allowed_local_host') || !boundary.includes('host') || !boundary.includes('origin == "null"') || !boundary.includes('return false;')) {
    checks.push(block('runtime:http-origin-host-boundary', 'HTTP boundary must validate local Host and explicitly reject Origin: null by default.', 'Keep local HTTP serve mode guarded against DNS rebinding and opaque-origin requests.'));
  } else {
    checks.push(pass('runtime:http-origin-host-boundary', 'Local HTTP boundary validates Host and Origin and explicitly rejects Origin: null by default.'));
  }

  const mcpHttp = readText('src/dashboard/mcp_http.rs') || '';
  if (!mcpHttp.includes('generated_mcp_http_session_id(request, &id, negotiated)')) {
    checks.push(block('runtime:server-minted-session-id', 'initialize path does not visibly mint session ids server-side', 'Streamable HTTP initialize should return a server-minted session id.'));
  } else {
    checks.push(pass('runtime:server-minted-session-id', 'Initialize path uses a server-minted session id.'));
  }

  const sessionStore = readText('src/dashboard/http_session.rs') || '';
  if (!sessionStore.includes('MAX_MCP_HTTP_SESSION_ID_BYTES') || !sessionStore.includes('getrandom::getrandom')) {
    checks.push(block('runtime:session-id-bounds-randomness', 'session store must bound ids and use OS randomness', 'Keep session ids bounded, visible ASCII, and generated from OS randomness when possible.'));
  } else {
    checks.push(pass('runtime:session-id-bounds-randomness', 'Session ids are bounded and generated from OS randomness with fallback diagnostics.'));
  }

  const reports = scanGeneratedReports();
  const runtime = reports.find((entry) => entry.relative === 'reports/runtime-trace-latest.json');
  if (!runtime || runtime.status === 'missing') {
    checks.push(warn('reports:runtime-trace', 'runtime trace report is missing in this checkout', 'Generate a fresh runtime trace before any runtime beta or release claim.'));
  } else if (runtime.status !== 'pass') {
    checks.push(warn('reports:runtime-trace', `runtime trace report is present but status is ${runtime.status}`, 'Keep runtime beta and release claims blocked until a fresh runtime trace passes on a supported host.'));
  } else {
    checks.push(pass('reports:runtime-trace-present', 'runtime trace report is present and passing.'));
  }

  const rustQuality = reports.find((entry) => entry.relative === 'reports/rust-quality-latest.json');
  if (rustQuality && !['pass', 'missing'].includes(rustQuality.status)) {
    checks.push(warn('reports:rust-quality', `rust quality report status is ${rustQuality.status}`, 'Regenerate Rust quality proof on a host with Cargo dependencies before release claims.'));
  }

  const blocked = checks.filter((entry) => entry.status === 'blocked');
  const warnings = checks.filter((entry) => entry.status === 'warn');
  const status = blocked.length > 0 ? 'blocked' : warnings.length > 0 ? 'pass-with-warnings' : 'pass';
  return {
    schema: 'mcpace.bugSweep.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status,
    summary: {
      checks: checks.length,
      blocked: blocked.length,
      warnings: warnings.length,
      issueTemplates: templates,
      generatedReports: reports,
    },
    checks,
    nextMoves: checks.filter((entry) => entry.status !== 'pass').map((entry) => entry.nextAction),
  };
}

function renderMarkdown(report) {
  const lines = [
    '# MCPace bug sweep',
    '',
    `Project: \`${report.project.name}\` v\`${report.project.version}\``,
    `Status: **${report.status}**`,
    '',
    '## Summary',
    '',
    `- Checks: ${report.summary.checks}`,
    `- Blocked: ${report.summary.blocked}`,
    `- Warnings: ${report.summary.warnings}`,
    '',
    '## Checks',
    '',
    '| Check | Severity | Status | Evidence | Next action |',
    '|---|---:|---:|---|---|',
  ];
  for (const check of report.checks) {
    lines.push(`| \`${check.id}\` | ${check.severity} | ${check.status} | ${String(check.evidence).replaceAll('|', '\\|')} | ${String(check.nextAction).replaceAll('|', '\\|')} |`);
  }
  if (report.nextMoves.length > 0) {
    lines.push('', '## Next moves', '');
    for (const move of report.nextMoves) lines.push(`- ${move}`);
  }
  return `${lines.join('\n')}\n`;
}

function writeFileEnsuringDir(relative, text) {
  const output = absolute(relative);
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(output, text);
}

function isDirectRun() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    if (parsed.help) {
      printHelp();
      return;
    }
    const report = buildReport();
    if (parsed.write) writeFileEnsuringDir(parsed.write, `${JSON.stringify(report, null, 2)}\n`);
    if (parsed.markdown) writeFileEnsuringDir(parsed.markdown, renderMarkdown(report));
    if (parsed.json) {
      process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    } else {
      process.stdout.write(`bug sweep: ${report.status} (${report.summary.blocked} blocked, ${report.summary.warnings} warnings)\n`);
    }
    if (report.status === 'blocked' || (parsed.strict && report.summary.warnings > 0)) {
      process.exitCode = 1;
    }
  } catch (error) {
    process.stderr.write(`bug-sweep failed: ${error.message || error}\n`);
    process.exitCode = 1;
  }
}

if (isDirectRun()) main();

export { buildReport, renderMarkdown };
