import { signalsFromServerDescriptor } from './mcp-signal-policy.mjs';
export function normalizeName(value) {
  return String(value || '')
    .trim()
    .replace(/^@/, '')
    .replace(/[\\/]+/g, '-')
    .replace(/[^A-Za-z0-9_.-]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .toLowerCase() || 'unnamed';
}

export function normalizeTransport(value, server = {}) {
  const raw = String(value || server.type || '').trim().toLowerCase();
  if (/^(sse|remote-sse|http\+sse|http-sse|legacy-sse|sse-legacy)$/.test(raw)) return 'sse-legacy';
  if (/^(streamablehttp|streamable-http|http-stream|remote-http|remote|http)$/.test(raw)) return 'streamable-http';
  if (/^(stdio|local|local-stdio|local-command|command)$/.test(raw)) return 'stdio';
  if (server.url) return 'streamable-http';
  return 'stdio';
}

export function launcherFrom(server = {}) {
  const text = `${server.command || ''} ${(server.args || []).join(' ')}`.toLowerCase();
  if (server.url) return 'remote-url';
  if (/\bnpx\b/.test(text)) return 'npx';
  if (/\buvx\b/.test(text)) return 'uvx';
  if (/\bdocker\b|\bpodman\b|^oci:|^docker:/.test(text)) return 'oci';
  if (!text.trim()) return 'unspecified';
  return 'local-command';
}

export function signalsFrom(raw = {}) {
  return signalsFromServerDescriptor(raw);
}


export function explicitStateless(policy = {}) {
  const state = String(policy.stateBinding || '').trim().toLowerCase();
  const concurrency = String(policy.concurrencyPolicy || '').trim().toLowerCase();
  return policy.stateless === true || ((concurrency === 'multi-reader' || concurrency === 'read-only' || concurrency === 'readonly') && (state === 'none' || state === 'stateless'));
}

export function profileFrom(raw = {}, source = 'settings') {
  const serverId = normalizeName(raw.serverId || raw.name || raw.id || 'unnamed');
  const transport = normalizeTransport(raw.transport, raw);
  const launcher = raw.launcher || launcherFrom(raw);
  const policy = raw.policy || {};
  const signals = signalsFrom({ ...raw, serverId, transport, launcher });
  const remote = transport === 'streamable-http' || Boolean(raw.url);
  const profile = {
    serverId,
    source,
    transport,
    launcher,
    parallelSafetyClass: 'P0_unknown_stdio',
    defaultPoolModel: transport === 'stdio' ? 'process-pool' : 'singleton',
    maxWorkers: 1,
    maxInFlightPerWorker: 1,
    lockDomains: ['server'],
    stateless: false,
    stateful: true,
    evidence: [
      { kind: 'source-config', confidence: 0.45, signals, summary: 'Conservative profile inferred from transport, launcher, command/url, args, and optional operator policy hints.' },
    ],
  };

  if (transport === 'sse-legacy') {
    return { ...profile, parallelSafetyClass: 'PX_legacy_compat', defaultPoolModel: 'legacy-disabled', maxWorkers: 0, maxInFlightPerWorker: 0, lockDomains: ['legacy-transport'] };
  }

  if (remote) {
    if (explicitStateless(policy)) {
      return { ...profile, parallelSafetyClass: 'P4_stateless_remote_candidate', defaultPoolModel: 'remote-http-shared-pool', maxWorkers: Number(policy.parallelismLimit || 8), lockDomains: ['provider-budget'], stateless: true, stateful: false };
    }
    return { ...profile, parallelSafetyClass: 'P2_session_safe', defaultPoolModel: 'remote-http-session-pool', maxWorkers: Number(policy.parallelismLimit || 1), lockDomains: ['transport-session', 'credential:remote-origin-or-credential'], stateless: false, stateful: true };
  }

  if (signals.includes('browser-or-desktop')) {
    return { ...profile, parallelSafetyClass: 'PX_forbidden_browser_until_context_isolated', defaultPoolModel: 'session-pool', maxWorkers: 1, lockDomains: ['browser-context', 'host-session'], stateless: false };
  }
  if (signals.includes('shell-or-process')) {
    return { ...profile, parallelSafetyClass: 'PX_forbidden_process_until_sandboxed', defaultPoolModel: 'singleton', maxWorkers: 1, lockDomains: ['host-session'], stateless: false };
  }
  if (signals.includes('memory-or-context')) {
    return { ...profile, parallelSafetyClass: 'P2_session_safe', defaultPoolModel: 'singleton', maxWorkers: 1, lockDomains: ['context-store', 'session'], stateless: false };
  }
  if (signals.includes('git-repository')) {
    return { ...profile, parallelSafetyClass: 'P3_project_safe', defaultPoolModel: 'project-pool', maxWorkers: 1, lockDomains: ['repo', 'project'], stateless: false };
  }
  if (signals.includes('database')) {
    return { ...profile, parallelSafetyClass: 'P3_project_safe', defaultPoolModel: 'project-pool', maxWorkers: 1, lockDomains: ['db', 'project'], stateless: false };
  }
  if (signals.includes('filesystem')) {
    return { ...profile, parallelSafetyClass: 'P3_project_safe', defaultPoolModel: 'project-pool', maxWorkers: 1, lockDomains: ['file', 'project'], stateless: false };
  }
  if (signals.includes('cloud-admin') || signals.includes('identity-admin') || signals.includes('cluster-control') || signals.includes('secrets-manager') || signals.includes('payments-financial') || signals.includes('blockchain-wallet') || signals.includes('credentials-or-auth') || signals.includes('network-or-external-api')) {
    return { ...profile, parallelSafetyClass: 'P2_session_safe', defaultPoolModel: 'credential-session-pool', maxWorkers: 1, lockDomains: ['credential:credential-profile', 'tenant'], stateless: false };
  }
  if (signals.includes('network-fetch')) {
    return { ...profile, parallelSafetyClass: 'P1_readonly_candidate', defaultPoolModel: 'process-pool', maxWorkers: 2, lockDomains: ['provider-budget'], stateless: true, stateful: false };
  }
  if (signals.includes('local-utility') || signals.includes('readonly-tools') || explicitStateless(policy)) {
    return { ...profile, parallelSafetyClass: 'P1_readonly_candidate', defaultPoolModel: 'process-pool', maxWorkers: Number(policy.parallelismLimit || 4), lockDomains: ['server'], stateless: true, stateful: false };
  }
  return profile;
}
