#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { pathToFileURL } from 'node:url';
import { deriveProjectName, deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';

function parseArgs(argv) {
  const parsed = { json: false, write: null, markdown: null, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json':
        parsed.json = true;
        break;
      case '--write':
        parsed.write = argv[++index] || null;
        if (!parsed.write) throw new Error('defect-gates requires a path after --write');
        break;
      case '--write-md':
      case '--markdown':
        parsed.markdown = argv[++index] || null;
        if (!parsed.markdown) throw new Error('defect-gates requires a path after --markdown');
        break;
      case '-h':
      case '--help':
        parsed.help = true;
        break;
      default:
        throw new Error(`unsupported defect-gates argument: ${token}`);
    }
  }
  return parsed;
}

function printHelp() {
  process.stdout.write('Usage: node scripts/defect-gates.mjs [--json] [--write <path>] [--markdown <path>]\n\nChecks whether MCPace has the bug-finding, bug-fixing, and regression-prevention guardrails expected before public GitHub work.\n');
}

function readText(relativePath) {
  const absolutePath = path.join(repoRoot, relativePath);
  if (!fs.existsSync(absolutePath)) return null;
  return fs.readFileSync(absolutePath, 'utf8');
}

function fileExists(relativePath) {
  return fs.existsSync(path.join(repoRoot, relativePath));
}

function includesAll(text, patterns) {
  return patterns.every((pattern) => text?.includes(pattern));
}

function includesAny(text, patterns) {
  return patterns.some((pattern) => text?.includes(pattern));
}

function evaluateCheck(check) {
  const text = check.file ? readText(check.file) : null;
  let ok = true;
  const missing = [];

  if (check.file && text === null) {
    ok = false;
    missing.push(`missing file: ${check.file}`);
  }

  if (ok && check.mustInclude) {
    for (const pattern of check.mustInclude) {
      if (!text.includes(pattern)) {
        ok = false;
        missing.push(pattern);
      }
    }
  }

  if (ok && check.mustIncludeAny) {
    for (const group of check.mustIncludeAny) {
      if (!includesAny(text, group)) {
        ok = false;
        missing.push(`one of: ${group.join(' | ')}`);
      }
    }
  }

  if (ok && check.custom && !check.custom()) {
    ok = false;
    missing.push(check.customMessage || 'custom check failed');
  }

  return {
    id: check.id,
    title: check.title,
    severity: check.severity || 'required',
    file: check.file || null,
    status: ok ? 'pass' : check.optional ? 'warning' : 'fail',
    missing,
    recommendation: ok ? null : check.recommendation
  };
}

function packageScript(name) {
  try {
    const pkg = JSON.parse(readText('package.json'));
    return pkg.scripts?.[name] || null;
  } catch {
    return null;
  }
}

function collectChecks() {
  return [
    {
      id: 'bug-template-reproducibility',
      title: 'Bug reports require reproducibility, environment, severity, and regression context',
      file: '.github/ISSUE_TEMPLATE/bug_report.yml',
      mustInclude: [
        'id: version',
        'id: area',
        'id: severity',
        'id: platform',
        'id: reproduction',
        'id: expected',
        'id: actual',
        'id: regression',
        'id: proof'
      ],
      recommendation: 'Keep the bug template strict enough that maintainers can reproduce and classify reports without a long back-and-forth.'
    },
    {
      id: 'label-taxonomy',
      title: 'Repository has a label taxonomy for severity, area, type, and status triage',
      file: '.github/labels.yml',
      mustInclude: [
        'severity:p0',
        'severity:p1',
        'type:bug',
        'type:regression',
        'type:security',
        'area:mcp-http',
        'area:upstream-stdio',
        'area:upstream-http',
        'status:needs-repro',
        'status:needs-runtime-trace',
        'full-ci'
      ],
      recommendation: 'Publish labels so issue triage is consistent from the first public week.'
    },
    {
      id: 'bug-lifecycle-doc',
      title: 'Maintainers have a written bug lifecycle and fix standard',
      file: 'docs/bug-lifecycle.md',
      mustInclude: [
        'reproduce',
        'minimal failing test',
        'root cause',
        'regression guard',
        'runtime trace',
        'security report',
        'release note'
      ],
      recommendation: 'Document the repeatable path from report -> reproduction -> root cause -> fix -> regression proof -> release verification.'
    },
    {
      id: 'pr-template-regression-proof',
      title: 'Pull requests require proof for fixes and explicit not-tested disclosure',
      file: '.github/pull_request_template.md',
      mustInclude: [
        'Linked issue',
        'Regression test',
        'Runtime trace',
        'Not-tested',
        'Risks'
      ],
      recommendation: 'Every bugfix PR should say what failed before, what now guards it, and what was not tested.'
    },
    {
      id: 'ci-runs-defect-gates',
      title: 'CI runs defect gates alongside source and GitHub readiness checks',
      file: '.github/workflows/ci.yml',
      mustInclude: ['npm run verify:defect-gates', 'npm test', 'npm run verify:github-readiness', 'npm run verify:rust-quality'],
      recommendation: 'Wire defect gates into CI so future edits cannot silently remove the bug process.'
    },
    {
      id: 'package-script',
      title: 'npm script exposes the defect gate as a first-class verification command',
      custom: () => packageScript('verify:defect-gates') === 'node scripts/defect-gates.mjs --json --write reports/defect-gates-latest.json --markdown reports/defect-gates-latest.md',
      customMessage: 'missing or drifted package.json scripts.verify:defect-gates',
      recommendation: 'Add verify:defect-gates to package.json and keep it deterministic.'
    },
    {
      id: 'session-fixation-guard',
      title: 'MCP HTTP initialize generates server-owned session ids',
      file: 'src/dashboard/mcp_http.rs',
      mustInclude: ['let session_id = http_session::generated_mcp_http_session_id(request, &id, negotiated);'],
      recommendation: 'Do not register client-supplied Mcp-Session-Id values during initialize; generate a server-owned cryptographically random id.'
    },
    {
      id: 'null-origin-rejected',
      title: 'Local browser-origin guard rejects null Origin instead of treating it as local',
      file: 'src/dashboard/http_boundary.rs',
      mustInclude: ['if origin == "null" {', 'return false;'],
      recommendation: 'Reject null/file origins by default for local MCP HTTP routes; no Origin remains acceptable for native clients.'
    },
    {
      id: 'nonlocal-bind-guard',
      title: 'Local HTTP mode refuses non-loopback bind hosts unless explicitly opted in',
      file: 'src/dashboard.rs',
      mustInclude: ['--allow-nonlocal-bind', 'refusing to bind non-loopback host', 'is_loopback_bind_host'],
      recommendation: 'Keep local serve/dashboard loopback-only by default until a real public auth mode exists.'
    },
    {
      id: 'security-disclosure-path',
      title: 'Security issues have a private disclosure path and public policy',
      file: 'SECURITY.md',
      mustInclude: ['private', 'vulnerability', 'report'],
      recommendation: 'Keep SECURITY.md actionable and enable GitHub private vulnerability reporting after publishing.'
    },
    {
      id: 'source-audit-script',
      title: 'Source audit remains available for structural bug smells',
      file: 'scripts/audit-source.mjs',
      mustInclude: ['productionUnwraps', 'unsafeOperations', 'critical'],
      recommendation: 'Keep source-audit focused on hard blockers and measured warnings.'
    },
    {
      id: 'runtime-trace-gate',
      title: 'Runtime trace gate exists for behavioral bugs beyond static checks',
      file: 'scripts/runtime-trace-harness.mjs',
      mustInclude: ['runtimeTrace', 'tools/list', 'tools/call'],
      recommendation: 'Runtime bugs need executable traces, not only static manifests.'
    },
    {
      id: 'product-practice-freshness',
      title: 'Product-practice gate rejects stale runtime proof',
      file: 'scripts/product-practice-harness.mjs',
      mustInclude: ['stale', 'freshness', 'generatedAt'],
      recommendation: 'Do not accept old green reports as current runtime proof.'
    }
  ];
}

