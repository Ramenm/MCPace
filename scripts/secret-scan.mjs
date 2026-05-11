#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { deriveProjectName, deriveProjectVersion, repoRoot } from './lib/project-metadata.mjs';

const DEFAULT_IGNORE_DIRS = new Set(['.git', 'target', 'node_modules', '.venv', 'dist', 'coverage']);
const DEFAULT_IGNORE_EXTS = new Set(['.zip', '.tgz', '.gz', '.br', '.png', '.jpg', '.jpeg', '.gif', '.webp', '.ico', '.exe', '.dll', '.dylib', '.so', '.a', '.rlib', '.rmeta', '.pdf']);
const MAX_FILE_BYTES = 2 * 1024 * 1024;

const RULES = [
  { id: 'private-key', severity: 'critical', pattern: /-----BEGIN (?:RSA |DSA |EC |OPENSSH |PGP )?PRIVATE KEY-----/g },
  { id: 'github-token', severity: 'critical', pattern: /\bgh[pousr]_[A-Za-z0-9_]{30,}\b/g },
  { id: 'npm-token', severity: 'critical', pattern: /\bnpm_[A-Za-z0-9]{30,}\b/g },
  { id: 'aws-access-key', severity: 'critical', pattern: /\bA(?:KIA|SIA)[0-9A-Z]{16}\b/g },
  { id: 'openai-key', severity: 'critical', pattern: /\bsk-[A-Za-z0-9]{40,}\b/g },
  { id: 'stripe-key', severity: 'critical', pattern: /\b(?:sk|pk)_(?:live|test)_[A-Za-z0-9]{24,}\b/g },
  { id: 'slack-token', severity: 'critical', pattern: /\bxox[baprs]-[A-Za-z0-9-]{20,}\b/g },
  { id: 'authorization-header-literal', severity: 'warning', pattern: /Authorization\s*[:=]\s*(?:Bearer|Basic)\s+[A-Za-z0-9._~+\/-]{20,}/gi },
];

function parseArgs(argv) {
  const out = { json: false, write: null, markdown: null, strict: false, help: false };
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (a === '--json') out.json = true;
    else if (a === '--write') out.write = argv[++i] || null;
    else if (a === '--markdown' || a === '--write-md') out.markdown = argv[++i] || null;
    else if (a === '--strict') out.strict = true;
    else if (a === '-h' || a === '--help') out.help = true;
    else throw new Error(`unsupported secret-scan argument: ${a}`);
  }
  return out;
}

function shouldSkip(relative, dirent = null) {
  const normalized = relative.split(path.sep).join('/');
  const parts = normalized.split('/');
  if (parts.some((part) => DEFAULT_IGNORE_DIRS.has(part))) return true;
  if (/reports\/(?:.*runtime.*|.*secret.*|.*local.*|.*publish.*)\.json$/.test(normalized)) return false;
  if (normalized.startsWith('reports/') && normalized.endsWith('.md')) return true;
  const ext = path.extname(normalized).toLowerCase();
  if (DEFAULT_IGNORE_EXTS.has(ext)) return true;
  if (dirent?.isSymbolicLink?.()) return true;
  return false;
}

function listFiles(dir = repoRoot, prefix = '') {
  const files = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const relative = path.join(prefix, entry.name);
    if (shouldSkip(relative, entry)) continue;
    const absolute = path.join(dir, entry.name);
    if (entry.isDirectory()) files.push(...listFiles(absolute, relative));
    else if (entry.isFile()) files.push(relative.split(path.sep).join('/'));
  }
  return files.sort();
}

function isTextFile(abs) {
  const stat = fs.statSync(abs);
  if (stat.size > MAX_FILE_BYTES) return false;
  const sample = fs.readFileSync(abs, { encoding: null }).subarray(0, Math.min(stat.size, 8192));
  return !sample.includes(0);
}

function redact(value) {
  const s = String(value || '');
  if (s.length <= 12) return '<redacted>';
  return `${s.slice(0, 4)}…${s.slice(-4)}`;
}

function scanFile(relative) {
  const absolute = path.join(repoRoot, relative);
  if (!isTextFile(absolute)) return [];
  const text = fs.readFileSync(absolute, 'utf8');
  const lines = text.split(/\r?\n/);
  const findings = [];
  for (const rule of RULES) {
    rule.pattern.lastIndex = 0;
    let match;
    while ((match = rule.pattern.exec(text))) {
      const upto = text.slice(0, match.index);
      const line = upto.split(/\r?\n/).length;
      const column = match.index - upto.lastIndexOf('\n');
      const lineText = lines[line - 1] || '';
      if (/example|placeholder|redacted|dummy/i.test(lineText) && rule.severity !== 'critical') continue;
      findings.push({ ruleId: rule.id, severity: rule.severity, file: relative, line, column, match: redact(match[0]) });
    }
  }
  return findings;
}

function buildReport() {
  const files = listFiles();
  const findings = files.flatMap(scanFile);
  const critical = findings.filter((f) => f.severity === 'critical').length;
  const warnings = findings.filter((f) => f.severity !== 'critical').length;
  return {
    schema: 'mcpace.secretScan.v1',
    generatedAt: new Date().toISOString(),
    project: { name: deriveProjectName(), version: deriveProjectVersion() },
    status: critical > 0 ? 'fail' : warnings > 0 ? 'pass-with-warnings' : 'pass',
    scannedFiles: files.length,
    summary: { findings: findings.length, critical, warnings },
    findings,
    nextActions: critical > 0 ? ['Remove secrets from the tree, rotate exposed credentials, and rerun the scan before publishing.'] : warnings > 0 ? ['Review warning-level literals before publishing.'] : [],
  };
}

function renderMarkdown(report) {
  return [
    '# MCPace local secret scan', '',
    `Project: \`${report.project.name}\` v\`${report.project.version}\``,
    `Status: \`${report.status}\``,
    `Scanned files: \`${report.scannedFiles}\``,
    '', '| severity | rule | file | line | evidence |', '|---|---|---|---:|---|',
    ...(report.findings.length ? report.findings.map((f) => `| ${f.severity} | ${f.ruleId} | ${f.file} | ${f.line} | ${f.match} |`) : ['| - | - | - | - | no findings |']),
    report.nextActions.length ? '\n## Next actions\n' : '',
    ...report.nextActions.map((action) => `- ${action}`), '',
  ].filter((line) => line !== '').join('\n');
}

function writeArtifacts(report, opts) {
  for (const [file, contents] of [[opts.write, `${JSON.stringify(report, null, 2)}\n`], [opts.markdown, renderMarkdown(report)]]) {
    if (!file) continue;
    const target = path.resolve(repoRoot, file);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, contents, 'utf8');
  }
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) {
    process.stdout.write('Usage: node scripts/secret-scan.mjs [--json] [--write <path>] [--markdown <path>] [--strict]\n');
    return;
  }
  const report = buildReport();
  writeArtifacts(report, opts);
  process.stdout.write(opts.json ? `${JSON.stringify(report, null, 2)}\n` : renderMarkdown(report));
  if ((opts.strict || true) && report.summary.critical > 0) process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(); } catch (err) { process.stderr.write(`${err.message || err}\n`); process.exitCode = 1; }
}

export { buildReport, renderMarkdown };
