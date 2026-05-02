#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { deriveProjectName, deriveProjectVersion, extractTomlPackageField, readJson, readText, repoRoot } from './lib/project-metadata.mjs';

const DEFAULT_TOP_LIMIT = 20;
const SKIP_DIRS = new Set(['.git','node_modules','target','dist','backups','logs','data','vendor']);
const SKIP_PREFIXES = ['.tmp-', 'tmp-'];
const TEXT_EXTENSIONS = new Set(['.rs','.js','.mjs','.json','.md','.toml','.yml','.yaml','.txt','.html','.css','.sh','.cmd','.ps1','.lock']);

function parseArgs(argv) {
  const parsed = { json: false, write: null, markdown: null, top: DEFAULT_TOP_LIMIT, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    switch (token) {
      case '--json': parsed.json = true; break;
      case '--write': parsed.write = argv[++index] || null; if (!parsed.write) throw new Error('inventory-source requires a path after --write'); break;
      case '--markdown': parsed.markdown = argv[++index] || null; if (!parsed.markdown) throw new Error('inventory-source requires a path after --markdown'); break;
      case '--top': {
        const value = Number.parseInt(argv[++index] || '', 10);
        if (!Number.isFinite(value) || value < 1) throw new Error('inventory-source requires a positive integer after --top');
        parsed.top = value;
        break;
      }
      case '-h': case '--help': parsed.help = true; break;
      default: throw new Error(`unsupported inventory-source argument: ${token}`);
    }
  }
  return parsed;
}

