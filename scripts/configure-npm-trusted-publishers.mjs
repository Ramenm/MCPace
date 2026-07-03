#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import process from 'node:process';
import { readCliPackageJson, repoRoot } from './lib/project-metadata.mjs';

const DEFAULT_REPOSITORY = 'Ramenm/MCPace';
const DEFAULT_WORKFLOW_FILE = 'publish-npm.yml';
const DEFAULT_ENVIRONMENT = 'npm-publish';
const DEFAULT_NPM_PACKAGE = 'npm@11.18.0';
const DEFAULT_SLEEP_MS = 2_000;

function usage() {
  return `Usage: node scripts/configure-npm-trusted-publishers.mjs [--execute] [--json]

Bulk-configure npm trusted publishers for @mcpace/cli and its native optional packages.

Default mode is a safe plan-only dry run.

Options:
  --execute                 Run npm trust github for each package.
  --json                    Print a JSON plan/report.
  --repo <owner/repo>       GitHub repository. Default: ${DEFAULT_REPOSITORY}
  --workflow <filename>     Workflow file under .github/workflows. Default: ${DEFAULT_WORKFLOW_FILE}
  --environment <name>      GitHub environment name. Default: ${DEFAULT_ENVIRONMENT}
  --npm-package <spec>      npm CLI package used through npm exec. Default: ${DEFAULT_NPM_PACKAGE}
  --sleep-ms <number>       Delay between execute calls. Default: ${DEFAULT_SLEEP_MS}
  --package <name>          Limit to one package; repeatable.
  --help                    Show this help.

Before --execute, run:
  npm login --auth-type=web

npm trust requires an authenticated npm owner account with 2FA enabled.
`;
}

function parseArgs(argv) {
  const args = {
    execute: false,
    json: false,
    repository: DEFAULT_REPOSITORY,
    workflowFile: DEFAULT_WORKFLOW_FILE,
    environment: DEFAULT_ENVIRONMENT,
    npmPackage: DEFAULT_NPM_PACKAGE,
    sleepMs: DEFAULT_SLEEP_MS,
    packageFilters: [],
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--execute') args.execute = true;
    else if (arg === '--json') args.json = true;
    else if (arg === '--repo' || arg === '--repository') args.repository = requireValue(argv, ++index, arg);
    else if (arg === '--workflow' || arg === '--file') args.workflowFile = requireValue(argv, ++index, arg);
    else if (arg === '--environment' || arg === '--env') args.environment = requireValue(argv, ++index, arg);
    else if (arg === '--npm-package') args.npmPackage = requireValue(argv, ++index, arg);
    else if (arg === '--sleep-ms') args.sleepMs = Number.parseInt(requireValue(argv, ++index, arg), 10);
    else if (arg === '--package') args.packageFilters.push(requireValue(argv, ++index, arg));
    else if (arg === '--help' || arg === '-h') {
      console.log(usage());
      process.exit(0);
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }

  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(args.repository)) {
    throw new Error(`--repo must be owner/repo, got '${args.repository}'`);
  }
  if (!/^[A-Za-z0-9_.-]+\.ya?ml$/.test(args.workflowFile)) {
    throw new Error(`--workflow must be a .yml/.yaml filename, got '${args.workflowFile}'`);
  }
  if (args.environment && !/^[A-Za-z0-9_.-]+$/.test(args.environment)) {
    throw new Error(`--environment contains unsupported characters: '${args.environment}'`);
  }
  if (!Number.isFinite(args.sleepMs) || args.sleepMs < 0) {
    throw new Error(`--sleep-ms must be a non-negative number, got '${args.sleepMs}'`);
  }
  return args;
}

function requireValue(argv, index, flag) {
  const value = argv[index];
  if (!value || value.startsWith('--')) throw new Error(`${flag} requires a value`);
  return value;
}

function packageNames(cliPackage, packageFilters) {
  const all = [cliPackage.name, ...Object.keys(cliPackage.optionalDependencies ?? {})]
    .filter(Boolean);
  const names = [...new Set(all)];
  if (packageFilters.length === 0) return names;
  const allowed = new Set(packageFilters);
  const missing = [...allowed].filter((name) => !names.includes(name));
  if (missing.length > 0) throw new Error(`unknown package filter(s): ${missing.join(', ')}`);
  return names.filter((name) => allowed.has(name));
}

function trustArgs(pkg, args) {
  const commandArgs = [
    'exec',
    '--yes',
    `--package=${args.npmPackage}`,
    '--',
    'npm',
    'trust',
    'github',
    pkg,
    '--repo',
    args.repository,
    '--file',
    args.workflowFile,
    '--allow-publish',
    '--yes',
  ];
  if (args.environment) commandArgs.push('--env', args.environment);
  return commandArgs;
}

