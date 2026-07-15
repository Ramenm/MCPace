#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { repoRoot as defaultRepoRoot } from './lib/project-metadata.mjs';
import { listWorkingTreeFiles } from './lib/repo-files.mjs';

const SKIP_DIRS = new Set(['.git', 'node_modules', 'target', 'dist', '.artifacts']);
const TEXT_EXTENSIONS = new Set([
  '.rs', '.mjs', '.js', '.json', '.md', '.toml', '.yml', '.yaml', '.ts', '.tsx', '.jsx', '.css', '.html', '.ps1', '.sh', '.py', '.sql', '.csv', '.txt', '.lock', '.npmrc', '.gitattributes', '.gitignore', '.editorconfig', '',
]);

const LARGE_RUST_PRODUCTION_LINES = 1200;
const LARGE_RUST_TEST_LINES = 900;
const LARGE_SCRIPT_LINES = 700;
const TOP_LIMIT = 25;

function parseArgs(argv) {
  const args = { json: false, repoRoot: defaultRepoRoot };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--json') args.json = true;
    else if (arg === '--repo') args.repoRoot = path.resolve(argv[++index]);
    else if (arg === '--help' || arg === '-h') {
      console.log('Usage: node scripts/architecture-debt-inventory.mjs [--json] [--repo DIR]');
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


function readText(file) {
  return fs.readFileSync(file, 'utf8');
}

function lineCount(source) {
  const normalized = source.replace(/\r\n/g, '\n').replace(/\n$/, '');
  return normalized.length === 0 ? 0 : normalized.split('\n').length;
}

function productionRustSource(source) {
  return source.replace(/#\[cfg\(test\)\]\s*mod\s+tests\s*\{[\s\S]*$/m, '');
}

function isRustTestFile(relative) {
  return relative.endsWith('/tests.rs') || relative.endsWith('_test.rs') || relative.endsWith('_tests.rs');
}

function relative(repoRoot, file) {
  return normalize(path.relative(repoRoot, file));
}

function topBy(items, key, limit = TOP_LIMIT) {
  return [...items].sort((left, right) => right[key] - left[key] || left.file.localeCompare(right.file)).slice(0, limit);
}

function countMatches(source, pattern) {
  return [...source.matchAll(pattern)].length;
}

function rustModuleMetrics(repoRoot, files) {
  const rustFiles = files.filter((file) => file.endsWith('.rs'));
  const metrics = rustFiles.map((file) => {
    const source = readText(file);
    const rel = relative(repoRoot, file);
    const production = productionRustSource(source);
    return {
      file: rel,
      lines: lineCount(source),
      productionLines: isRustTestFile(rel) ? 0 : lineCount(production),
      testLines: isRustTestFile(rel) ? lineCount(source) : Math.max(0, lineCount(source) - lineCount(production)),
      publicItems: countMatches(production, /^\s*pub\s+(?:async\s+)?(?:fn|struct|enum|trait|mod|type|const|static)\b/gm),
      crateItems: countMatches(production, /^\s*pub\(crate\)\s+(?:async\s+)?(?:fn|struct|enum|trait|mod|type|const|static)\b/gm),
      functions: countMatches(production, /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+[A-Za-z0-9_]+\s*\(/gm),
      inlineTestModule: /#\[cfg\(test\)\]\s*mod\s+tests\s*\{/.test(source),
      externalTestModule: /#\[cfg\(test\)\]\s*mod\s+tests\s*;/.test(source),
    };
  });
  return {
    rustFiles: rustFiles.length,
    totalRustLines: metrics.reduce((sum, item) => sum + item.lines, 0),
    totalProductionRustLines: metrics.reduce((sum, item) => sum + item.productionLines, 0),
    largeProductionFiles: metrics.filter((item) => item.productionLines > LARGE_RUST_PRODUCTION_LINES),
    largeTestFiles: metrics.filter((item) => item.lines > LARGE_RUST_TEST_LINES && isRustTestFile(item.file)),
    inlineTestModules: metrics.filter((item) => item.inlineTestModule).map((item) => item.file),
    topRustFiles: topBy(metrics, 'lines'),
    topProductionRustFiles: topBy(metrics, 'productionLines'),
  };
}

function scriptMetrics(repoRoot, files) {
  const scriptFiles = files.filter((file) => file.endsWith('.mjs') || file.endsWith('.js'));
  const metrics = scriptFiles.map((file) => {
    const source = readText(file);
    return {
      file: relative(repoRoot, file),
      lines: lineCount(source),
      functions: countMatches(source, /\bfunction\s+[A-Za-z0-9_]+\s*\(|=>\s*\{/g),
    };
  });
  return {
    scriptFiles: scriptFiles.length,
    largeScriptFiles: metrics.filter((item) => item.lines > LARGE_SCRIPT_LINES),
    topScriptFiles: topBy(metrics, 'lines'),
  };
}

function libSurface(repoRoot) {
  const file = path.join(repoRoot, 'src', 'lib.rs');
  const source = fs.existsSync(file) ? readText(file) : '';
  const publicModules = [...source.matchAll(/^pub\s+mod\s+([A-Za-z0-9_]+)\s*;/gm)].map((match) => match[1]);
  const crateModules = [...source.matchAll(/^pub\(crate\)\s+mod\s+([A-Za-z0-9_]+)\s*;/gm)].map((match) => match[1]);
  return {
    publicModules,
    crateModules,
    publicModuleCount: publicModules.length,
    crateModuleCount: crateModules.length,
    recommendation: publicModules.length > 10
      ? 'treat src/lib.rs as an internal binary crate boundary: keep app::run public, move subsystem modules to pub(crate) unless an external API contract exists'
      : 'public surface is already narrow',
  };
}

function collectTextFiles(repoRoot, files) {
  return files.filter((file) => {
    const rel = relative(repoRoot, file);
    if (rel.startsWith('reports/') || rel.startsWith('eval/random-') || rel === 'package-lock.json') return false;
    const ext = path.extname(file);
    return TEXT_EXTENSIONS.has(ext) || path.basename(file).startsWith('.');
  });
}

function legacyMarkers(repoRoot, files) {
  const textFiles = collectTextFiles(repoRoot, files);
  const patterns = {
    legacy: /\blegacy\b/gi,
    compat: /\bcompat(?:ibility)?\b/gi,
    shim: /\bshim\b/gi,
    fallback: /\bfallback\b/gi,
    deprecated: /\bdeprecated\b/gi,
    todo: /\b(?:TODO|FIXME|HACK)\b/g,
  };
  const perFile = [];
  const totals = Object.fromEntries(Object.keys(patterns).map((key) => [key, 0]));
  for (const file of textFiles) {
    let source;
    try {
      source = readText(file);
    } catch {
      continue;
    }
    const counts = Object.fromEntries(Object.entries(patterns).map(([key, pattern]) => {
      pattern.lastIndex = 0;
      const count = countMatches(source, pattern);
      totals[key] += count;
      return [key, count];
    }));
    const count = Object.values(counts).reduce((sum, value) => sum + value, 0);
    if (count > 0) perFile.push({ file: relative(repoRoot, file), count, counts });
  }
  return {
    totals,
    files: topBy(perFile, 'count', 40),
    hotspotLiterals: literalHotspots(repoRoot, files),
  };
}

function literalHotspots(repoRoot, files) {
  const rustFiles = files.filter((file) => file.endsWith('.rs'));
  const literals = [
    'sse-legacy',
    'legacy-sse',
    'legacy-compat',
    'legacy-disabled',
    'legacy-compat-disabled',
    'stdio-shim',
    'mcpace-agent-launcher.exe',
  ];
  return literals.map((literal) => {
    const evidence = [];
    let count = 0;
    for (const file of rustFiles) {
      const source = readText(file);
      const matches = source.split(literal).length - 1;
      if (matches > 0) {
        count += matches;
        evidence.push(relative(repoRoot, file));
      }
    }
    return { literal, count, files: [...new Set(evidence)].sort() };
  }).filter((item) => item.count > 0);
}

function recommendedSplits(rust) {
  const known = new Map([
    ['src/dashboard/overview.rs', ['src/dashboard/overview/model.rs', 'src/dashboard/overview/collect.rs', 'src/dashboard/overview/render.rs', 'src/dashboard/overview/access_review.rs']],
    ['src/server/loader.rs', ['src/server/loader/schema.rs', 'src/server/loader/sources.rs', 'src/server/loader/normalize.rs', 'src/server/loader/classify.rs', 'src/server/loader/validate.rs']],
    ['src/dashboard.rs', ['src/dashboard/routes.rs', 'src/dashboard/server.rs', 'src/dashboard/state.rs', 'src/dashboard/static_assets.rs']],
    ['src/setup.rs', ['src/setup/home_import.rs', 'src/setup/client_install.rs', 'src/setup/run.rs']],
    ['src/service.rs', ['src/service/config.rs', 'src/service/autostart_plan.rs', 'src/service/platform.rs', 'src/service/verify.rs', 'src/service/report.rs']],
    ['src/serve.rs', ['src/serve/config.rs', 'src/serve/lifecycle.rs', 'src/serve/status.rs', 'src/serve/health.rs']],
    ['src/adapter/discovery.rs', ['src/adapter/discovery/search.rs', 'src/adapter/discovery/resources.rs', 'src/adapter/discovery/schema.rs']],
    ['src/upstream/lease_runtime.rs', ['src/upstream/lease_runtime/model.rs', 'src/upstream/lease_runtime/scheduler.rs', 'src/upstream/lease_runtime/guards.rs']],
  ]);
  return rust.topProductionRustFiles
    .filter((item) => item.productionLines > LARGE_RUST_PRODUCTION_LINES)
    .slice(0, 12)
    .map((item) => ({
      file: item.file,
      productionLines: item.productionLines,
      targetMaxLinesAfterSplit: item.file.includes('/tests') ? 900 : 700,
      suggestedModules: known.get(item.file) ?? [`${item.file.replace(/\.rs$/, '')}/model.rs`, `${item.file.replace(/\.rs$/, '')}/logic.rs`, `${item.file.replace(/\.rs$/, '')}/tests.rs`],
    }));
}

function run() {
  const args = parseArgs(process.argv.slice(2));
  const repoRoot = args.repoRoot;
  const files = listWorkingTreeFiles(repoRoot).filter((file) => {
    const relativePath = relative(repoRoot, file);
    return !relativePath.split('/').some((part) => SKIP_DIRS.has(part));
  });
  const rust = rustModuleMetrics(repoRoot, files);
  const scripts = scriptMetrics(repoRoot, files);
  const lib = libSurface(repoRoot);
  const legacy = legacyMarkers(repoRoot, files);
  const report = {
    schema: 'mcpace.architectureDebtInventory.v1',
    generatedAt: new Date().toISOString(),
    repoRoot: '.',
    thresholds: {
      largeRustProductionLines: LARGE_RUST_PRODUCTION_LINES,
      largeRustTestLines: LARGE_RUST_TEST_LINES,
      largeScriptLines: LARGE_SCRIPT_LINES,
    },
    summary: {
      files: files.length,
      rustFiles: rust.rustFiles,
      scriptFiles: scripts.scriptFiles,
      largeRustProductionFiles: rust.largeProductionFiles.length,
      largeRustTestFiles: rust.largeTestFiles.length,
      inlineTestModules: rust.inlineTestModules.length,
      publicModules: lib.publicModuleCount,
      legacyMarkerFiles: legacy.files.length,
    },
    rust,
    scripts,
    libSurface: lib,
    legacy,
    recommendedSplits: recommendedSplits(rust),
  };
  if (args.json) console.log(JSON.stringify(report, null, 2));
  else {
    console.log(`${report.schema}: ${report.summary.largeRustProductionFiles} large production Rust files, ${report.summary.inlineTestModules} inline test modules, ${report.summary.publicModules} public root modules`);
    for (const item of report.recommendedSplits.slice(0, 10)) {
      console.log(`- split ${item.file} (${item.productionLines} prod lines) -> ${item.suggestedModules.join(', ')}`);
    }
  }
}

try {
  run();
} catch (error) {
  console.error(error?.stack ?? String(error));
  process.exitCode = 1;
}
