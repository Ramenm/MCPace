#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import { repoRoot, deriveProjectVersion } from './lib/project-metadata.mjs';

const DEFAULT_FIXTURE = 'eval/fixtures/registry-sample.json';
const DEFAULT_JSON_REPORT = 'reports/registry-lab-latest.json';
const DEFAULT_MARKDOWN_REPORT = 'reports/registry-lab-latest.md';
const REGISTRY_BASE_URL = 'https://registry.modelcontextprotocol.io';
const DEFAULT_TIMEOUT_MS = 10_000;

function parseArgs(argv) {
  const args = {
    json: false,
    input: DEFAULT_FIXTURE,
    limit: 50,
    live: false,
    write: DEFAULT_JSON_REPORT,
    markdown: DEFAULT_MARKDOWN_REPORT,
    timeoutMs: DEFAULT_TIMEOUT_MS,
    noWrite: false,
    help: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    const readValue = () => {
      const value = argv[index + 1];
      if (!value || value.startsWith('--')) throw new Error(`${token} requires a value`);
      index += 1;
      return value;
    };
    switch (token) {
      case '--json': args.json = true; break;
      case '--input': args.input = readValue(); break;
      case '--limit': args.limit = parsePositiveInteger(readValue(), token); break;
      case '--live': args.live = true; break;
      case '--write': args.write = readValue(); break;
      case '--markdown': args.markdown = readValue(); break;
      case '--timeout-ms': args.timeoutMs = parsePositiveInteger(readValue(), token); break;
      case '--no-write': args.write = null; args.markdown = null; args.noWrite = true; break;
      case '--help':
      case '-h': args.help = true; break;
      default: throw new Error(`unsupported registry-lab argument: ${token}`);
    }
  }
  return args;
}

function parsePositiveInteger(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) throw new Error(`${label} must be a positive integer`);
  return parsed;
}

function printHelp() {
  console.log(`Usage: node scripts/registry-lab.mjs [options]\n\nBuilds a metadata-only MCP Registry classification and policy-review report.\nDefault mode reads ${DEFAULT_FIXTURE}; it does not install, launch, or call arbitrary third-party MCP servers.\n\nOptions:\n  --input <path>       Read a registry-like JSON fixture.\n  --live               Fetch one metadata page from the public MCP Registry REST API.\n  --limit <n>          Fixture/live server limit. Default 50.\n  --write <path>       JSON report path.\n  --markdown <path>    Markdown report path.\n  --timeout-ms <ms>    Live fetch timeout. Default ${DEFAULT_TIMEOUT_MS}.\n  --no-write           Do not write reports.\n  --json               Print JSON report.\n`);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printHelp();
    return;
  }
  const started = performance.now();
  const source = await loadRegistryInput(args);
  const servers = normalizeServers(source.payload).slice(0, args.limit);
  const classifications = servers.map(classifyServer);
  const status = source.status === 'blocked' ? 'blocked' : 'pass';
  const report = {
    schema: 'mcpace.registryLab.v2',
    version: deriveProjectVersion(),
    generatedAt: new Date().toISOString(),
    status,
    mode: args.live ? 'live-registry-metadata-only' : 'fixture-metadata-only',
    source: source.source,
    elapsedMs: Math.round(performance.now() - started),
    safety: {
      executesThirdPartyPackages: false,
      contactsRegistryOnlyInLiveMode: Boolean(args.live),
      sandboxLaunchImplemented: false,
      destructiveToolCallsAllowed: false,
      defaultUnknownPolicy: 'review-required + single-writer + disabled-until-user-confirms',
    },
    summary: summarize(classifications),
    classifications,
    notes: [
      'This lab is intentionally metadata-only. It never runs npx, uvx, docker, or arbitrary stdio commands.',
      'Registry metadata is discovery input, not trust proof. Unknown servers stay conservative until policy review.',
      'Sandbox probing and concurrency torture are planned follow-up lanes and must run without user secrets by default.',
      ...(source.notes || []),
    ],
  };
  writeOutputs(report, args);
  if (args.json) console.log(JSON.stringify(report, null, 2));
}

