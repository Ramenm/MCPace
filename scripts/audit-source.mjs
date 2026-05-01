#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

const DEFAULT_INCLUDE_DIRS = ['src', 'scripts', 'packages/npm/cli', 'tests'];
const CODE_EXTENSIONS = new Set(['.rs', '.js', '.mjs']);
const LARGE_MODULE_LINE_THRESHOLD = 1500;
const MAX_WARNING_SAMPLES = 40;
const APPROVED_UNSAFE_RUST_FILES = new Set(['src/process_detach.rs', 'src/windows_process.rs']);

const ARCHITECTURE_BOUNDARIES = [
  {
    name: 'protocol primitives stay transport and command agnostic',
    file: 'src/mcp_protocol.rs',
    forbiddenPatterns: [
      { pattern: /crate::app\b|super::app\b/, reason: 'protocol primitives must not call the CLI router' },
      { pattern: /std::process::Command|Command::new\s*\(/, reason: 'protocol primitives must not spawn commands' },
      { pattern: /TcpListener|TcpStream|std::net::/, reason: 'protocol primitives must not own transport IO' },
      { pattern: /runtimepaths|hub::|server::|client::/, reason: 'protocol primitives must not depend on runtime state modules' },
    ],
  },
  {
    name: 'resource defaults stay side-effect free',
    file: 'src/resources.rs',
    forbiddenPatterns: [
      { pattern: /std::process::Command|Command::new\s*\(/, reason: 'resource defaults must not shell out' },
      { pattern: /TcpListener|TcpStream|std::net::/, reason: 'resource defaults must not own network sockets' },
      { pattern: /fs::|std::fs::/, reason: 'resource defaults must not read or write project state' },
    ],
  },
];

function parseArgs(argv) {
  const options = {
    json: false,
    failOnCritical: false,
    write: null,
    includeDirs: DEFAULT_INCLUDE_DIRS,
    help: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--json') {
      options.json = true;
    } else if (arg === '--fail-on-critical') {
      options.failOnCritical = true;
    } else if (arg === '--write') {
      const value = argv[index + 1];
      if (!value) {
        throw new Error('audit-source requires a path after --write');
      }
      options.write = value;
      index += 1;
    } else if (arg === '--include') {
      const value = argv[index + 1];
      if (!value) {
        throw new Error('audit-source requires a comma-separated list after --include');
      }
      options.includeDirs = value.split(',').map((entry) => entry.trim()).filter(Boolean);
      index += 1;
    } else if (arg === '-h' || arg === '--help') {
      options.help = true;
    } else {
      throw new Error(`unsupported audit-source argument: ${arg}`);
    }
  }

  return options;
}

function printHelp() {
  console.log(`Usage: node scripts/audit-source.mjs [--json] [--fail-on-critical] [--write <path>] [--include <dirs>]

Scans source files for architectural risk signals. Critical findings are kept
small and deterministic so this can run in CI without becoming a subjective
style linter. Warnings are advisory and intended for refactor planning.`);
}

function walkCodeFiles(includeDirs) {
  const files = [];
  const stack = includeDirs.map((relativePath) => path.join(repoRoot, relativePath));

  while (stack.length > 0) {
    const current = stack.pop();
    if (!current || !fs.existsSync(current)) {
      continue;
    }
    const stat = fs.statSync(current);
    if (stat.isDirectory()) {
      const base = path.basename(current);
      if (base === 'node_modules' || base === 'target' || base === 'dist' || base === '.git') {
        continue;
      }
      for (const entry of fs.readdirSync(current)) {
        stack.push(path.join(current, entry));
      }
      continue;
    }
    if (stat.isFile() && CODE_EXTENSIONS.has(path.extname(current))) {
      files.push(current);
    }
  }

  return files.sort();
}

function splitProductionAndTestRust(lines) {
  const cfgTestIndex = lines.findIndex((line) => /^\s*#\[cfg\(test\)\]/.test(line));
  if (cfgTestIndex === -1) {
    return { productionLines: lines, testLines: [] };
  }
  return {
    productionLines: lines.slice(0, cfgTestIndex),
    testLines: lines.slice(cfgTestIndex),
  };
}

function addFinding(collection, severity, filePath, lineNumber, message, excerpt) {
  collection.push({
    severity,
    file: path.relative(repoRoot, filePath).replaceAll('\\\\', '/'),
    line: lineNumber,
    message,
    excerpt: excerpt.trim(),
  });
}

function sampleWarnings(warnings) {
  if (warnings.length <= MAX_WARNING_SAMPLES) {
    return warnings;
  }
  return warnings.slice(0, MAX_WARNING_SAMPLES);
}

function countRegex(lines, regex) {
  let count = 0;
  for (const line of lines) {
    if (regex.test(line)) {
      count += 1;
    }
  }
  return count;
}

function auditArchitectureBoundaries(critical) {
  return ARCHITECTURE_BOUNDARIES.map((boundary) => {
    const filePath = path.join(repoRoot, boundary.file);
    const result = {
      name: boundary.name,
      file: boundary.file,
      ok: true,
      violations: [],
    };
    if (!fs.existsSync(filePath)) {
      result.ok = false;
      result.violations.push({ line: 0, reason: 'expected source file is missing', excerpt: boundary.file });
      addFinding(critical, 'critical', filePath, 0, `architecture boundary missing: ${boundary.name}`, boundary.file);
      return result;
    }

    const lines = fs.readFileSync(filePath, 'utf8').split(/\r?\n/);
    const { productionLines } = splitProductionAndTestRust(lines);
    productionLines.forEach((line, offset) => {
      for (const rule of boundary.forbiddenPatterns) {
        if (rule.pattern.test(line)) {
          const violation = {
            line: offset + 1,
            reason: rule.reason,
            excerpt: line.trim(),
          };
          result.ok = false;
          result.violations.push(violation);
          addFinding(
            critical,
            'critical',
            filePath,
            offset + 1,
            `architecture boundary violation: ${boundary.name}; ${rule.reason}`,
            line,
          );
        }
      }
    });
    return result;
  });
}

function audit(options = {}) {
  const includeDirs = options.includeDirs || DEFAULT_INCLUDE_DIRS;
  const files = walkCodeFiles(includeDirs);
  const critical = [];
  const warnings = [];
  const counters = {
    files: files.length,
    rustFiles: 0,
    nodeFiles: 0,
    productionRustLines: 0,
    testRustLines: 0,
    nodeLines: 0,
    largeModules: 0,
    directThreadSpawns: 0,
    commandSpawns: 0,
    productionUnwraps: 0,
    unsafeOperations: 0,
    foreignFunctionBlocks: 0,
  };

  for (const filePath of files) {
    const relative = path.relative(repoRoot, filePath).replaceAll('\\\\', '/');
    const text = fs.readFileSync(filePath, 'utf8');
    const lines = text.split(/\r?\n/);
    const ext = path.extname(filePath);

    if (ext === '.rs') {
      counters.rustFiles += 1;
      const { productionLines, testLines } = splitProductionAndTestRust(lines);
      counters.productionRustLines += productionLines.length;
      counters.testRustLines += testLines.length;
      counters.productionUnwraps += countRegex(productionLines, /\.unwrap\s*\(/);

      productionLines.forEach((line, offset) => {
        const lineNumber = offset + 1;
        if (relative.startsWith('src/') && /\b(todo!|unimplemented!)\s*\(/.test(line)) {
          addFinding(critical, 'critical', filePath, lineNumber, 'production Rust contains todo!/unimplemented!', line);
        }
        if (relative.startsWith('src/') && /\bpanic!\s*\(/.test(line)) {
          addFinding(critical, 'critical', filePath, lineNumber, 'production Rust contains panic!', line);
        }
        if (/thread::spawn\s*\(/.test(line)) {
          counters.directThreadSpawns += 1;
          if (relative.startsWith('src/') && !['src/dashboard.rs', 'src/upstream.rs', 'src/hub/lifecycle.rs'].includes(relative)) {
            addFinding(warnings, 'warning', filePath, lineNumber, 'direct thread spawn outside reviewed runtime fan-out modules', line);
          }
        }
        if (/Command::new\s*\(/.test(line)) {
          counters.commandSpawns += 1;
        }
        if (/\bunsafe\s*(?:\{|fn\b|impl\b|trait\b)/.test(line)) {
          counters.unsafeOperations += 1;
          if (relative.startsWith('src/') && !APPROVED_UNSAFE_RUST_FILES.has(relative)) {
            addFinding(critical, 'critical', filePath, lineNumber, 'unsafe Rust must stay inside reviewed process boundary modules', line);
          }
        }
        if (/extern\s+"(?:C|system)"\s*\{/.test(line)) {
          counters.foreignFunctionBlocks += 1;
          if (relative.startsWith('src/') && !APPROVED_UNSAFE_RUST_FILES.has(relative)) {
            addFinding(critical, 'critical', filePath, lineNumber, 'FFI declarations must stay inside reviewed process boundary modules', line);
          }
        }
      });

      if (lines.length > LARGE_MODULE_LINE_THRESHOLD && relative.startsWith('src/')) {
        counters.largeModules += 1;
        addFinding(warnings, 'warning', filePath, 1, `large Rust module has ${lines.length} lines; consider another split after behavior stabilizes`, relative);
      }
    } else {
      counters.nodeFiles += 1;
      counters.nodeLines += lines.length;
      lines.forEach((line, offset) => {
        const lineNumber = offset + 1;
        if (/process\.exit\s*\(\s*0\s*\)/.test(line) && relative.startsWith('scripts/')) {
          addFinding(warnings, 'warning', filePath, lineNumber, 'script exits explicitly with 0; prefer returning from main when practical', line);
        }
      });
    }
  }

  const architectureBoundaries = auditArchitectureBoundaries(critical);

  const report = {
    ok: critical.length === 0,
    generatedAt: new Date().toISOString(),
    policy: {
      critical: [
        'No production Rust todo! or unimplemented! macros.',
        'No production Rust panic! macros.',
        'Protocol primitives stay transport and command agnostic.',
        'Resource defaults stay side-effect free.',
        'Unsafe Rust and FFI declarations stay inside reviewed process boundary modules.',
      ],
      warnings: [
        `Rust modules over ${LARGE_MODULE_LINE_THRESHOLD} lines are tracked as refactor candidates.`,
        'Direct thread spawns outside reviewed runtime modules are tracked for future abstraction.',
        'Production unwrap counts are tracked as hardening backlog, not as an immediate blocker.',
        'Unsafe operations and foreign function blocks are counted and allowed only in reviewed process-detach modules.',
      ],
    },
    summary: counters,
    architecture: {
      boundaries: architectureBoundaries,
    },
    critical,
    warnings: sampleWarnings(warnings),
    warningCount: warnings.length,
  };

  return report;
}

function printHuman(report) {
  console.log(`source audit: ${report.ok ? 'ok' : 'critical findings present'}`);
  console.log(`files=${report.summary.files} rust=${report.summary.rustFiles} node=${report.summary.nodeFiles} critical=${report.critical.length} warnings=${report.warningCount}`);
  console.log(`largeModules=${report.summary.largeModules} directThreadSpawns=${report.summary.directThreadSpawns} commandSpawns=${report.summary.commandSpawns} productionUnwraps=${report.summary.productionUnwraps} unsafeOperations=${report.summary.unsafeOperations} foreignFunctionBlocks=${report.summary.foreignFunctionBlocks}`);
  if (report.critical.length > 0) {
    console.log('\ncritical findings:');
    for (const finding of report.critical) {
      console.log(`- ${finding.file}:${finding.line} ${finding.message}`);
    }
  }
  if (report.warnings.length > 0) {
    console.log('\nwarning samples:');
    for (const finding of report.warnings) {
      console.log(`- ${finding.file}:${finding.line} ${finding.message}`);
    }
  }
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    printHelp();
    return;
  }

  const report = audit(options);

  if (options.write) {
    const outputPath = path.resolve(repoRoot, options.write);
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);
  }

  if (options.json) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    printHuman(report);
  }

  if (options.failOnCritical && report.critical.length > 0) {
    process.exitCode = 1;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error.message || String(error));
    process.exitCode = 1;
  });
}

export { audit, parseArgs, walkCodeFiles };
