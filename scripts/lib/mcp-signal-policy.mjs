export function normalizeMcpSignalText(value) {
  return String(value || '').toLowerCase();
}

export function uniqueSorted(values) {
  return [...new Set(values.filter(Boolean))].sort();
}

const TEXT_SIGNAL_CACHE_LIMIT = 8192;
const textSignalCache = new Map();
const UNKNOWN_SIDE_EFFECT_SIGNALS = Object.freeze(['unknown-side-effects']);

function rememberTextSignals(key, value) {
  if (textSignalCache.size >= TEXT_SIGNAL_CACHE_LIMIT) {
    const oldestKey = textSignalCache.keys().next().value;
    textSignalCache.delete(oldestKey);
  }
  textSignalCache.set(key, value);
  return value;
}

export function classifyMcpTextSignals(input) {
  const text = normalizeMcpSignalText(Array.isArray(input) ? input.join(' ') : input);
  const cached = textSignalCache.get(text);
  if (cached) return cached;

  const signals = [];

  if (/filesystem|file-system|server-filesystem|read_file|write_file|allowed.?director|\bfiles?\b|\bpath\b/.test(text)) signals.push('filesystem');
  if (/\bgit\b|github|gitlab|bitbucket|repository|worktree|repo-root|\brepo\b/.test(text)) signals.push('git-repository');
  if (/sqlite|postgres|mysql|mongodb|database|\bsql\b|db-path|duckdb|kusto|datadog/.test(text)) signals.push('database');
  if (/playwright|puppeteer|chrome|browser|computer-use|desktop|browserstack|screenshot/.test(text)) signals.push('browser-or-desktop');
  if (/memory|sequential[- ]?thinking|context-store|knowledge|thinking|conversation|\bcontext\b/.test(text)) signals.push('memory-or-context');
  if (/fetch|http|url|web|crawl|scrape|search|wikipedia|deepl|maps/.test(text)) signals.push('network-fetch');
  if (/context7|docs|documentation|notion|sentry|slack|jira|linear|hubspot|salesforce|postman|xero|contentful|\bapi\b/.test(text)) signals.push('network-or-external-api');
  if (/slack|notion|sentry|linear|jira|hubspot|salesforce|postman|xero|contentful|oauth|token|auth/.test(text)) signals.push('credential-api');
  if (/azure|aws|gcp|cloudflare|railway|heroku|vercel|terraform|pulumi|kubernetes|\bk8s\b|argocd|circleci/.test(text)) signals.push('cloud-admin');
  if (/kubernetes|\bk8s\b|kubectl|helm/.test(text)) signals.push('cluster-control');
  if (/okta|auth0|entra|active directory|identity|scim/.test(text)) signals.push('identity-admin');
  if (/vault|secret manager|1password|bitwarden|keychain|\bsecret\b/.test(text)) signals.push('secrets-manager');
  if (/bitwarden|vault|\bsecret\b|okta|auth0|identity/.test(text)) signals.push('secret-or-identity');
  if (/stripe|paypal|billing|payment|invoice/.test(text)) signals.push('payments-financial');
  if (/stripe|paypal|billing|wallet|coinbase|evm|crypto|blockchain/.test(text)) signals.push('payments-or-wallet');
  if (/wallet|ethereum|blockchain|\bweb3\b|coinbase|evm|crypto/.test(text)) signals.push('blockchain-wallet');
  if (/token|api[_-]?key|auth|oauth|bearer|credential/.test(text)) signals.push('credentials-or-auth');
  if (/code-runner|command-runner|\bshell\b|terminal|\bexec\b|\bspawn\b|\bprocess\b/.test(text)) signals.push('shell-or-process');
  if (/\btime\b|date|timezone|calculator|math|uuid|hello-world/.test(text)) signals.push('local-utility');
  if (/read.?only|readonly|\blist\b|\bget\b|inspect/.test(text)) signals.push('readonly-tools');

  return rememberTextSignals(text, signals.length ? Object.freeze(uniqueSorted(signals)) : UNKNOWN_SIDE_EFFECT_SIGNALS);
}

export function packageSignalText(pkg = {}) {
  return [
    pkg.name,
    pkg.description,
    ...(Array.isArray(pkg.keywords) ? pkg.keywords : []),
    pkg.version,
  ].join(' ');
}