async function loadRegistryInput(args) {
  if (!args.live) {
    const sourcePath = path.resolve(repoRoot, args.input);
    return {
      status: 'pass',
      source: { type: 'fixture', path: path.relative(repoRoot, sourcePath) },
      payload: JSON.parse(fs.readFileSync(sourcePath, 'utf8')),
      notes: [],
    };
  }

  const url = `${REGISTRY_BASE_URL}/v0.1/servers?limit=${encodeURIComponent(args.limit)}`;
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), args.timeoutMs);
  try {
    const response = await fetch(url, {
      signal: controller.signal,
      headers: {
        accept: 'application/json',
        'user-agent': 'mcpace-registry-lab/1.0',
      },
    });
    if (!response.ok) throw new Error(`registry returned ${response.status}`);
    return {
      status: 'pass',
      source: { type: 'live-registry', url },
      payload: await response.json(),
      notes: [],
    };
  } catch (error) {
    return {
      status: 'blocked',
      source: { type: 'live-registry', url },
      payload: { servers: [] },
      notes: [`Live registry metadata fetch was blocked or failed in this host: ${error.message}`],
    };
  } finally {
    clearTimeout(timeout);
  }
}

function normalizeServers(payload) {
  if (Array.isArray(payload)) return payload;
  if (Array.isArray(payload?.servers)) return payload.servers;
  return [];
}

function classifyServer(server) {
  const name = String(server.name || server.id || '').trim();
  const title = String(server.title || '').trim();
  const description = String(server.description || '').trim();
  const packages = Array.isArray(server.packages) ? server.packages : [];
  const haystack = [name, title, description, ...packages.flatMap((pkg) => [pkg.identifier, pkg.registryType, pkg.transport?.type])]
    .filter(Boolean)
    .join(' ')
    .toLowerCase();
  const transportTypes = Array.from(new Set(packages.map((pkg) => pkg.transport?.type || pkg.transportType || '').filter(Boolean)));
  const packageKinds = Array.from(new Set(packages.map((pkg) => pkg.registryType || pkg.registry || '').filter(Boolean)));
  const riskSignals = detectRiskSignals(haystack);
  const policy = suggestPolicy(haystack, riskSignals, transportTypes);
  const reviewRequired = true;
  return {
    name,
    title,
    status: server.status || 'unknown',
    transportTypes,
    packageKinds,
    packageCount: packages.length,
    riskSignals,
    suggestedPolicy: policy,
    reviewRequired,
    confidence: policy.confidence,
    decision: policy.decision,
  };
}

function detectRiskSignals(haystack) {
  const signals = [];
  const addIf = (id, pattern) => { if (pattern.test(haystack)) signals.push(id); };
  addIf('filesystem', /filesystem|file[-_ ]?system|files?\b|path|directory|folder/);
  addIf('git-repository', /\bgit\b|repository|repo|worktree|commit|branch/);
  addIf('browser-or-desktop', /browser|playwright|puppeteer|selenium|desktop|window|clipboard|click|page/);
  addIf('memory-or-context', /memory|graph|stateful|context[-_ ]?store|sequential[-_ ]?thinking|conversation/);
  addIf('database', /sqlite|postgres|mysql|database|db\b|sql\b/);
  addIf('shell-or-process', /shell|terminal|exec|process|command|bash|powershell|docker|container|code[-_ ]?runner/);
  addIf('network-open-world', /http|fetch|web|search|api|remote|slack|jira|docs|documentation|github[-_ ]?(api|server|mcp)|maps|tavily/);
  addIf('cloud-admin', /azure|aws|gcp|cloudflare|terraform|pulumi|subscription|tenant|resource[-_ ]?group|iam/);
  addIf('blockchain-wallet', /evm|ethereum|web3|wallet|blockchain|smart contract|transaction|token transfer|private key/);
  addIf('cluster-control', /kubernetes|k8s|kubectl|openshift|cluster|namespace|pod|helm/);
  addIf('openapi-bridge', /openapi|swagger|api[-_ ]?bridge|api[-_ ]?gateway/);
  addIf('secrets-manager', /secret manager|secrets manager|vault|1password|bitwarden|password manager|keychain|credential store/);
  addIf('payments-financial', /stripe|paypal|payment|billing|invoice|bank|treasury|card|refund|charge|payout/);
  addIf('identity-admin', /okta|auth0|entra|active directory|scim|sso|identity|user management|group management/);
  addIf('messaging-email', /gmail|outlook|email|imap|smtp|mailbox|send mail|slack|teams|discord|sms/);
  addIf('prompt-injection-surface', /ignore (all )?(previous|prior) instructions|system prompt|developer message|exfiltrate|leak secret|prompt injection/);
  addIf('secrets-or-credentials', /token|secret|credential|oauth|api[-_ ]?key|login|auth|wallet|private key/);
  if (!signals.length) signals.push('unknown-side-effects');
  return signals;
}

