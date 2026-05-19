import {
  classifyMcpPackageMetadata,
  normalizeMcpSignalText,
  packageSignalText,
  policyRequiresReview as sharedPolicyRequiresReview,
  signalsFromPackageMetadata,
} from './mcp-signal-policy.mjs';

export function normalizePolicyText(value) {
  return normalizeMcpSignalText(value);
}

export function metadataText(pkg = {}) {
  return normalizeMcpSignalText(packageSignalText(pkg));
}

export function classifyPackageMetadata(pkg = {}) {
  return classifyMcpPackageMetadata(pkg);
}

export function policyRequiresReview(policy) {
  return sharedPolicyRequiresReview(policy);
}

export function minimalPackageProfile(item = {}) {
  return {
    name: item.name,
    version: item.version || 'latest',
    description: item.description || '',
    keywords: Array.isArray(item.keywords) ? item.keywords : [],
    date: item.date || null,
    links: item.links || {},
    classification: classifyMcpPackageMetadata(item),
  };
}

export { signalsFromPackageMetadata };