function npmInvocation(commandArgs) {
  if (process.platform === 'win32') {
    return {
      command: process.env.ComSpec || 'cmd.exe',
      args: ['/d', '/s', '/c', 'npm', ...commandArgs],
    };
  }
  return { command: 'npm', args: commandArgs };
}

function commandLine(invocation) {
  const offset = process.platform === 'win32' ? 4 : 0;
  return ['npm', ...invocation.args.slice(offset)]
    .map((part) => (/[\s"]/u.test(part) ? JSON.stringify(part) : part))
    .join(' ');
}

function sleep(ms) {
  if (ms <= 0) return;
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function runNpm(commandArgs, options = {}) {
  const invocation = npmInvocation(commandArgs);
  return spawnSync(invocation.command, invocation.args, {
    cwd: repoRoot,
    encoding: options.encoding ?? 'utf8',
    env: process.env,
    maxBuffer: 16 * 1024 * 1024,
    stdio: options.stdio ?? ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  });
}

function verifyLoggedIn(args) {
  const result = runNpm(['exec', '--yes', `--package=${args.npmPackage}`, '--', 'npm', 'whoami'], {
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  if (result.status === 0) {
    return { ok: true, user: String(result.stdout || '').trim() };
  }
  return {
    ok: false,
    detail: String(result.stderr || result.stdout || result.error?.message || '').trim(),
  };
}

function createPlan(args) {
  const cliPackage = readCliPackageJson();
  const packages = packageNames(cliPackage, args.packageFilters);
  return {
    schema: 'mcpace.npmTrustedPublishersBulkPlan.v1',
    execute: args.execute,
    repository: args.repository,
    workflowFile: args.workflowFile,
    environment: args.environment,
    npmPackage: args.npmPackage,
    sleepMs: args.sleepMs,
    packages: packages.map((pkg) => {
      const invocation = npmInvocation(trustArgs(pkg, args));
      return {
        name: pkg,
        command: commandLine(invocation),
      };
    }),
  };
}

function printHumanPlan(plan) {
  console.log(`npm trusted publisher bulk plan (${plan.packages.length} packages)`);
  console.log(`  repository:  ${plan.repository}`);
  console.log(`  workflow:    ${plan.workflowFile}`);
  console.log(`  environment: ${plan.environment || 'none'}`);
  console.log(`  npm cli:     ${plan.npmPackage}`);
  console.log('');
  for (const pkg of plan.packages) console.log(`  ${pkg.command}`);
  console.log('');
  if (plan.execute) console.log('Execute mode requested; npm login will be checked before any trust changes are attempted.');
  else console.log('This was a plan only. To apply it after npm login, rerun with --execute.');
}

function executePlan(plan, args) {
  const login = verifyLoggedIn(args);
  if (!login.ok) {
    console.error('npm is not logged in for trusted publisher configuration.');
    console.error('Run this once, complete the browser/2FA flow, then rerun this script:');
    console.error('  npm login --auth-type=web');
    if (login.detail) console.error(`\nwhoami failure:\n${login.detail}`);
    process.exit(1);
  }

  console.log(`Authenticated to npm as ${login.user || '<unknown>'}.`);
  console.log('The first npm trust call may require 2FA; choose the website option to skip 2FA for the next 5 minutes to let the bulk run finish.');
  const results = [];
  for (const [index, pkg] of plan.packages.entries()) {
    console.log(`\n[${index + 1}/${plan.packages.length}] ${pkg.name}`);
    console.log(pkg.command);
    const result = runNpm(trustArgs(pkg.name, args), { stdio: 'inherit', encoding: 'utf8' });
    results.push({ package: pkg.name, status: result.status ?? 1 });
    if (index < plan.packages.length - 1) sleep(args.sleepMs);
  }

  const failures = results.filter((entry) => entry.status !== 0);
  if (failures.length > 0) {
    console.error(`\nFailed to configure ${failures.length}/${results.length} package(s): ${failures.map((entry) => entry.package).join(', ')}`);
    process.exit(1);
  }
  console.log(`\nConfigured npm trusted publishers for ${results.length} package(s).`);
}

try {
  const args = parseArgs(process.argv.slice(2));
  const plan = createPlan(args);
  if (args.json) process.stdout.write(`${JSON.stringify(plan, null, 2)}\n`);
  else printHumanPlan(plan);
  if (args.execute) executePlan(plan, args);
} catch (error) {
  console.error(error?.stack ?? String(error));
  process.exitCode = 1;
}