function suggestPolicy(haystack, signals, transportTypes) {
  const transport = transportTypes.includes('streamable-http') ? 'streamable-http' : transportTypes[0] || 'stdio';
  if (signals.includes('browser-or-desktop')) {
    return {
      decision: 'shared-exclusive-host-lock',
      scopeClass: 'shared-exclusive',
      concurrencyPolicy: 'single-session',
      stateBinding: 'host-desktop',
      credentialBinding: 'browser-profile',
      parallelismLimit: 1,
      projectRootMode: 'optional',
      stateProfileMode: 'required',
      hostLock: 'browser-profile',
      routingGroup: 'browser',
      discoveryRequiresLease: true,
      confidence: 'medium',
      reason: 'browser/desktop servers usually share a profile, visible window, or process state',
    };
  }
  if (/context7|documentation|docs|library/.test(haystack) && signals.includes('network-open-world')) {
    return {
      decision: 'network-docs-multi-reader-review',
      scopeClass: 'credential-scoped',
      concurrencyPolicy: 'multi-reader',
      stateBinding: 'none',
      credentialBinding: signals.includes('secrets-or-credentials') ? 'api-credential' : 'remote-origin-or-none',
      parallelismLimit: 4,
      projectRootMode: 'none',
      stateProfileMode: 'none',
      hostLock: 'none',
      routingGroup: 'network-docs',
      discoveryRequiresLease: false,
      confidence: 'medium',
      reason: 'documentation fetchers can be read-mostly candidates after annotation and network review',
    };
  }
  if (signals.includes('identity-admin')) {
    return {
      decision: 'identity-admin-credential-review',
      scopeClass: 'credential-scoped',
      concurrencyPolicy: 'single-writer',
      stateBinding: 'identity-tenant',
      credentialBinding: 'identity-admin-credential',
      parallelismLimit: 1,
      projectRootMode: 'optional',
      stateProfileMode: 'required',
      hostLock: 'identity-tenant',
      routingGroup: 'identity-admin',
      discoveryRequiresLease: true,
      defaultEnabled: false,
      confidence: 'high',
      reason: 'identity administration servers can change users/groups/access and must be disabled until tenant and role scope are reviewed',
    };
  }
  if (signals.includes('cloud-admin')) {
    return {
      decision: 'cloud-admin-credential-review',
      scopeClass: 'credential-scoped',
      concurrencyPolicy: 'single-writer',
      stateBinding: 'cloud-account-or-subscription',
      credentialBinding: 'cloud-credential',
      parallelismLimit: 1,
      projectRootMode: 'optional',
      stateProfileMode: 'required',
      hostLock: 'cloud-account',
      routingGroup: 'cloud-admin',
      discoveryRequiresLease: true,
      defaultEnabled: false,
      confidence: 'high',
      reason: 'cloud administration servers can mutate external infrastructure and must be disabled until account, tenant, and credential scope are reviewed',
    };
  }
  if (signals.includes('blockchain-wallet')) {
    return {
      decision: 'blockchain-wallet-review',
      scopeClass: 'credential-scoped',
      concurrencyPolicy: 'single-writer',
      stateBinding: 'wallet-and-chain',
      credentialBinding: 'wallet-or-rpc-credential',
      parallelismLimit: 1,
      projectRootMode: 'optional',
      stateProfileMode: 'required',
      hostLock: 'wallet',
      routingGroup: 'blockchain-wallet',
      discoveryRequiresLease: true,
      defaultEnabled: false,
      confidence: 'high',
      reason: 'wallet/blockchain servers can sign or submit irreversible transactions and must be disabled until wallet, chain, and operation scopes are reviewed',
    };
  }
  if (signals.includes('cluster-control')) {
    return {
      decision: 'cluster-admin-credential-review',
      scopeClass: 'credential-scoped',
      concurrencyPolicy: 'single-writer',
      stateBinding: 'cluster-context',
      credentialBinding: 'kubeconfig-or-cloud-credential',
      parallelismLimit: 1,
      projectRootMode: 'optional',
      stateProfileMode: 'required',
      hostLock: 'cluster-context',
      routingGroup: 'cluster-control',
      discoveryRequiresLease: true,
      defaultEnabled: false,
      confidence: 'high',
      reason: 'cluster-control servers can mutate remote infrastructure and must be disabled until kube-context and permissions are reviewed',
    };
  }
  if (signals.includes('openapi-bridge')) {
    return {
      decision: 'network-openapi-review',
      scopeClass: 'credential-scoped',
      concurrencyPolicy: 'single-writer',
      stateBinding: 'remote-api-spec',
      credentialBinding: signals.includes('secrets-or-credentials') ? 'api-credential' : 'remote-origin-or-none',
      parallelismLimit: 1,
      projectRootMode: 'optional',
      stateProfileMode: 'optional',
      hostLock: 'none',
      routingGroup: 'openapi-bridge',
      discoveryRequiresLease: true,
      defaultEnabled: false,
      confidence: 'low',
      reason: 'OpenAPI bridges can expose arbitrary remote operations and need operation-level review before enablement',
    };
  }
  if (signals.includes('secrets-manager')) {
    return {
      decision: 'secrets-manager-disabled-review',
      scopeClass: 'credential-scoped',
      concurrencyPolicy: 'single-writer',
      stateBinding: 'secret-store',
      credentialBinding: 'secret-manager-credential',
      parallelismLimit: 1,
      projectRootMode: 'optional',
      stateProfileMode: 'required',
      hostLock: 'secret-store',
      routingGroup: 'secrets-manager',
      discoveryRequiresLease: true,
      defaultEnabled: false,
      confidence: 'high',
      reason: 'secrets managers can expose or rotate credentials and must stay disabled until vault scope and masking rules are reviewed',
    };
  }
  if (signals.includes('payments-financial')) {
    return {
      decision: 'payments-financial-review',
      scopeClass: 'credential-scoped',
      concurrencyPolicy: 'single-writer',
      stateBinding: 'merchant-or-bank-account',
      credentialBinding: 'financial-api-credential',
      parallelismLimit: 1,
      projectRootMode: 'optional',
      stateProfileMode: 'required',
      hostLock: 'financial-account',
      routingGroup: 'payments-financial',
      discoveryRequiresLease: true,
      defaultEnabled: false,
      confidence: 'high',
      reason: 'payment/financial servers can move money or mutate billing state and require explicit account and operation review',
    };
  }
  if (signals.includes('messaging-email')) {
    return {
      decision: 'messaging-external-review',
      scopeClass: 'credential-scoped',
      concurrencyPolicy: 'single-writer',
      stateBinding: 'mailbox-or-workspace',
      credentialBinding: 'messaging-api-credential',
      parallelismLimit: 1,
      projectRootMode: 'optional',
      stateProfileMode: 'required',
      hostLock: 'mailbox-or-workspace',
      routingGroup: 'messaging-email',
      discoveryRequiresLease: true,
      defaultEnabled: false,
      confidence: 'medium',
      reason: 'messaging/email servers can send messages or expose private inbox/workspace data and require explicit owner review',
    };
  }
  if (signals.includes('shell-or-process')) {
    return {
      decision: 'disabled-dangerous-command-runner',
      scopeClass: 'host-global',
      concurrencyPolicy: 'single-writer',
      stateBinding: 'host-process',
      credentialBinding: 'host-env',
      parallelismLimit: 1,
      projectRootMode: 'required',
      stateProfileMode: 'required',
      hostLock: 'host-process-table',
      routingGroup: 'dangerous-command-runner',
      discoveryRequiresLease: true,
      defaultEnabled: false,
      confidence: 'high',
      reason: 'command runners have broad host blast radius and require explicit user review',
    };
  }
  if (signals.includes('git-repository')) {
    return {
      decision: 'project-repo-single-writer',
      scopeClass: 'project-local',
      concurrencyPolicy: 'single-writer',
      stateBinding: 'repository',
      credentialBinding: 'git-config',
      parallelismLimit: 1,
      projectRootMode: 'required',
      worktreeBinding: 'repository-root',
      stateProfileMode: 'none',
      hostLock: 'none',
      routingGroup: 'project-git',
      discoveryRequiresLease: true,
      confidence: 'medium',
      reason: 'git worktrees have mutable index/lock files and should serialize per repository',
    };
  }
  if (signals.includes('database')) {
    return {
      decision: 'database-path-single-writer',
      scopeClass: 'state-profile',
      concurrencyPolicy: 'single-writer',
      stateBinding: 'database-file',
      credentialBinding: signals.includes('secrets-or-credentials') ? 'database-credential' : 'none',
      parallelismLimit: 1,
      projectRootMode: 'optional',
      stateProfileMode: 'required',
      hostLock: 'none',
      routingGroup: 'database',
      discoveryRequiresLease: true,
      confidence: 'low',
      reason: 'database servers need explicit DB-path or credential scoping before parallelism',
    };
  }
  if (signals.includes('filesystem')) {
    return {
      decision: 'project-filesystem-single-writer',
      scopeClass: 'project-local',
      concurrencyPolicy: 'isolated-per-project',
      stateBinding: 'file',
      credentialBinding: 'none',
      parallelismLimit: 1,
      projectRootMode: 'required',
      worktreeBinding: 'project-root',
      stateProfileMode: 'none',
      hostLock: 'none',
      routingGroup: 'project-filesystem',
      discoveryRequiresLease: true,
      confidence: 'medium',
      reason: 'filesystem servers need a root boundary and should not mix project roots',
    };
  }
    if (signals.includes('memory-or-context')) {
    return {
      decision: 'state-profile-single-session',
      scopeClass: 'state-profile',
      concurrencyPolicy: 'single-session',
      stateBinding: 'context-store',
      credentialBinding: 'none',
      parallelismLimit: 1,
      projectRootMode: 'optional',
      stateProfileMode: 'required',
      hostLock: 'none',
      routingGroup: 'memory-context',
      discoveryRequiresLease: true,
      confidence: 'low',
      reason: 'memory/context servers often mutate hidden state and need profile affinity',
    };
  }
  if (signals.includes('network-open-world') || signals.includes('secrets-or-credentials')) {
    return {
      decision: transport === 'stdio' ? 'credential-scoped-stdio-review' : 'remote-credential-scoped-review',
      scopeClass: 'credential-scoped',
      concurrencyPolicy: 'single-writer',
      stateBinding: transport === 'stdio' ? 'runtime-process' : 'remote-session',
      credentialBinding: signals.includes('secrets-or-credentials') ? 'api-credential' : 'remote-origin-or-none',
      parallelismLimit: 1,
      projectRootMode: 'optional',
      stateProfileMode: 'optional',
      hostLock: 'none',
      routingGroup: transport === 'stdio' ? 'external-api-stdio' : 'remote-mcp',
      discoveryRequiresLease: true,
      defaultEnabled: false,
      confidence: 'low',
      reason: 'API/network servers need explicit credential, quota, and owner review before enabling',
    };
  }
  return {
    decision: 'unknown-conservative-review',
    scopeClass: 'configured-source',
    concurrencyPolicy: 'single-writer',
    stateBinding: 'runtime-source',
    credentialBinding: 'source-config',
    parallelismLimit: 1,
    projectRootMode: 'optional',
    stateProfileMode: 'none',
    hostLock: 'none',
    routingGroup: 'settings-only',
    discoveryRequiresLease: true,
    defaultEnabled: false,
    confidence: 'low',
    reason: 'metadata is insufficient to prove read-only or stateless behavior',
  };
}

