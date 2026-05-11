#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const COMMON_NPX_ENV_VARS = [
  'NPM_CONFIG_REGISTRY',
  'NPM_CONFIG_USERCONFIG',
  'NPM_CONFIG_GLOBALCONFIG',
  'NPM_CONFIG_CACHE',
  'NODE_EXTRA_CA_CERTS',
  'SSL_CERT_FILE',
  'REQUESTS_CA_BUNDLE',
  'HTTP_PROXY',
  'HTTPS_PROXY',
  'NO_PROXY',
  'http_proxy',
  'https_proxy',
  'no_proxy',
  'CI',
];

const SERVER_SPECIFIC_ENV_VARS = new Map([
  ['exa', ['EXA_API_KEY']],
  ['context7', ['CONTEXT7_API_KEY']],
  ['brave-search', ['BRAVE_API_KEY']],
  ['firecrawl', ['FIRECRAWL_API_KEY']],
  ['github', ['GITHUB_TOKEN', 'GITHUB_PERSONAL_ACCESS_TOKEN']],
  ['notion', ['NOTION_API_KEY']],
  ['sentry', ['SENTRY_AUTH_TOKEN']],
  ['postgres', ['DATABASE_URL', 'POSTGRES_URL', 'POSTGRES_CONNECTION_STRING']],
  ['screenpipe', ['SCREENPIPE_API_KEY']],
]);

const DANGEROUS_SOURCE_NAMES = new Set(['.claude', '.codex', '.omc', '%SystemDrive%']);

