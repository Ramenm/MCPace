#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';

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

const SPECIFIC = [
  [/exa/i, ['EXA_API_KEY']],
  [/context7/i, ['CONTEXT7_API_KEY']],
  [/brave[-_ ]?search/i, ['BRAVE_API_KEY']],
  [/firecrawl/i, ['FIRECRAWL_API_KEY']],
  [/github/i, ['GITHUB_TOKEN', 'GITHUB_PERSONAL_ACCESS_TOKEN']],
  [/notion/i, ['NOTION_API_KEY']],
  [/sentry/i, ['SENTRY_AUTH_TOKEN']],
  [/postgres|postgresql|pg\b/i, ['DATABASE_URL', 'POSTGRES_URL', 'POSTGRES_CONNECTION_STRING']],
  [/screenpipe/i, ['SCREENPIPE_API_KEY']],
];

function parseArgs(argv) {
  const args = { config: null, apply: false, json: false, backup: true };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--apply') args.apply = true;
    else if (arg === '--json') args.json = true;
    else if (arg === '--no-backup') args.backup = false;
    else if (arg === '--config') args.config = argv[++i];
    else if (arg === '--help' || arg === '-h') {
      console.log(`Usage: node scripts/mcpace-config-hardener.mjs --config FILE [--apply] [--json]\n\nAdds env_vars names for npx/uvx upstreams and safer Serena timeout/project defaults.\nIt never writes secret values, only variable names.`);
      process.exit(0);
    } else if (!args.config) args.config = arg;
    else throw new Error(`Unknown argument: ${arg}`);
  }
  if (!args.config) {
    args.config = path.join(os.homedir(), '.mcpace', 'mcp_settings.d', 'restored-from-mcpace-history-72d64b0.json');
  }
  args.config = path.resolve(args.config);
  return args;
}

function addMany(current, values) {
  const set = new Set(Array.isArray(current) ? current.map(String) : []);
  const added = [];
  for (const value of values) {
    if (!set.has(value)) {
      set.add(value);
      added.push(value);
    }
  }
  return { next: [...set], added };
}

function commandText(name, server) {
  const args = Array.isArray(server.args) ? server.args.join(' ') : '';
  return `${name} ${server.command || ''} ${args}`;
}

function isNpxServer(server) {
  return /(?:^|[\\/])npx(?:\.cmd)?$/i.test(String(server.command || '')) || /\bnpx(?:\.cmd)?\b/i.test(String(server.command || ''));
}

function isUvxServer(server) {
  return /(?:^|[\\/])uvx(?:\.exe)?$/i.test(String(server.command || '')) || /\buvx(?:\.exe)?\b/i.test(String(server.command || ''));
}

function serverEnabled(server) {
  return server && server.enabled !== false && server.disabled !== true;
}

function timeoutValue(server) {
  for (const item of [server.initTimeout, server.timeout, server.options?.timeout, server.timeoutMs]) {
    const n = Number(item);
    if (Number.isFinite(n) && n > 0) return n;
  }
  return null;
}

function hardenServer(name, server) {
  const changes = [];
  if (!server || typeof server !== 'object') return changes;
  if (!serverEnabled(server)) return changes;
  const text = commandText(name, server);
  if (isNpxServer(server)) {
    const merged = addMany(server.env_vars, COMMON_NPX_ENV_VARS);
    if (merged.added.length > 0) {
      server.env_vars = merged.next;
      changes.push({ type: 'env_vars', server: name, added: merged.added });
    }
  }
  if (isUvxServer(server) || /serena/i.test(text)) {
    const merged = addMany(server.env_vars, [
      'UV_INDEX_URL',
      'UV_EXTRA_INDEX_URL',
      'PIP_INDEX_URL',
      'PIP_EXTRA_INDEX_URL',
      'REQUESTS_CA_BUNDLE',
      'SSL_CERT_FILE',
      'HTTP_PROXY',
      'HTTPS_PROXY',
      'NO_PROXY',
      'http_proxy',
      'https_proxy',
      'no_proxy',
      'CI',
    ]);
    if (merged.added.length > 0) {
      server.env_vars = merged.next;
      changes.push({ type: 'env_vars', server: name, added: merged.added });
    }
  }
  for (const [pattern, vars] of SPECIFIC) {
    if (pattern.test(text)) {
      const merged = addMany(server.env_vars, vars);
      if (merged.added.length > 0) {
        server.env_vars = merged.next;
        changes.push({ type: 'env_vars', server: name, added: merged.added });
      }
    }
  }
  if (/serena/i.test(text)) {
    if (!server.options || typeof server.options !== 'object' || Array.isArray(server.options)) server.options = {};
    const current = timeoutValue(server);
    if (!current || current < 120_000) {
      server.options.timeout = 120_000;
      server.initTimeout = 120_000;
      changes.push({ type: 'timeout', server: name, from: current, to: 120_000 });
    }
  }
  return changes;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const raw = fs.readFileSync(args.config, 'utf8');
  const data = JSON.parse(raw);
  if (!data.mcpServers || typeof data.mcpServers !== 'object') data.mcpServers = {};
  const changes = [];
  for (const [name, server] of Object.entries(data.mcpServers)) {
    changes.push(...hardenServer(name, server));
  }
  const result = { config: args.config, apply: args.apply, changed: changes.length > 0, changes };
  if (args.apply && changes.length > 0) {
    if (args.backup) {
      const backup = `${args.config}.bak-${new Date().toISOString().replace(/[:.]/g, '-')}`;
      fs.copyFileSync(args.config, backup);
      result.backup = backup;
    }
    fs.writeFileSync(args.config, `${JSON.stringify(data, null, 2)}\n`);
  }
  if (args.json) console.log(JSON.stringify(result, null, 2));
  else {
    console.log(args.apply ? 'Applied MCPace config hardening.' : 'Dry run only. Add --apply to write changes.');
    console.log(JSON.stringify(result, null, 2));
  }
}

try {
  main();
} catch (err) {
  console.error(err?.stack || err?.message || String(err));
  process.exit(2);
}
