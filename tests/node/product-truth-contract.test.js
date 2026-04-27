const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { read, readJson, packageVersion, repoRoot } = require('./helpers');

function normalize(value) {
  return String(value || '').replace(/\s+/g, ' ').trim();
}

const truthDocs = [
  'README.md',
  'STATE.md',
  'docs/product-truth-and-beta-gate.md',
  'packages/npm/cli/README.md',
  'reports/summary.md'
];
const clientCatalogModule = path.join(repoRoot, 'scripts', 'lib', 'client-catalog.mjs');

async function loadCatalogHelpers() {
  return import(pathToFileUrl(clientCatalogModule));
}

test('machine-readable product truth stays versioned and structurally complete', async () => {
  const value = readJson('docs/product-truth.json');
  const { resolveInstallSupportTargets, resolveProofFocusTargets } = await loadCatalogHelpers();
  const proofTargets = resolveProofFocusTargets(value);
  const installTargets = resolveInstallSupportTargets(value);

  assert.equal(value.version, packageVersion());
  assert.match(value.currentPromise, /One local MCPace endpoint/i);
  assert.equal(value.entrypointContract.product, 'serve');
  assert.equal(value.entrypointContract.internalLifecycle, 'hub');
  assert.equal(value.entrypointContract.optionalView, 'dashboard');
  assert.deepEqual(value.proofFocusSelector, {
    catalogSource: 'src/client_catalog.rs',
    field: 'proofTier',
    equals: 'tier-1'
  });
  assert.deepEqual(value.installSupportSelector, {
    catalogSource: 'src/client_catalog.rs',
    field: 'installSupported',
    equals: true
  });
  assert.ok(proofTargets.length >= 1);
  assert.ok(installTargets.length >= proofTargets.length);
  assert.ok(proofTargets.every((target) => target.proofTier === 'tier-1'));
  assert.ok(proofTargets.every((target) => target.surfaceClass === 'local'));
  assert.ok(proofTargets.every((target) => target.installSupported === true));
  assert.ok(installTargets.every((target) => target.installSupported === true));
  assert.equal(value.activation.provenToday.length, 3);
  assert.equal(value.activation.betaTruthRequires.length, 3);
});

test('current promise and catalog-driven proof-tier selection stay aligned across user-facing docs', () => {
  const truth = readJson('docs/product-truth.json');
  const combined = normalize(truthDocs.map((file) => read(file)).join('\n'));
  assert.ok(combined.includes(normalize(truth.currentPromise)));
  assert.match(combined, /catalog-driven/i);
  assert.match(combined, /proofTier\s*=\s*tier-1/i);
  assert.match(combined, /`?serve`? is the product/i);
  assert.match(combined, /`?hub`? as lifecycle machinery|`?hub`? is internal\/operator-facing lifecycle machinery/i);
});

test('docs index and verification snapshot expose the machine-readable product truth', async () => {
  const docsIndex = read('docs/README.md');
  const verification = readJson('reports/verification-latest.json');
  const truth = readJson('docs/product-truth.json');
  const { resolveInstallSupportTargets, resolveProofFocusTargets } = await loadCatalogHelpers();
  const proofFocusSurfaces = resolveProofFocusTargets(truth).map((target) => target.id);
  const installSupportedSurfaces = resolveInstallSupportTargets(truth).map((target) => target.id);

  assert.match(docsIndex, /product-truth\.json/);
  assert.equal(verification.productTruth.currentPromise, truth.currentPromise);
  assert.deepEqual(verification.productTruth.proofFocusSurfaces, proofFocusSurfaces);
  assert.deepEqual(verification.productTruth.installSupportedSurfaces, installSupportedSurfaces);
  assert.equal(verification.capabilityInventory.claimStatusCounts['control-plane-only'], 4);
});

function pathToFileUrl(filePath) {
  return pathToFileURL(filePath).href;
}
