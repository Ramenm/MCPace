#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function parseArgs(argv) {
  const args = {
    root: repoRoot,
    json: false,
    write: null,
    markdown: null,
    includeEdgeCases: true,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    if (token === '--root') args.root = path.resolve(argv[++i] || '.');
    else if (token === '--json') args.json = true;
    else if (token === '--write') args.write = path.resolve(argv[++i] || '');
    else if (token === '--markdown') args.markdown = path.resolve(argv[++i] || '');
    else if (token === '--no-edge-cases') args.includeEdgeCases = false;
    else if (token === '--help' || token === '-h') args.help = true;
    else throw new Error(`unknown adaptive-worker-plan argument: ${token}`);
  }
  return args;
}

function printHelp() {
  console.log(`Usage: node scripts/adaptive-worker-plan.mjs [--json] [--write FILE] [--markdown FILE]\n\nDerives scheduler worker plans from adaptive MCP server profiles and validates pool/lock/degradation invariants.`);
}

function readJson(relative, fallback = null, root = repoRoot) {
  const file = path.join(root, relative);
  if (!fs.existsSync(file)) return fallback;
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

function runAdaptiveAudit(root) {
  const result = spawnSync(process.execPath, ['scripts/adaptive-parallelism-audit.mjs', '--json'], {
    cwd: root,
    encoding: 'utf8',
    env: { ...process.env, NO_COLOR: '1' },
    timeout: 60_000,
  });
  if (result.status !== 0) {
    throw new Error(`adaptive audit failed: ${result.stderr || result.stdout}`);
  }
  return JSON.parse(result.stdout);
}

function unique(values) {
  return [...new Set(values.filter(Boolean))];
}

function budgetClassFor(profile) {
  const text = `${profile.serverId} ${profile.parallelSafetyClass} ${profile.defaultPoolModel} ${(profile.lockDomains || []).join(' ')}`.toLowerCase();
  if (/paid|billing|cost/.test(text)) return 'paid';
  if (/credential|provider|remote|http|network/.test(text)) return 'metered';
  if (/unknown|p0_/.test(text)) return 'unknown';
  return 'free';
}

function affinityKeysFor(profile) {
  switch (profile.defaultPoolModel) {
    case 'remote-http-session-pool':
      return unique(['transportSessionId', 'sessionId', 'credentialProfile', 'tenantId']);
    case 'credential-session-pool':
      return unique(['credentialProfile', 'sessionId', 'tenantId']);
    case 'project-pool':
      return unique(['projectRoot', 'sessionId', 'clientInstanceId']);
    case 'session-pool':
      return unique(['browserContextId', 'sessionId', 'clientInstanceId', 'transportSessionId']);
    case 'process-pool':
      return unique(['clientInstanceId', 'sessionId', 'projectRoot']);
    case 'singleton':
      return unique(['tenantId']);
    case 'legacy-disabled':
      return [];
    default:
      return unique(['sessionId']);
  }
}

function lockForDomain(domain) {
  const value = String(domain || 'server');
  if (/budget|provider/i.test(value)) return { domain: value, mode: 'token-bucket', key: `${value}:{provider}` };
  if (/credential/i.test(value)) return { domain: value, mode: 'exclusive', key: `${value}:{credentialProfile}` };
  if (/file/i.test(value)) return { domain: value, mode: 'write', key: `${value}:{projectRoot}:{resourcePath}` };
  if (/repo/i.test(value)) return { domain: value, mode: 'write', key: `${value}:{repoRoot}` };
  if (/project/i.test(value)) return { domain: value, mode: 'write', key: `${value}:{projectRoot}` };
  if (/browser|session/i.test(value)) return { domain: value, mode: 'exclusive', key: `${value}:{sessionId}:{browserContextId}` };
  if (/legacy/i.test(value)) return { domain: value, mode: 'exclusive', key: `${value}:disabled` };
  return { domain: value, mode: 'exclusive', key: `${value}:{serverId}` };
}

function workerPoolKey(profile) {
  const id = profile.serverId || 'unknown';
  switch (profile.defaultPoolModel) {
    case 'project-pool':
      return `${id}:project:{projectRoot}`;
    case 'session-pool':
      return `${id}:session:{sessionId}:context:{browserContextId}`;
    case 'credential-session-pool':
      return `${id}:credential:{credentialProfile}:session:{sessionId}`;
    case 'remote-http-session-pool':
      return `${id}:remote:{transportSessionId}:credential:{credentialProfile}`;
    case 'process-pool':
      return `${id}:process:{projectRoot}:{clientInstanceId}`;
    case 'singleton':
      return `${id}:singleton`;
    case 'legacy-disabled':
      return `${id}:legacy-disabled`;
    default:
      return `${id}:default:{sessionId}`;
  }
}

function degradationPolicyFor(profile) {
  const disabled = profile.defaultPoolModel === 'legacy-disabled';
  return {
    onConflict: disabled ? 'remain-disabled-and-require-migration' : 'downgrade-to-singleton-and-record-conflict-evidence',
    onCrashLoop: disabled ? 'remain-disabled' : 'halve-workers-then-singleton-after-repeated-crashes',
    onAuthMixup: 'disable-server-until-credential-boundary-is-reviewed',
    onLatencyRegression: disabled ? 'no-op' : 'reduce-workers-before-raising-timeouts',
  };
}

function planFromProfile(profile, source = 'runtime-profile') {
  const locks = (profile.lockDomains || ['server']).map(lockForDomain);
  const plan = {
    serverId: profile.serverId,
    source,
    parallelSafetyClass: profile.parallelSafetyClass,
    poolModel: profile.defaultPoolModel,
    workerPoolKey: workerPoolKey(profile),
    maxWorkers: Number(profile.maxWorkers || 0),
    maxInFlightPerWorker: Number(profile.maxInFlightPerWorker || 0),
    affinityKeys: affinityKeysFor(profile),
    locks,
    requiresConsent: String(profile.parallelSafetyClass || '').startsWith('PX_') || String(profile.parallelSafetyClass || '').startsWith('P0_'),
    budgetClass: budgetClassFor(profile),
    degradationPolicy: degradationPolicyFor(profile),
  };
  if (plan.poolModel === 'legacy-disabled') {
    plan.maxWorkers = 0;
    plan.maxInFlightPerWorker = 0;
  }
  return plan;
}

function checkPlan(plan) {
  const failures = [];
  if (!plan.serverId) failures.push('serverId missing');
  if (!plan.poolModel) failures.push('poolModel missing');
  if (!plan.workerPoolKey) failures.push('workerPoolKey missing');
  if (!Number.isInteger(plan.maxWorkers) || plan.maxWorkers < 0) failures.push('maxWorkers invalid');
  if (!Number.isInteger(plan.maxInFlightPerWorker) || plan.maxInFlightPerWorker < 0) failures.push('maxInFlightPerWorker invalid');
  if (!Array.isArray(plan.affinityKeys)) failures.push('affinityKeys invalid');
  if (!Array.isArray(plan.locks) || plan.locks.length === 0) failures.push('locks missing');
  if (!plan.degradationPolicy?.onConflict || !plan.degradationPolicy?.onCrashLoop || !plan.degradationPolicy?.onAuthMixup || !plan.degradationPolicy?.onLatencyRegression) failures.push('degradationPolicy incomplete');
  if (String(plan.parallelSafetyClass || '').startsWith('P0_') && plan.maxInFlightPerWorker !== 1) failures.push('unknown server must stay one in-flight per worker');
  if (String(plan.parallelSafetyClass || '').startsWith('PX_') && plan.requiresConsent !== true) failures.push('high-risk profile must require consent');
  if (plan.poolModel === 'legacy-disabled' && (plan.maxWorkers !== 0 || plan.maxInFlightPerWorker !== 0)) failures.push('legacy-disabled must have zero workers and in-flight');
  if (plan.poolModel === 'session-pool' && !plan.affinityKeys.includes('sessionId')) failures.push('session-pool must include sessionId affinity');
  if (plan.poolModel === 'project-pool' && !plan.affinityKeys.includes('projectRoot')) failures.push('project-pool must include projectRoot affinity');
  if (plan.poolModel === 'credential-session-pool' && !plan.affinityKeys.includes('credentialProfile')) failures.push('credential-session-pool must include credentialProfile affinity');
  if (plan.poolModel === 'remote-http-session-pool' && !plan.affinityKeys.includes('transportSessionId')) failures.push('remote-http-session-pool must include transportSessionId affinity');
  if (plan.poolModel === 'process-pool' && plan.maxInFlightPerWorker !== 1) failures.push('process-pool is probe-gated and must stay one in-flight per worker here');
  if (/browser/i.test(plan.serverId) || /browser/.test(plan.workerPoolKey)) {
    if (!plan.affinityKeys.includes('browserContextId')) failures.push('browser automation must include browserContextId affinity');
    if (!plan.locks.some((lock) => /browser-context|session/i.test(lock.domain))) failures.push('browser automation must carry browser/session lock');
  }
  return failures;
}

function renderMarkdown(report) {
  const lines = [];
  lines.push('# Adaptive worker plan');
  lines.push('');
  lines.push(`Generated: ${report.generatedAt}`);
  lines.push('');
  lines.push(`Status: **${report.status}**`);
  lines.push('');
  lines.push(`Plans: ${report.summary.planCount}; blockers: ${report.blockers.length}; warnings: ${report.warnings.length}.`);
  lines.push('');
  lines.push('| Server | Source | Safety | Pool | Workers | In-flight/worker | Affinity | Locks | Consent | Budget |');
  lines.push('|---|---|---|---|---:|---:|---|---|---:|---|');
  for (const plan of report.plans) {
    const locks = plan.locks.map((lock) => `${lock.domain}:${lock.mode}`).join(', ');
    lines.push(`| ${plan.serverId} | ${plan.source} | ${plan.parallelSafetyClass} | ${plan.poolModel} | ${plan.maxWorkers} | ${plan.maxInFlightPerWorker} | ${plan.affinityKeys.join(', ') || 'none'} | ${locks || 'none'} | ${plan.requiresConsent ? 'yes' : 'no'} | ${plan.budgetClass} |`);
  }
  lines.push('');
  lines.push('## Invariants');
  lines.push('');
  for (const check of report.checks) lines.push(`- ${check.ok ? 'PASS' : 'FAIL'} ${check.id}: ${check.detail}`);
  if (report.blockers.length) {
    lines.push('');
    lines.push('## Blockers');
    lines.push('');
    for (const blocker of report.blockers) lines.push(`- ${blocker}`);
  }
  if (report.warnings.length) {
    lines.push('');
    lines.push('## Warnings');
    lines.push('');
    for (const warning of report.warnings) lines.push(`- ${warning}`);
  }
  lines.push('');
  return `${lines.join('\n')}\n`;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printHelp();
    return;
  }
  const pkg = readJson('package.json', {}, args.root);
  const audit = runAdaptiveAudit(args.root);
  const runtimePlans = audit.profiles.map((profile) => planFromProfile(profile, 'runtime-profile'));
  const edgePlans = args.includeEdgeCases ? audit.edgeCases.map((edge) => planFromProfile({ serverId: edge.id, ...edge.actual }, 'edge-fixture')) : [];
  const plans = [...runtimePlans, ...edgePlans];
  const planFailures = plans.flatMap((plan) => checkPlan(plan).map((failure) => `${plan.serverId}: ${failure}`));
  const checks = [
    { id: 'profiles-materialized', ok: runtimePlans.length >= 4, detail: 'Runtime server profiles resolve to worker plans.' },
    { id: 'edge-cases-materialized', ok: !args.includeEdgeCases || edgePlans.length >= 10, detail: 'Synthetic adaptive edge cases resolve to worker plans.' },
    { id: 'unknown-safe', ok: plans.every((plan) => !String(plan.parallelSafetyClass).startsWith('P0_') || plan.maxInFlightPerWorker === 1), detail: 'Unknown servers remain one in-flight per worker.' },
    { id: 'legacy-disabled', ok: plans.every((plan) => plan.poolModel !== 'legacy-disabled' || (plan.maxWorkers === 0 && plan.maxInFlightPerWorker === 0)), detail: 'Legacy transports are disabled for worker scheduling.' },
    { id: 'affinity-boundaries', ok: plans.every((plan) => checkPlan(plan).length === 0), detail: 'Every worker plan has required affinity, lock, consent, and degradation boundaries.' },
  ];
  const blockers = [...checks.filter((check) => !check.ok).map((check) => `${check.id}: ${check.detail}`), ...planFailures];
  const warnings = [];
  for (const plan of plans) {
    if (String(plan.parallelSafetyClass).startsWith('P0_')) warnings.push(`${plan.serverId}: generated conservative plan; safe probes required before raising concurrency.`);
    if (plan.requiresConsent) warnings.push(`${plan.serverId}: consent/review gate remains required before risky execution.`);
    if (plan.budgetClass === 'metered' || plan.budgetClass === 'paid') warnings.push(`${plan.serverId}: budget/rate-limit guardrail must be enforced at runtime.`);
  }
  const report = {
    schema: 'mcpace.adaptiveWorkerPlan.v1',
    generatedAt: new Date().toISOString(),
    project: { name: 'mcpace', version: pkg.version || '0.0.0' },
    status: blockers.length ? 'blocked' : 'pass',
    summary: {
      planCount: plans.length,
      runtimePlanCount: runtimePlans.length,
      edgePlanCount: edgePlans.length,
      consentPlanCount: plans.filter((plan) => plan.requiresConsent).length,
      meteredPlanCount: plans.filter((plan) => plan.budgetClass === 'metered' || plan.budgetClass === 'paid').length,
    },
    plans,
    checks,
    warnings: unique(warnings),
    blockers,
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
  else console.log(`adaptive worker plan: ${report.status} (${report.summary.planCount} plans)`);
  if (blockers.length) process.exitCode = 1;
}

try {
  main();
} catch (error) {
  console.error(error?.stack || error?.message || String(error));
  process.exitCode = 2;
}