export function runDefectGates() {
  const checks = collectChecks().map(evaluateCheck);
  const blockers = checks.filter((check) => check.status === 'fail');
  const warnings = checks.filter((check) => check.status === 'warning');
  return {
    schema: 'mcpace.defectGates.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status: blockers.length > 0 ? 'blocked' : warnings.length > 0 ? 'ready-with-warnings' : 'pass',
    summary: {
      total: checks.length,
      passed: checks.filter((check) => check.status === 'pass').length,
      warnings: warnings.length,
      blockers: blockers.length
    },
    checks,
    operatingModel: {
      intake: 'Every bug starts with a reproducible issue, severity, affected area, version/platform, and expected-vs-actual behavior.',
      repair: 'Every fix gets a minimal failing test or runtime trace first, then root-cause notes, implementation, regression guard, and not-tested disclosure.',
      release: 'Bugfix releases are accepted only after source gates, Rust gates, runtime trace, npm/install readiness, and security gates are fresh.'
    }
  };
}

export function renderDefectGatesMarkdown(report) {
  const lines = [
    '# MCPace defect gates',
    '',
    `Generated: ${report.generatedAt}`,
    '',
    `Project: \`${report.project.name}\` v\`${report.project.version}\``,
    '',
    `Status: **${report.status}**`,
    '',
    '| Gate | Status | File |',
    '|---|---|---|'
  ];
  for (const check of report.checks) {
    lines.push(`| ${check.title} | ${check.status} | ${check.file || 'package/script'} |`);
  }
  const failing = report.checks.filter((check) => check.status !== 'pass');
  if (failing.length > 0) {
    lines.push('', '## Attention');
    for (const check of failing) {
      lines.push('', `### ${check.title}`, '', `Missing: ${check.missing.join(', ') || 'n/a'}`, '', `Recommendation: ${check.recommendation}`);
    }
  }
  lines.push('', '## Operating model', '');
  lines.push(`- Intake: ${report.operatingModel.intake}`);
  lines.push(`- Repair: ${report.operatingModel.repair}`);
  lines.push(`- Release: ${report.operatingModel.release}`);
  lines.push('');
  return `${lines.join('\n')}\n`;
}

function writeFileEnsuringDir(filePath, contents) {
  const target = path.resolve(filePath);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, contents, 'utf8');
}

function isCliInvocation() {
  const entry = process.argv[1];
  return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false;
}

function main() {
  try {
    const options = parseArgs(process.argv.slice(2));
    if (options.help) {
      printHelp();
      return;
    }
    const report = runDefectGates();
    if (options.write) writeFileEnsuringDir(options.write, `${JSON.stringify(report, null, 2)}\n`);
    if (options.markdown) writeFileEnsuringDir(options.markdown, renderDefectGatesMarkdown(report));
    if (options.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else process.stdout.write(`defect gates: ${report.status} (${report.summary.passed}/${report.summary.total} passed)\n`);
    if (report.status === 'blocked') process.exit(1);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}

if (isCliInvocation()) main();
