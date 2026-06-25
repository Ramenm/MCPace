#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { repoRoot, readJson, readText } from './lib/project-metadata.mjs';

function normalize(relativePath) {
  return relativePath.split(path.sep).join('/');
}

function walkFiles(root, predicate = () => true) {
  const files = [];
  const stack = [root];
  while (stack.length > 0) {
    const current = stack.pop();
    if (!fs.existsSync(current)) continue;
    for (const entry of fs.readdirSync(current, { withFileTypes: true }).sort((left, right) => left.name.localeCompare(right.name))) {
      const full = path.join(current, entry.name);
      const relative = normalize(path.relative(repoRoot, full));
      if (relative.startsWith('node_modules/') || relative.startsWith('dist/') || relative.startsWith('.git/')) continue;
      if (entry.isDirectory()) stack.push(full);
      else if (entry.isFile() && predicate(full, relative)) files.push(full);
    }
  }
  return files.sort();
}

function lineOf(text, index) {
  return text.slice(0, Math.max(0, index)).split(/\r?\n/).length;
}

function check(id, ok, detail, severity = 'fail', evidence = []) {
  return { id, status: ok ? 'pass' : severity, detail, evidence };
}

function workflowFiles() {
  const dir = path.join(repoRoot, '.github', 'workflows');
  return walkFiles(dir, (_file, relative) => /\.ya?ml$/i.test(relative));
}

function runBlocks(yaml) {
  const lines = yaml.split(/\r?\n/);
  const blocks = [];
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    const match = line.match(/^(\s*)run:\s*(\|)?\s*(.*)$/);
    if (!match) continue;
    const indent = match[1].length;
    const start = i + 1;
    if (!match[2]) {
      blocks.push({ start, text: match[3] || '' });
      continue;
    }
    const body = [];
    for (let j = i + 1; j < lines.length; j += 1) {
      const next = lines[j];
      if (next.trim() !== '' && next.match(/^\s*/)[0].length <= indent) break;
      body.push(next);
      i = j;
    }
    blocks.push({ start, text: body.join('\n') });
  }
  return blocks;
}

function directExpressionInRunFindings() {
  const findings = [];
  const untrustedPattern = /\$\{\{\s*(?:github\.event|github\.head_ref|github\.ref_name|github\.ref|inputs\.|env\.)[\s\S]*?\}\}/g;
  for (const file of workflowFiles()) {
    const relative = normalize(path.relative(repoRoot, file));
    const text = fs.readFileSync(file, 'utf8');
    for (const block of runBlocks(text)) {
      for (const match of block.text.matchAll(untrustedPattern)) {
        findings.push(`${relative}:${block.start + lineOf(block.text, match.index) - 1}: ${match[0]}`);
      }
    }
  }
  return findings;
}