function summarize(classifications) {
  const byDecision = {};
  const bySignal = {};
  let reviewRequired = 0;
  for (const item of classifications) {
    byDecision[item.decision] = (byDecision[item.decision] || 0) + 1;
    if (item.reviewRequired) reviewRequired += 1;
    for (const signal of item.riskSignals) {
      bySignal[signal] = (bySignal[signal] || 0) + 1;
    }
  }
  return {
    serverCount: classifications.length,
    reviewRequired,
    byDecision,
    bySignal,
  };
}

function writeOutputs(report, args) {
  if (args.write) writeJson(args.write, report);
  if (args.markdown) writeText(args.markdown, renderMarkdown(report));
}

function writeJson(relativePath, value) {
  const fullPath = path.resolve(repoRoot, relativePath);
  fs.mkdirSync(path.dirname(fullPath), { recursive: true });
  fs.writeFileSync(fullPath, `${JSON.stringify(value, null, 2)}\n`);
}

function writeText(relativePath, value) {
  const fullPath = path.resolve(repoRoot, relativePath);
  fs.mkdirSync(path.dirname(fullPath), { recursive: true });
  fs.writeFileSync(fullPath, value);
}

function renderMarkdown(report) {
  const rows = report.classifications.map((item) => (
    `| ${item.name || '—'} | ${item.decision} | ${item.suggestedPolicy.scopeClass} | ${item.suggestedPolicy.concurrencyPolicy} | ${item.riskSignals.join(', ')} | ${item.confidence} |`
  ));
  return `# MCP Registry Lab Report\n\nSchema: \`${report.schema}\`  \nStatus: **${report.status}**  \nMode: \`${report.mode}\`  \nGenerated: ${report.generatedAt}\n\nThis report is metadata-only. It does **not** install, launch, or call arbitrary third-party MCP servers.\n\n## Summary\n\n- Servers classified: ${report.summary.serverCount}\n- Servers requiring review: ${report.summary.reviewRequired}\n- Unknown default: ${report.safety.defaultUnknownPolicy}\n\n## Classifications\n\n| Server | Decision | Scope | Concurrency | Risk signals | Confidence |\n|---|---|---|---|---|---|\n${rows.join('\n')}\n\n## Required next lanes\n\n1. Sandbox launch with no user secrets, no user home directory, pinned package versions, clean environment, timeout, and process-tree kill.\n2. Safe probe only: initialize and tools/list; no destructive tool calls.\n3. Policy review comparing tool annotations, names, descriptions, transport, package registry, and user-provided trust.\n4. Concurrency torture only for allowlisted fixtures and servers.\n\n## Notes\n\n${report.notes.map((note) => `- ${note}`).join('\n')}\n`;
}

main().catch((error) => {
  console.error(error.stack || error.message || String(error));
  process.exitCode = 1;
});