function printHelp() {
  process.stdout.write('Usage: node scripts/inventory-source.mjs [--json] [--write <path>] [--markdown <path>] [--top <n>]\n\nCreates a deterministic source inventory: file counts, large files, version drift, release-manifest coverage, and first-use assets.\n');
}
function shouldSkipDir(name) { return SKIP_DIRS.has(name) || SKIP_PREFIXES.some((prefix) => name.startsWith(prefix)); }
function normalizeRelative(filePath) { return path.relative(repoRoot, filePath).split(path.sep).join('/'); }
function walkFiles() {
  const files = [];
  const stack = [repoRoot];
  while (stack.length > 0) {
    const current = stack.pop();
    const entries = fs.readdirSync(current, { withFileTypes: true }).sort((left, right) => left.name.localeCompare(right.name));
    for (const entry of entries) {
      const fullPath = path.join(current, entry.name);
      const relative = normalizeRelative(fullPath);
      if (entry.isDirectory()) {
        if (!shouldSkipDir(entry.name)) stack.push(fullPath);
      } else if (entry.isFile()) {
        if (!relative.endsWith('.zip') && !relative.endsWith('.tgz') && !relative.endsWith('.tar')) files.push({ path: relative, absolutePath: fullPath, ext: path.extname(entry.name).toLowerCase() || '[none]' });
      }
    }
  }
  return files.sort((left, right) => left.path.localeCompare(right.path));
}
function categoryFor(relativePath) {
  if (relativePath.startsWith('src/')) return 'rust-source';
  if (relativePath.startsWith('tests/')) return 'tests';
  if (relativePath.startsWith('packages/npm/cli/')) return 'npm-cli';
  if (relativePath.startsWith('packages/npm/')) return 'npm-platform-packages';
  if (relativePath.startsWith('scripts/')) return 'scripts';
  if (relativePath.startsWith('docs/')) return 'docs';
  if (relativePath.startsWith('reports/')) return 'reports';
  if (relativePath.startsWith('memory-bank/')) return 'memory-bank';
  if (relativePath.startsWith('presets/')) return 'presets';
  if (relativePath.startsWith('schemas/')) return 'schemas';
  if (relativePath.startsWith('eval/')) return 'eval';
  if (relativePath.startsWith('examples/')) return 'examples';
  if (relativePath.startsWith('.github/')) return 'github';
  if (relativePath.startsWith('.config/')) return 'config';
  return 'root-or-other';
}
function countLines(file) {
  if (!TEXT_EXTENSIONS.has(file.ext)) return null;
  try { const text = fs.readFileSync(file.absolutePath, 'utf8'); return text.length === 0 ? 0 : text.split(/\r?\n/).length; } catch { return null; }
}
function increment(map, key, amount = 1) { map[key] = (map[key] || 0) + amount; }
function readMaybeJson(relativePath) { try { return readJson(relativePath); } catch (error) { return { __error: error instanceof Error ? error.message : String(error) }; } }
function collectVersions() {
  const expectedVersion = deriveProjectVersion();
  const entries = [];
  function push(name, value, file) { entries.push({ name, version: value || null, file, matchesExpected: value === expectedVersion }); }
  push('cargo-package', extractTomlPackageField(readText('Cargo.toml'), 'version'), 'Cargo.toml');
  push('workspace-package', readMaybeJson('package.json').version, 'package.json');
  push('npm-cli', readMaybeJson('packages/npm/cli/package.json').version, 'packages/npm/cli/package.json');
  push('mcpace-config', readMaybeJson('mcpace.config.json').version, 'mcpace.config.json');
  const platformDir = path.join(repoRoot, 'packages', 'npm');
  if (fs.existsSync(platformDir)) {
    for (const entry of fs.readdirSync(platformDir).sort()) {
      if (!entry.startsWith('cli-')) continue;
      const packagePath = `packages/npm/${entry}/package.json`;
      if (fs.existsSync(path.join(repoRoot, packagePath))) push(`npm-platform:${entry}`, readMaybeJson(packagePath).version, packagePath);
    }
  }
  return { expectedVersion, entries, drift: entries.filter((entry) => !entry.matchesExpected) };
}
function collectReleaseManifest() {
  const manifest = readMaybeJson('release-manifest.json');
  const includePaths = Array.isArray(manifest.includePaths) ? manifest.includePaths : [];
  const optionalIncludePaths = Array.isArray(manifest.optionalIncludePaths) ? manifest.optionalIncludePaths : [];
  return {
    includePathCount: includePaths.length,
    optionalIncludePathCount: optionalIncludePaths.length,
    missing: includePaths.filter((relativePath) => !fs.existsSync(path.join(repoRoot, relativePath))),
    optionalPresent: optionalIncludePaths.filter((relativePath) => fs.existsSync(path.join(repoRoot, relativePath))),
    optionalMissing: optionalIncludePaths.filter((relativePath) => !fs.existsSync(path.join(repoRoot, relativePath))),
  };
}
function collectPresetSummary() {
  const catalogPath = path.join(repoRoot, 'presets', 'mcp-servers.json');
  if (!fs.existsSync(catalogPath)) return { status: 'missing', presetCount: 0, starterPresetCount: 0, ids: [] };
  try {
    const catalog = JSON.parse(fs.readFileSync(catalogPath, 'utf8'));
    const presets = Array.isArray(catalog.presets) ? catalog.presets : [];
    return { status: 'ok', schema: catalog.$schema || null, version: catalog.version || null, presetCount: presets.length, ids: presets.map((preset) => preset.id).filter(Boolean).sort(), starterPresetCount: Array.isArray(catalog.starter?.presets) ? catalog.starter.presets.length : 0 };
  } catch (error) {
    return { status: 'parse-error', error: error instanceof Error ? error.message : String(error), presetCount: 0, starterPresetCount: 0, ids: [] };
  }
}
function collectBootAssets() {
  const required = ['README.md','docs/README.md','docs/universal-mcp-connectivity.md','docs/mcp-http-api-spec.md','presets/mcp-servers.json','mcp_settings.d/README.md','schemas/mcpace-config.schema.json','reports/summary.md','memory-bank/activeContext.md'];
  return required.map((relativePath) => ({ relativePath, exists: fs.existsSync(path.join(repoRoot, relativePath)) }));
}
export function inventorySource(options = {}) {
  const topLimit = options.top || DEFAULT_TOP_LIMIT;
  const files = walkFiles();
  const byExtension = {};
  const byCategory = {};
  const rustModules = [];
  const largestTextFiles = [];
  let textFileCount = 0;
  let totalTextLines = 0;
  for (const file of files) {
    increment(byExtension, file.ext);
    increment(byCategory, categoryFor(file.path));
    const lines = countLines(file);
    if (lines !== null) {
      textFileCount += 1; totalTextLines += lines;
      largestTextFiles.push({ path: file.path, lines, category: categoryFor(file.path) });
      if (file.path.startsWith('src/') && file.ext === '.rs') rustModules.push({ path: file.path, lines });
    }
  }
  largestTextFiles.sort((left, right) => right.lines - left.lines || left.path.localeCompare(right.path));
  rustModules.sort((left, right) => right.lines - left.lines || left.path.localeCompare(right.path));
  const versions = collectVersions();
  const releaseManifest = collectReleaseManifest();
  const presets = collectPresetSummary();
  const bootAssets = collectBootAssets();
  const missingBootAssets = bootAssets.filter((asset) => !asset.exists).map((asset) => asset.relativePath);
  const warnings = [];
  if (versions.drift.length > 0) warnings.push(`version drift detected in ${versions.drift.length} file(s)`);
  if (releaseManifest.missing.length > 0) warnings.push(`release manifest missing ${releaseManifest.missing.length} required path(s)`);
  if (presets.status !== 'ok') warnings.push(`preset catalog status is ${presets.status}`);
  if (missingBootAssets.length > 0) warnings.push(`missing first-use assets: ${missingBootAssets.join(', ')}`);
  return {
    schema: 'mcpace.sourceInventory.v1', generatedAt: new Date().toISOString(), project: { name: deriveProjectName(), version: versions.expectedVersion },
    summary: { totalFiles: files.length, textFiles: textFileCount, totalTextLines, rustFiles: files.filter((file) => file.ext === '.rs').length, nodeFiles: files.filter((file) => file.ext === '.js' || file.ext === '.mjs').length, markdownFiles: files.filter((file) => file.ext === '.md').length, jsonFiles: files.filter((file) => file.ext === '.json').length, testFiles: files.filter((file) => file.path.startsWith('tests/') || file.path.includes('/test/')).length, docsFiles: files.filter((file) => file.path.startsWith('docs/')).length, reportsFiles: files.filter((file) => file.path.startsWith('reports/')).length, schemaFiles: files.filter((file) => file.path.startsWith('schemas/')).length, presetCatalogs: presets.status === 'ok' ? 1 : 0 },
    byExtension, byCategory, largestTextFiles: largestTextFiles.slice(0, topLimit), largestRustModules: rustModules.slice(0, topLimit), versions, releaseManifest, presets,
    firstUseAssets: { required: bootAssets, missing: missingBootAssets }, ok: warnings.length === 0, warnings,
  };
}
export function renderInventoryMarkdown(report) {
  const lines = ['# MCPace source inventory','',`Generated: ${report.generatedAt}`,'',`Project: \`${report.project.name}\` v\`${report.project.version}\``,'',`Status: **${report.ok ? 'ok' : 'attention needed'}**`,'','## Summary','','| metric | value |','|---|---:|'];
  for (const [key, value] of Object.entries(report.summary)) lines.push(`| ${key} | ${value} |`);
  lines.push('','## Categories','','| category | files |','|---|---:|');
  for (const [key, value] of Object.entries(report.byCategory).sort(([left], [right]) => left.localeCompare(right))) lines.push(`| ${key} | ${value} |`);
  lines.push('','## Largest Rust modules','','| file | lines |','|---|---:|');
  for (const entry of report.largestRustModules.slice(0, 15)) lines.push(`| ${entry.path} | ${entry.lines} |`);
  lines.push('','## Version drift','');
  if (report.versions.drift.length === 0) lines.push('No version drift detected across Cargo, root package, CLI package, platform packages, and `mcpace.config.json`.');
  else { lines.push('| name | file | version | expected |','|---|---|---|---|'); for (const entry of report.versions.drift) lines.push(`| ${entry.name} | ${entry.file} | ${entry.version || 'missing'} | ${report.versions.expectedVersion} |`); }
  lines.push('','## Release manifest','');
  lines.push(report.releaseManifest.missing.length === 0 ? 'All required `release-manifest.json` include paths exist.' : `Missing required include paths: ${report.releaseManifest.missing.join(', ')}`);
  lines.push('','## Presets','',`Preset catalog status: \`${report.presets.status}\`; presets: ${report.presets.presetCount}; starter entries: ${report.presets.starterPresetCount}.`);
  if (report.presets.ids.length > 0) lines.push(`Preset ids: ${report.presets.ids.map((id) => `\`${id}\``).join(', ')}.`);
  if (report.warnings.length > 0) { lines.push('','## Warnings',''); for (const warning of report.warnings) lines.push(`- ${warning}`); }
  lines.push(''); return `${lines.join('\n')}\n`;
}
function writeFileEnsuringDir(relativeOrAbsolutePath, contents) { const target = path.resolve(relativeOrAbsolutePath); fs.mkdirSync(path.dirname(target), { recursive: true }); fs.writeFileSync(target, contents, 'utf8'); }
function isCliInvocation() { const entry = process.argv[1]; return entry ? pathToFileURL(path.resolve(entry)).href === import.meta.url : false; }
function main() {
  try {
    const parsed = parseArgs(process.argv.slice(2));
    if (parsed.help) { printHelp(); return; }
    const report = inventorySource(parsed);
    if (parsed.write) writeFileEnsuringDir(parsed.write, `${JSON.stringify(report, null, 2)}\n`);
    if (parsed.markdown) writeFileEnsuringDir(parsed.markdown, renderInventoryMarkdown(report));
    if (parsed.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    else { process.stdout.write(`${report.ok ? 'ok' : 'attention needed'}: ${report.summary.totalFiles} files, ${report.summary.rustFiles} Rust, ${report.summary.nodeFiles} Node\n`); for (const warning of report.warnings) process.stdout.write(`warning: ${warning}\n`); }
  } catch (error) { process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`); process.exit(1); }
}
if (isCliInvocation()) main();