function actionPinFindings() {
  const warnings = [];
  const usesPattern = /^\s*uses:\s*([^\s#]+)\s*$/gm;
  for (const file of workflowFiles()) {
    const relative = normalize(path.relative(repoRoot, file));
    const text = fs.readFileSync(file, 'utf8');
    for (const match of text.matchAll(usesPattern)) {
      const uses = match[1].replace(/^['"]|['"]$/g, '');
      if (uses.startsWith('./') || uses.startsWith('docker://')) continue;
      const ref = uses.includes('@') ? uses.slice(uses.lastIndexOf('@') + 1) : '';
      if (!/^[0-9a-f]{40}$/i.test(ref)) warnings.push(`${relative}:${lineOf(text, match.index)}: ${uses}`);
    }
  }
  return warnings;
}

function lifecycleScriptFindings() {
  const findings = [];
  const packagePaths = [
    'package.json',
    'packages/npm/cli/package.json',
  ];
  const lifecycle = /^(?:pre|post)?(?:install|prepare|publish|pack|shrinkwrap)$/;
  for (const relative of packagePaths) {
    const pkg = readJson(relative);
    for (const name of Object.keys(pkg.scripts || {})) {
      if (lifecycle.test(name)) findings.push(`${relative}: scripts.${name}`);
    }
  }
  return findings;
}

function npmInstallScriptFindings() {
  const findings = [];
  for (const file of workflowFiles()) {
    const relative = normalize(path.relative(repoRoot, file));
    const text = fs.readFileSync(file, 'utf8');
    const pattern = /\bnpm\s+(?:ci|install)\b(?![^\n]*--ignore-scripts)/g;
    for (const match of text.matchAll(pattern)) {
      findings.push(`${relative}:${lineOf(text, match.index)}: ${match[0]}`);
    }
  }
  return findings;
}

function regexFindings() {
  const findings = [];
  const allowlistedDynamicRegex = new Set([
    // Internal helper constructs a regex from a fixed field-name supplied by project metadata callers.
    'scripts/lib/project-metadata.mjs',
  ]);
  const files = walkFiles(repoRoot, (_file, relative) => {
    if (!/\.(?:mjs|js|rs)$/.test(relative)) return false;
    if (relative.startsWith('tests/') || relative.includes('/test/') || relative.endsWith('/tests.rs')) return false;
    return relative.startsWith('src/') || relative.startsWith('scripts/') || relative.startsWith('packages/npm/cli/');
  });
  const dynamicPatterns = [/\bnew\s+RegExp\s*\(/g, /\bRegex::new\s*\(/g];
  for (const file of files) {
    const relative = normalize(path.relative(repoRoot, file));
    const text = fs.readFileSync(file, 'utf8');
    for (const pattern of dynamicPatterns) {
      for (const match of text.matchAll(pattern)) {
        if (!allowlistedDynamicRegex.has(relative)) {
          findings.push(`${relative}:${lineOf(text, match.index)}: dynamic regex construction requires explicit ReDoS review`);
        }
      }
    }
  }
  return findings;
}

function rustBrokenFragmentFindings() {
  const findings = [];
  const files = walkFiles(path.join(repoRoot, 'src'), (_file, relative) => relative.endsWith('.rs'));
  const forbidden = [
    { pattern: /let\s+body_bytes\s*=\s*if\s+parsed\s*\n\s*let\s+body_bytes\s*=\s*if\s+parsed/g, detail: 'duplicated parse_http_response binding' },
    { pattern: /Ok\(value\)\s*=>\s*value,\s*\n\s*Ok\(value\)\s*=>\s*value,/g, detail: 'duplicated match arm' },
  ];
  for (const file of files) {
    const relative = normalize(path.relative(repoRoot, file));
    const text = fs.readFileSync(file, 'utf8');
    for (const entry of forbidden) {
      for (const match of text.matchAll(entry.pattern)) {
        findings.push(`${relative}:${lineOf(text, match.index)}: ${entry.detail}`);
      }
    }
  }
  return findings;
}

function ssrfBoundaryFindings() {
  const findings = [];
  const dashboard = readText('src/dashboard/http_boundary.rs');
  const upstream = readText('src/upstream/http_runtime.rs');
  const dashboardRs = readText('src/dashboard.rs');
  const required = [
    [dashboard, /missing required Host header/, 'dashboard requires Host header'],
    [dashboard, /multiple Host headers are not allowed/, 'dashboard rejects duplicate Host headers'],
    [dashboard, /is_allowed_local_origin/, 'dashboard validates Origin'],
    [dashboard, /is_loopback_host/, 'dashboard restricts local authority to loopback'],
    [dashboardRs, /refusing to bind non-loopback host/, 'dashboard refuses non-loopback bind by default'],
    [dashboardRs, /validate_action_path_field/, 'dashboard validates action file paths'],
    [dashboardRs, /must be a local file path, not a remote URL/, 'dashboard rejects remote URL action paths'],
    [upstream, /HTTP upstream URL cannot be empty or contain whitespace\/control characters/, 'HTTP upstream rejects header-injection URL characters'],
    [upstream, /host_header:\s*String/, 'HTTP upstream uses sanitized Host header'],
    [upstream, /collect::<Vec<_>>\(\)/, 'HTTP upstream tries all resolved socket addresses'],
  ];
  for (const [text, pattern, detail] of required) {
    if (!pattern.test(text)) findings.push(detail);
  }
  return findings;
}

function nativeResolverFindings() {
  const resolver = readText('packages/npm/cli/lib/resolve-binary.js');
  const required = [
    [/must be an absolute path, not a cwd-relative binary override/, 'explicit native binary override must be absolute'],
    [/readRegularTextFileStable/, 'native package metadata is read through stable file helper'],
    [/O_NOFOLLOW/, 'native package metadata open uses no-follow where available'],
    [/packageJson\.mcpace\?\.target !== target\.key/, 'optional package target metadata is verified'],
    [/installed MCPace binary package version mismatch/, 'optional package version drift is rejected'],
    [/installed MCPace binary escapes package root/, 'optional binary realpath containment is enforced'],
  ];
  return required.filter(([pattern]) => !pattern.test(resolver)).map(([, detail]) => detail);
}

function publishTokenFallbackOk(workflow) {
  const tokenReference = /\b(?:NPM_TOKEN|NODE_AUTH_TOKEN|NPM_CONFIG_[A-Z0-9_]*TOKEN)\b/i;
  if (!tokenReference.test(workflow)) return true;
  const strippedAllowedBootstrapLines = workflow.replace(/^\s*NODE_AUTH_TOKEN:\s*\$\{\{\s*secrets\.NPM_TOKEN\s*\}\}\s*$/gm, '');
  return /environment:\s*npm-publish/.test(workflow) && !tokenReference.test(strippedAllowedBootstrapLines);
}

function publishWorkflowFindings() {
  const workflow = readText('.github/workflows/publish-npm.yml');
  const findings = [];
  const required = [
    [/id-token:\s*write/, 'publish workflow must request OIDC id-token: write for trusted publishing'],
    [/environment:\s*npm-publish/, 'publish workflow must use a protected publish environment'],
    [/startsWith\(github\.ref, 'refs\/tags\/'\)(?:\s*\|\|\s*\(github\.event_name == 'workflow_dispatch' && inputs\.dry_run == true\))?/, 'publish workflow must allow real publish only from tags; branch dispatch may run only as dry-run'],
    [/verify-npm-publish-contract\.mjs --enforce/, 'publish workflow must enforce native package contract'],
    [/npm exec --yes --package=npm@11\.13\.0 -- npm publish/, 'publish workflow must use pinned npm for publish'],
  ];
  for (const [pattern, detail] of required) {
    if (!pattern.test(workflow)) findings.push(detail);
  }
  if (!publishTokenFallbackOk(workflow)) {
    findings.push('publish workflow may use npm token fallback only as NODE_AUTH_TOKEN from NPM_TOKEN inside protected npm-publish environment for initial bootstrap');
  }
  for (const block of runBlocks(workflow)) {
    if (/\bnpm publish\b(?![^\n]*--access public)/.test(block.text)) {
      findings.push(`publish workflow run block at line ${block.start} has npm publish without --access public`);
    }
  }
  return findings;
}

const directExpressionFindings = directExpressionInRunFindings();
const actionPinWarnings = actionPinFindings();
const lifecycleFindings = lifecycleScriptFindings();
const npmInstallFindings = npmInstallScriptFindings();
const regexPolicyFindings = regexFindings();
const rustFragmentFindings = rustBrokenFragmentFindings();
const ssrfFindings = ssrfBoundaryFindings();
const nativeFindings = nativeResolverFindings();
const publishFindings = publishWorkflowFindings();

const checks = [
  check('github-actions-no-direct-expressions-in-run', directExpressionFindings.length === 0, 'Inline shell blocks must not interpolate untrusted GitHub expressions directly.', 'fail', directExpressionFindings),
  check('github-actions-full-sha-pinning', actionPinWarnings.length === 0, 'Third-party actions should be pinned to full commit SHA for immutable supply-chain references.', 'warn', actionPinWarnings),
  check('npm-package-has-no-install-lifecycle-scripts', lifecycleFindings.length === 0, 'Published npm package should not carry install/prepare/publish lifecycle scripts.', 'fail', lifecycleFindings),
  check('workflows-use-ignore-scripts-for-npm-install', npmInstallFindings.length === 0, 'CI workflow installs must use --ignore-scripts unless explicitly justified.', 'fail', npmInstallFindings),
  check('redos-policy-no-dynamic-or-nested-regex-in-production', regexPolicyFindings.length === 0, 'Production JS/Rust should not introduce dynamic or nested-quantifier regex without review.', 'fail', regexPolicyFindings),
  check('rust-source-no-known-broken-duplicate-fragments', rustFragmentFindings.length === 0, 'Rust source must not contain known duplicate fragments that compile only with a Rust gate.', 'fail', rustFragmentFindings),
  check('local-http-ssrf-and-csrf-boundaries-present', ssrfFindings.length === 0, 'Local HTTP/dashboard boundaries must keep Host/Origin/non-loopback/path guards.', 'fail', ssrfFindings),
  check('native-resolver-spoofing-and-toctou-guards-present', nativeFindings.length === 0, 'Native resolver must verify explicit overrides, metadata, target/version and containment.', 'fail', nativeFindings),
  check('npm-publish-workflow-trusted-publishing-shape', publishFindings.length === 0, 'Publish workflow must be OIDC/tag/protected-env/pinned-npm/native-contract shaped.', 'fail', publishFindings),
];

const hardFailures = checks.filter((entry) => entry.status === 'fail');
const warnings = checks.filter((entry) => entry.status === 'warn');
const payload = {
  schema: 'mcpace.securityPolicyCheck.v1',
  generatedAt: new Date().toISOString(),
  status: hardFailures.length > 0 ? 'fail' : warnings.length > 0 ? 'warn' : 'pass',
  hardFailures: hardFailures.length,
  warnings: warnings.length,
  checks,
};

console.log(JSON.stringify(payload, null, 2));
if (hardFailures.length > 0) process.exit(1);