function parseArgs(argv) {
  const out = {
    root: process.cwd(),
    json: false,
    strict: false,
    write: null,
    markdown: null,
    bin: null,
    config: [],
    serverTimeoutMs: 30_000,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--json') out.json = true;
    else if (arg === '--strict') out.strict = true;
    else if (arg === '--root') out.root = path.resolve(argv[++i] ?? '.');
    else if (arg === '--bin') out.bin = path.resolve(argv[++i] ?? '');
    else if (arg === '--config') out.config.push(path.resolve(argv[++i] ?? ''));
    else if (arg === '--write') out.write = path.resolve(argv[++i] ?? '');
    else if (arg === '--markdown') out.markdown = path.resolve(argv[++i] ?? '');
    else if (arg === '--server-timeout-ms') out.serverTimeoutMs = Number(argv[++i] ?? out.serverTimeoutMs);
    else if (arg === '--help' || arg === '-h') {
      printHelp();
      process.exit(0);
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }
  return out;
}

function printHelp() {
  console.log(`Usage: node scripts/mcpace-full-doctor.mjs [options]\n\nOptions:\n  --root DIR             Project root to inspect. Defaults to cwd.\n  --bin FILE             mcpace binary to smoke-check. Defaults to PATH or target/release.\n  --config FILE          Extra mcp_settings JSON file to inspect. Repeatable.\n  --server-timeout-ms N  Expected upstream initialize timeout threshold. Default 30000.\n  --json                Print JSON.\n  --write FILE           Write JSON report.\n  --markdown FILE        Write markdown report.\n  --strict              Exit non-zero on warnings as well as failures.\n`);
}

function nowIso() {
  return new Date().toISOString();
}

function checkFactory() {
  const checks = [];
  return {
    checks,
    add(id, status, summary, detail = '', meta = {}) {
      checks.push({ id, status, summary, detail, meta });
    },
  };
}

function commandExists(cmd) {
  const pathExt = process.platform === 'win32' ? (process.env.PATHEXT || '.EXE;.CMD;.BAT;.COM').split(';') : [''];
  const pathParts = (process.env.PATH || '').split(path.delimiter).filter(Boolean);
  const candidates = [];
  if (path.isAbsolute(cmd) || cmd.includes(path.sep)) {
    candidates.push(cmd);
  } else {
    for (const part of pathParts) {
      for (const ext of pathExt) {
        candidates.push(path.join(part, cmd.endsWith(ext) ? cmd : `${cmd}${ext}`));
      }
    }
  }
  return candidates.find((candidate) => existsFile(candidate)) || null;
}

function existsFile(p) {
  try {
    return fs.statSync(p).isFile();
  } catch {
    return false;
  }
}

function existsDir(p) {
  try {
    return fs.statSync(p).isDirectory();
  } catch {
    return false;
  }
}

function isExecutable(p) {
  if (!existsFile(p)) return false;
  if (process.platform === 'win32') return /\.(exe|cmd|bat|com)$/i.test(p);
  try {
    fs.accessSync(p, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function run(cmd, args = [], opts = {}) {
  const result = spawnSync(cmd, args, {
    cwd: opts.cwd || process.cwd(),
    encoding: 'utf8',
    timeout: opts.timeout || 10_000,
    windowsHide: true,
    shell: false,
    env: opts.env || process.env,
  });
  return {
    status: result.status,
    signal: result.signal,
    error: result.error ? String(result.error.message || result.error) : null,
    stdout: (result.stdout || '').trim(),
    stderr: (result.stderr || '').trim(),
  };
}

function detectLibc() {
  if (process.platform !== 'linux') return { kind: 'not-linux' };
  const report = typeof process.report?.getReport === 'function' ? process.report.getReport() : null;
  const glibc = report?.header?.glibcVersionRuntime || report?.header?.glibcVersionCompiler;
  if (glibc) return { kind: 'glibc', version: glibc, source: 'process.report' };
  const ldd = run('ldd', ['--version'], { timeout: 3000 });
  const text = `${ldd.stdout}\n${ldd.stderr}`;
  const muslMatch = text.match(/musl/i);
  if (muslMatch) return { kind: 'musl', version: (text.match(/Version\s+([0-9.]+)/i) || [])[1] || null, source: 'ldd' };
  const glibcMatch = text.match(/(?:GLIBC|GNU libc|Debian GLIBC)\s*[^0-9]*([0-9]+\.[0-9]+)/i);
  if (glibcMatch) return { kind: 'glibc', version: glibcMatch[1], source: 'ldd' };
  return { kind: 'unknown', raw: text.slice(0, 500) };
}

function detectWsl() {
  if (process.platform !== 'linux') return false;
  try {
    return /microsoft|wsl/i.test(fs.readFileSync('/proc/version', 'utf8'));
  } catch {
    return false;
  }
}

function resolveMcpaceBinary(root, explicit) {
  if (explicit) return explicit;
  const suffix = process.platform === 'win32' ? '.exe' : '';
  const local = path.join(root, 'target', 'release', `mcpace${suffix}`);
  if (existsFile(local)) return local;
  return commandExists('mcpace');
}

function readJson(file) {
  const raw = fs.readFileSync(file, 'utf8');
  return JSON.parse(raw);
}

function findConfigFiles(root, extras) {
  const files = [];
  const maybe = (p) => {
    if (p && existsFile(p) && !files.includes(p)) files.push(p);
  };
  for (const extra of extras) maybe(extra);
  maybe(path.join(root, 'mcp_settings.json'));
  const rootD = path.join(root, 'mcp_settings.d');
  if (existsDir(rootD)) {
    for (const item of fs.readdirSync(rootD).sort()) {
      if (item.endsWith('.json')) maybe(path.join(rootD, item));
    }
  }
  const home = os.homedir();
  if (home) {
    maybe(path.join(home, '.mcpace', 'mcp_settings.json'));
    const homeD = path.join(home, '.mcpace', 'mcp_settings.d');
    if (existsDir(homeD)) {
      for (const item of fs.readdirSync(homeD).sort()) {
        if (item.endsWith('.json')) maybe(path.join(homeD, item));
      }
    }
  }
  return files;
}

function normalizedCommand(config) {
  return String(config.command || config.cmd || '').toLowerCase();
}

function joinedArgs(config) {
  const args = Array.isArray(config.args) ? config.args : [];
  return args.map(String).join(' ').toLowerCase();
}

function serverIsEnabled(config) {
  return config.enabled !== false && config.disabled !== true;
}

function envVars(config) {
  return Array.isArray(config.env_vars) ? config.env_vars.map(String) : [];
}

function missingEnvVars(config, names) {
  const have = new Set(envVars(config));
  return names.filter((name) => !have.has(name));
}

function inferSpecificEnv(serverName, config) {
  const key = serverName.toLowerCase();
  const haystack = `${key} ${normalizedCommand(config)} ${joinedArgs(config)}`;
  const out = new Set();
  for (const [needle, vars] of SERVER_SPECIFIC_ENV_VARS.entries()) {
    if (haystack.includes(needle)) {
      for (const v of vars) out.add(v);
    }
  }
  return [...out];
}

function maybeTimeout(config) {
  const candidates = [config.initTimeout, config.timeout, config.options?.timeout, config.options?.timeout_ms, config.timeoutMs];
  for (const item of candidates) {
    const n = Number(item);
    if (Number.isFinite(n) && n > 0) return n;
  }
  return null;
}

function isTempPath(value) {
  const s = String(value || '').toLowerCase().replace(/\\/g, '/');
  return s.includes('/tmp/') || s.includes('/temp/') || s.includes('/appdata/local/temp/') || s.endsWith('/tmp') || s.endsWith('/temp');
}

function flagValue(args, names) {
  const set = new Set(names);
  for (let i = 0; i < args.length - 1; i += 1) {
    if (set.has(String(args[i]))) return args[i + 1];
  }
  return null;
}

function hasRuntimePlaceholder(value) {
  return /\$\{[A-Z0-9_]+(?::-[^}]*)?\}/i.test(String(value || ''));
}

function auditConfigFile(file, state) {
  let data;
  try {
    data = readJson(file);
  } catch (err) {
    state.add(`config.parse:${file}`, 'fail', 'Config JSON cannot be parsed', String(err.message || err), { file });
    return;
  }
  const servers = data.mcpServers && typeof data.mcpServers === 'object' ? data.mcpServers : {};
  const names = Object.keys(servers);
  state.add(`config.file:${file}`, 'pass', `Config loaded: ${names.length} server(s)`, '', { file, serverCount: names.length });
  for (const name of names) {
    const config = servers[name] || {};
    const enabled = serverIsEnabled(config);
    const command = normalizedCommand(config);
    const args = joinedArgs(config);
    if (!enabled) {
      state.add(`server.disabled:${name}`, 'skip', `Server disabled: ${name}`, '', { file, name });
      continue;
    }
    if (!config.command) {
      state.add(`server.command:${name}`, 'warn', `Server has no command: ${name}`, 'Enabled stdio upstreams need an executable command.', { file, name });
    }
    if (/\bnpx(?:\.cmd)?\b/.test(command) || command.endsWith('/npx') || command.endsWith('\\npx.cmd')) {
      const missingCommon = missingEnvVars(config, COMMON_NPX_ENV_VARS);
      if (missingCommon.length > 0) {
        state.add(`server.npx-env:${name}`, 'warn', `npx server is missing env_vars passthrough: ${name}`, missingCommon.join(', '), { file, name, missing: missingCommon });
      } else {
        state.add(`server.npx-env:${name}`, 'pass', `npx env_vars look complete: ${name}`, '', { file, name });
      }
    }
    const specific = inferSpecificEnv(name, config);
    const missingSpecific = missingEnvVars(config, specific);
    if (specific.length > 0 && missingSpecific.length > 0) {
      state.add(`server.api-env:${name}`, 'warn', `Server may need API env_vars: ${name}`, missingSpecific.join(', '), { file, name, missing: missingSpecific });
    }
    if (name.toLowerCase().includes('serena') || args.includes('serena')) {
      const timeout = maybeTimeout(config);
      if (!timeout || timeout < 60_000) {
        state.add(`server.serena-timeout:${name}`, 'warn', 'Serena should use a longer initialize timeout', `Current timeout: ${timeout ?? 'not configured'}. Recommended: >= 120000 ms.`, { file, name, timeout });
      }
      const projectArgs = Array.isArray(config.args) ? config.args.map(String) : [];
      const projectRoot = flagValue(projectArgs, ['--project', '--project-root']) || config.projectRoot || config.cwd;
      if (!projectRoot) {
        state.add(`server.serena-project:${name}`, 'warn', 'Serena has no explicit project root/cwd', 'Use a real project path, not a temporary adapter-test path.', { file, name });
      } else if (hasRuntimePlaceholder(projectRoot)) {
        state.add(`server.serena-project:${name}`, 'pass', 'Serena project root uses a runtime placeholder', String(projectRoot), { file, name, projectRoot });
      } else if (isTempPath(projectRoot)) {
        state.add(`server.serena-project:${name}`, 'fail', 'Serena project root points at a temp directory', String(projectRoot), { file, name, projectRoot });
      } else if (!existsDir(path.resolve(String(projectRoot)))) {
        state.add(`server.serena-project:${name}`, 'warn', 'Serena project root was not found on this machine', String(projectRoot), { file, name, projectRoot });
      } else {
        state.add(`server.serena-project:${name}`, 'pass', 'Serena project root exists', String(projectRoot), { file, name, projectRoot });
      }
    }
    if ([config.cwd, ...(Array.isArray(config.args) ? config.args : [])].some(isTempPath)) {
      state.add(`server.temp-path:${name}`, 'warn', `Server references a temporary path: ${name}`, 'Temporary paths make smoke checks flaky and may disappear before initialize.', { file, name });
    }
  }
}

function inspectSourceTree(root, state) {
  if (!existsDir(root)) {
    state.add('source.root', 'fail', 'Root directory does not exist', root, { root });
    return;
  }
  state.add('source.root', 'pass', 'Root directory exists', root, { root });
  for (const item of DANGEROUS_SOURCE_NAMES) {
    const p = path.join(root, item);
    if (existsDir(p) || existsFile(p)) {
      state.add(`source.private-state:${item}`, 'fail', `Source tree contains local/private machine-state: ${item}`, p, { path: p });
    }
  }
  const screenshots = fs.readdirSync(root).filter((name) => /^screenshot.*\.(png|jpe?g|webp)$/i.test(name));
  if (screenshots.length > 0) {
    state.add('source.screenshots', 'warn', 'Root contains screenshot artifacts', screenshots.join(', '), { screenshots });
  }
  const envFile = path.join(root, '.env');
  if (existsFile(envFile)) {
    state.add('source.env-file', 'warn', 'Source tree contains .env', 'Do not include real secrets in source archives.', { path: envFile });
  }
}

function inspectPlatform(state) {
  const libc = detectLibc();
  state.add('platform.identity', 'pass', `Platform: ${process.platform}/${process.arch}`, '', {
    platform: process.platform,
    arch: process.arch,
    node: process.version,
    libc,
    wsl: detectWsl(),
  });
  if (process.platform === 'linux') {
    if (libc.kind === 'musl') {
      state.add('platform.linux-musl', 'warn', 'Linux musl/Alpine detected', 'Only claim Alpine support when linux-*-musl artifacts and install smoke pass.', { libc });
    } else if (libc.kind === 'glibc') {
      state.add('platform.linux-glibc', 'pass', `Linux glibc detected: ${libc.version || 'unknown version'}`, '', { libc });
    } else {
      state.add('platform.linux-libc', 'warn', 'Linux libc could not be identified', 'Verify glibc/musl target compatibility before publishing.', { libc });
    }
  }
}

function inspectTooling(state) {
  const tools = process.platform === 'win32' ? ['node', 'npm', 'npx', 'where'] : ['node', 'npm', 'npx', 'sh'];
  for (const tool of tools) {
    const found = commandExists(tool);
    state.add(`tool.${tool}`, found ? 'pass' : 'warn', found ? `${tool} found` : `${tool} not found in PATH`, found || '', { tool, path: found });
  }
  if (process.platform === 'linux') {
    const systemctl = commandExists('systemctl');
    if (!systemctl) {
      state.add('tool.systemctl-user', 'warn', 'systemctl not found', 'Autostart should degrade cleanly in containers/minimal distros.', {});
    } else {
      const result = run(systemctl, ['--user', 'show-environment'], { timeout: 5000 });
      const ok = result.status === 0;
      state.add('tool.systemctl-user', ok ? 'pass' : 'warn', ok ? 'systemd --user is available' : 'systemd --user is not available now', ok ? '' : `${result.stderr || result.stdout || result.error}`, { status: result.status });
    }
  }
}

function inspectMcpaceBinary(root, explicitBin, state) {
  const bin = resolveMcpaceBinary(root, explicitBin);
  if (!bin) {
    state.add('mcpace.binary', 'warn', 'mcpace binary not found', 'Pass --bin or build target/release/mcpace.', { root });
    return null;
  }
  if (!isExecutable(bin)) {
    state.add('mcpace.binary', 'fail', 'mcpace binary exists but is not executable', bin, { bin });
    return bin;
  }
  state.add('mcpace.binary', 'pass', 'mcpace binary found and executable', bin, { bin });
  const help = run(bin, ['--help'], { timeout: 10_000, cwd: root });
  state.add('mcpace.help', help.status === 0 ? 'pass' : 'fail', help.status === 0 ? 'mcpace --help works' : 'mcpace --help failed', help.stderr || help.stdout || help.error || '', { status: help.status });
  const version = run(bin, ['version'], { timeout: 10_000, cwd: root });
  state.add('mcpace.version', version.status === 0 ? 'pass' : 'warn', version.status === 0 ? 'mcpace version works' : 'mcpace version failed', version.stdout || version.stderr || version.error || '', { status: version.status });
  const doctor = run(bin, ['doctor', '--json', '--root', root], { timeout: 15_000, cwd: root });
  state.add('mcpace.doctor', doctor.status === 0 ? 'pass' : 'warn', doctor.status === 0 ? 'mcpace doctor works' : 'mcpace doctor failed or unavailable', (doctor.stderr || doctor.stdout || doctor.error || '').slice(0, 4000), { status: doctor.status });
  return bin;
}

function renderMarkdown(report) {
  const counts = report.summary.counts;
  const lines = [];
  lines.push(`# MCPace full doctor report`);
  lines.push('');
  lines.push(`Generated: ${report.generatedAt}`);
  lines.push(`Root: \`${report.root}\``);
  lines.push('');
  lines.push(`Status: **${report.summary.status.toUpperCase()}**`);
  lines.push('');
  lines.push(`Checks: pass ${counts.pass || 0}, warn ${counts.warn || 0}, fail ${counts.fail || 0}, skip ${counts.skip || 0}.`);
  lines.push('');
  for (const check of report.checks) {
    const icon = check.status === 'pass' ? '✅' : check.status === 'warn' ? '⚠️' : check.status === 'skip' ? '⏭️' : '❌';
    lines.push(`## ${icon} ${check.id}`);
    lines.push('');
    lines.push(`**${check.summary}**`);
    if (check.detail) {
      lines.push('');
      lines.push('```text');
      lines.push(String(check.detail).slice(0, 6000));
      lines.push('```');
    }
    lines.push('');
  }
  return `${lines.join('\n')}\n`;
}

function summarize(checks, strict) {
  const counts = {};
  for (const check of checks) counts[check.status] = (counts[check.status] || 0) + 1;
  const status = counts.fail > 0 ? 'fail' : strict && counts.warn > 0 ? 'fail' : counts.warn > 0 ? 'warn' : 'pass';
  return { status, counts };
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const state = checkFactory();
  inspectPlatform(state);
  inspectTooling(state);
  inspectSourceTree(args.root, state);
  inspectMcpaceBinary(args.root, args.bin, state);
  const configFiles = findConfigFiles(args.root, args.config);
  if (configFiles.length === 0) {
    state.add('config.discovery', 'warn', 'No MCP settings JSON files found', 'Expected mcp_settings.json or mcp_settings.d/*.json in root or ~/.mcpace.', { root: args.root });
  } else {
    state.add('config.discovery', 'pass', `Found ${configFiles.length} MCP settings file(s)`, configFiles.join('\n'), { files: configFiles });
    for (const file of configFiles) auditConfigFile(file, state);
  }
  const report = {
    generatedAt: nowIso(),
    tool: path.basename(fileURLToPath(import.meta.url)),
    root: args.root,
    summary: summarize(state.checks, args.strict),
    checks: state.checks,
  };
  if (args.write) {
    fs.mkdirSync(path.dirname(args.write), { recursive: true });
    fs.writeFileSync(args.write, `${JSON.stringify(report, null, 2)}\n`);
  }
  if (args.markdown) {
    fs.mkdirSync(path.dirname(args.markdown), { recursive: true });
    fs.writeFileSync(args.markdown, renderMarkdown(report));
  }
  if (args.json) console.log(JSON.stringify(report, null, 2));
  else console.log(renderMarkdown(report));
  process.exitCode = report.summary.status === 'fail' ? 1 : 0;
}

try {
  main();
} catch (err) {
  console.error(err?.stack || err?.message || String(err));
  process.exit(2);
}
