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

function exists(relativePath) {
  return fs.existsSync(rel(relativePath));
}

function read(relativePath) {
  return fs.readFileSync(rel(relativePath), 'utf8');
}

function walk(dir, predicate = () => true) {
  const root = rel(dir);
  if (!fs.existsSync(root)) return [];
  const found = [];
  const stack = [root];
  while (stack.length > 0) {
    const current = stack.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      if (entry.name === 'target' || entry.name === 'node_modules' || entry.name === '.git') continue;
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

function lineMatches(files, regex) {
  const matches = [];
  for (const file of files) {
    const lines = read(file).split(/\r?\n/);
    for (let index = 0; index < lines.length; index += 1) {
      if (regex.test(lines[index])) {
        matches.push({ file, line: index + 1, text: lines[index].trim() });
      }
    }
  }
  return matches;
}

function record(id, severity, status, details, extra = {}) {
  return { id, severity, status, details, ...extra };
}

const rustFiles = walk('src', (file) => file.endsWith('.rs'));
const nodeFiles = [
  ...walk('scripts', (file) => file.endsWith('.mjs') || file.endsWith('.js')),
  ...walk('tests/node', (file) => file.endsWith('.mjs') || file.endsWith('.js')),
  ...walk('packages/npm/cli', (file) => file.endsWith('.mjs') || file.endsWith('.js')),
];
const docsFiles = walk('docs', (file) => file.endsWith('.md'));
const projectFiles = [...rustFiles, ...nodeFiles, ...docsFiles, 'README.md', 'CHANGELOG.md', 'SECURITY.md']
  .filter(exists)
  .filter((file) => file !== 'scripts/audit-deep-risks.mjs');

const sourceAudit = exists('scripts/audit-source-bundle.mjs') ? JSON.parse(
  (await import('node:child_process')).execFileSync('node', ['scripts/audit-source-bundle.mjs', '--json'], {
    cwd: repoRoot,
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
  }),
) : null;

const releaseManifest = exists('release-manifest.json') ? JSON.parse(read('release-manifest.json')) : { includePaths: [] };
const missingManifestEntries = (releaseManifest.includePaths || []).filter((entry) => !exists(entry));
const scriptsReferencedByPackage = Object.values(JSON.parse(read('package.json')).scripts || {})
  .flatMap((script) => [...String(script).matchAll(/node\s+([^\s&|;]+)/g)].map((match) => match[1]))
  .map((script) => script.replace(/^['\"]|['\"]$/g, ''))
  .filter((script) => !script.startsWith('-'));
const missingPackageScripts = scriptsReferencedByPackage.filter((script) => !exists(script));
const todoMatches = lineMatches(projectFiles, /\b(TODO|FIXME|HACK)\b/i)
  .filter((match) => !match.file.startsWith('tests/'));
const notImplementedMatches = lineMatches(rustFiles, /not implemented yet|todo!\s*\(|unimplemented!\s*\(|panic!\s*\(/i)
  .filter((match) => !match.file.includes('/tests') && !match.file.endsWith('tests.rs'));
const unsafeMatches = lineMatches(rustFiles, /(^|[^A-Za-z_])unsafe\s*(\{|extern|fn|impl|trait)?/i)
  .filter((match) => !match.file.includes('/tests') && !match.file.endsWith('tests.rs') && !match.text.startsWith('//!') && !match.text.startsWith('//') && !match.text.includes('\"unsafe\"'));
const unsafeWithoutSafetyComment = unsafeMatches.filter((match) => {
  const lines = read(match.file).split(/\r?\n/);
  const start = Math.max(0, match.line - 6);
  const window = lines.slice(start, match.line + 2).join('\n');
  return !/SAFETY:/i.test(window);
});
const shellOutMatches = lineMatches(rustFiles, /std::process::Command|process::Command|Command::new/)
  .filter((match) => match.file !== 'src/cli_args.rs');
const envMutationMatches = lineMatches(rustFiles, /set_var\s*\(|remove_var\s*\(/);
const manualParserWarnings = sourceAudit?.warnings?.filter((warning) => warning.id === 'manual-cli-parsers-remaining') ?? [];
const bootstrapOnlySurfaces = lineMatches(rustFiles, /bootstrap-only proof surface|canForwardMcpToday|Live MCP stdio message forwarding is not implemented yet/i);

const findings = [
  record(
    'release-manifest-paths-exist',
    'high',
    missingManifestEntries.length === 0 ? 'pass' : 'fail',
    missingManifestEntries.length === 0
      ? 'Every release-manifest include path exists.'
      : `Missing release-manifest include paths: ${missingManifestEntries.join(', ')}`,
    { missingManifestEntries },
  ),
  record(
    'package-script-files-exist',
    'high',
    missingPackageScripts.length === 0 ? 'pass' : 'fail',
    missingPackageScripts.length === 0
      ? 'Every package.json node script target exists.'
      : `Missing node script targets referenced by package.json: ${missingPackageScripts.join(', ')}`,
    { missingPackageScripts },
  ),
  record(
    'source-audit-has-no-failures',
    'high',
    sourceAudit?.status === 'pass' ? 'pass' : 'fail',
    sourceAudit?.status === 'pass' ? 'Source audit has no failing checks.' : 'Source audit failed.',
    { sourceAuditStatus: sourceAudit?.status ?? 'missing' },
  ),
  record(
    'manual-cli-parsers-tracked',
    'medium',
    manualParserWarnings.length === 0 ? 'pass' : 'warn',
    manualParserWarnings.length === 0
      ? 'No remaining manual CLI parser warning in source audit.'
      : manualParserWarnings[0].details,
  ),
  record(
    'bootstrap-only-surfaces-tracked',
    'medium',
    bootstrapOnlySurfaces.length === 0 ? 'pass' : 'warn',
    bootstrapOnlySurfaces.length === 0
      ? 'No bootstrap-only product surfaces found.'
      : `${bootstrapOnlySurfaces.length} bootstrap-only stdio-shim references remain; keep them explicit until live forwarding is implemented.`,
    { matches: bootstrapOnlySurfaces.slice(0, 20) },
  ),
  record(
    'unsafe-blocks-have-safety-comments',
    'medium',
    unsafeWithoutSafetyComment.length === 0 ? 'pass' : 'warn',
    unsafeWithoutSafetyComment.length === 0
      ? 'Every unsafe occurrence has a nearby SAFETY comment window.'
      : `${unsafeWithoutSafetyComment.length} unsafe occurrences do not have a nearby SAFETY comment window.`,
    { matches: unsafeWithoutSafetyComment.slice(0, 40) },
  ),
  record(
    'todo-fixme-inventory',
    'low',
    todoMatches.length === 0 ? 'pass' : 'warn',
    todoMatches.length === 0
      ? 'No TODO/FIXME/HACK markers found in shipped sources/docs/scripts.'
      : `${todoMatches.length} TODO/FIXME/HACK markers found; review before public release.`,
    { matches: todoMatches.slice(0, 40) },
  ),
  record(
    'not-implemented-inventory',
    'low',
    notImplementedMatches.length === 0 ? 'pass' : 'warn',
    notImplementedMatches.length === 0
      ? 'No panic/todo/unimplemented/not-implemented markers found outside tests.'
      : `${notImplementedMatches.length} not-implemented/panic/todo markers found outside tests.`,
    { matches: notImplementedMatches.slice(0, 40) },
  ),
  record(
    'process-shellout-inventory',
    'low',
    shellOutMatches.length === 0 ? 'pass' : 'warn',
    shellOutMatches.length === 0
      ? 'No process spawn points found.'
      : `${shellOutMatches.length} process spawn points found; keep command argument construction non-shell and covered by tests.`,
    { matches: shellOutMatches.slice(0, 40) },
  ),
  record(
    'env-mutation-inventory',
    'low',
    envMutationMatches.length === 0 ? 'pass' : 'warn',
    envMutationMatches.length === 0
      ? 'No environment mutation points found in Rust source.'
      : `${envMutationMatches.length} environment mutation points found; keep them test-isolated.`,
    { matches: envMutationMatches.slice(0, 40) },
  ),
];

const failed = findings.filter((finding) => finding.status === 'fail');
const warned = findings.filter((finding) => finding.status === 'warn');
const result = {
  schema: 'mcpace.deepRiskAudit.v1',
  generatedAt: new Date().toISOString(),
  root: repoRoot,
  status: failed.length === 0 ? 'pass' : 'fail',
  summary: {
    rustFiles: rustFiles.length,
    nodeFiles: nodeFiles.length,
    docsFiles: docsFiles.length,
    failed: failed.length,
    warnings: warned.length,
    unsafeOccurrences: unsafeMatches.length,
    todoFixmeHackMarkers: todoMatches.length,
    notImplementedMarkers: notImplementedMatches.length,
    processSpawnPoints: shellOutMatches.length,
    envMutationPoints: envMutationMatches.length,
  },
  findings,
};

if (jsonOutput) {
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
} else {
  process.stdout.write(`MCPace deep risk audit: ${result.status}; warnings=${warned.length}; failures=${failed.length}\n`);
  for (const finding of findings) {
    process.stdout.write(`[${finding.status}] ${finding.id}: ${finding.details}\n`);
  }
}

if (failed.length > 0) {
  process.exitCode = 1;
}