export function serverSignalText(raw = {}) {
  const args = Array.isArray(raw.args) ? raw.args : [];
  const policyText = Object.values(raw.policy || {}).filter((value) => typeof value === 'string');
  const operational = [
    raw.source,
    raw.transport,
    raw.launcher,
    raw.command,
    raw.url,
    ...args,
    raw.description,
    ...policyText,
  ].filter(Boolean);

  if (operational.length) return operational.join(' ');
  return [raw.serverId, raw.name, raw.id].filter(Boolean).join(' ');
}

export function signalsFromPackageMetadata(pkg = {}) {
  return classifyMcpTextSignals(packageSignalText(pkg));
}

export function signalsFromServerDescriptor(raw = {}) {
  return classifyMcpTextSignals(serverSignalText(raw));
}

export function policyRequiresReview(policy) {
  return /credential|admin|dangerous|host-lock|sensitive/.test(String(policy || '')) || policy === 'review-required-single-writer';
}


const PACKAGE_CLASSIFICATION_CACHE_LIMIT = 4096;
const packageClassificationCache = new Map();

function cachedObject(value) {
  Object.freeze(value.signals);
  Object.freeze(value.locks);
  return Object.freeze(value);
}

function rememberPackageClassification(key, value) {
  if (packageClassificationCache.size >= PACKAGE_CLASSIFICATION_CACHE_LIMIT) {
    const oldestKey = packageClassificationCache.keys().next().value;
    packageClassificationCache.delete(oldestKey);
  }
  packageClassificationCache.set(key, value);
  return value;
}

export function classifyMcpPackageMetadata(pkg = {}) {
  const cacheKey = normalizeMcpSignalText(packageSignalText(pkg));
  const cached = packageClassificationCache.get(cacheKey);
  if (cached) return cached;

  const signals = classifyMcpTextSignals(cacheKey);
  const signalSet = new Set(signals);
  let policy = 'review-required-single-writer';
  let stateClass = 'unknown-stateful';
  let locks = ['server'];
  let maxWorkers = 1;

  if (signalSet.has('browser-or-desktop')) {
    policy = 'shared-exclusive-host-lock';
    stateClass = 'host-context-stateful';
    locks = ['browser-context', 'host-session'];
  } else if (signalSet.has('shell-or-process')) {
    policy = 'disabled-dangerous-command-runner';
    stateClass = 'host-process-stateful';
    locks = ['host-session'];
  } else if (
    signalSet.has('cloud-admin') ||
    signalSet.has('cluster-control') ||
    signalSet.has('identity-admin') ||
    signalSet.has('secrets-manager') ||
    signalSet.has('secret-or-identity') ||
    signalSet.has('payments-financial') ||
    signalSet.has('payments-or-wallet') ||
    signalSet.has('blockchain-wallet')
  ) {
    policy = 'sensitive-admin-credential-review';
    stateClass = 'credential-tenant-stateful';
    locks = ['credential-profile', 'tenant'];
  } else if (signalSet.has('credential-api') || signalSet.has('credentials-or-auth')) {
    policy = 'credential-scoped-review';
    stateClass = 'credential-session-stateful';
    locks = ['credential-profile', 'tenant'];
  } else if (signalSet.has('database')) {
    policy = 'database-path-single-writer';
    stateClass = 'project-stateful';
    locks = ['database', 'project'];
  } else if (signalSet.has('git-repository')) {
    policy = 'project-repo-single-writer';
    stateClass = 'project-stateful';
    locks = ['repo', 'project'];
  } else if (signalSet.has('filesystem')) {
    policy = 'project-filesystem-single-writer';
    stateClass = 'project-stateful';
    locks = ['file', 'project'];
  } else if (signalSet.has('memory-or-context')) {
    policy = 'state-profile-single-session';
    stateClass = 'session-stateful';
    locks = ['session', 'context-store'];
  } else if (signalSet.has('network-fetch') || signalSet.has('network-or-external-api')) {
    policy = 'network-fetch-review';
    stateClass = 'readonly-network-candidate';
    locks = ['provider-budget'];
    maxWorkers = 2;
  } else if (signalSet.has('local-utility') || signalSet.has('readonly-tools')) {
    policy = 'local-utility-multi-reader-candidate';
    stateClass = 'stateless-candidate';
    locks = ['server'];
    maxWorkers = 4;
  }

  const reviewRequired = policyRequiresReview(policy);
  return rememberPackageClassification(cacheKey, cachedObject({
    signals,
    policy,
    stateClass,
    locks,
    maxWorkers,
    maxInFlightPerWorker: 1,
    executeDefault: false,
    reviewRequired,
    rationale: 'metadata-only classification; never auto-execute random package code',
  }));
}
