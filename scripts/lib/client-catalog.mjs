#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { readText, repoRoot } from './project-metadata.mjs';

const TARGET_BLOCK_PATTERN = /^\s{4}ClientTarget\s*\{[\s\S]*?^\s{4}\},/gm;

function extractCatalogSection(source, sourcePath) {
  const startMarker = 'pub const CLIENT_TARGETS: &[ClientTarget] = &[';
  const start = source.indexOf(startMarker);
  if (start === -1) {
    throw new Error('failed to locate CLIENT_TARGETS catalog in src/client_catalog.rs');
  }
  const end = source.indexOf('\n];', start);
  if (end === -1) {
    throw new Error('failed to locate CLIENT_TARGETS closing bracket in src/client_catalog.rs');
  }
  return source.slice(start + startMarker.length, end);
}

function extractStringField(block, fieldName) {
  const pattern = new RegExp(`^\\s+${fieldName}:\\s*"([^"]*)"`, 'm');
  const match = block.match(pattern);
  return match ? match[1] : null;
}

function extractInstallSupport(block) {
  const installBlock = block.match(/install_support:\s*(Some\(ClientInstallSupport\s*\{[\s\S]*?\}\)|None)/m);
  if (!installBlock) {
    return {
      installSupported: false,
      preferredInstallScope: null,
      preferredInstallPath: null,
      installKind: null
    };
  }
  if (installBlock[1] === 'None') {
    return {
      installSupported: false,
      preferredInstallScope: null,
      preferredInstallPath: null,
      installKind: null
    };
  }
  const body = installBlock[1];
  const kindMatch = body.match(/kind:\s*ClientInstallKind::([A-Za-z]+)/);
  const preferredScopeMatch = body.match(/preferred_scope:\s*"([^"]*)"/);
  const preferredPathMatch = body.match(/preferred_config_path:\s*"([^"]*)"/);
  return {
    installSupported: true,
    preferredInstallScope: preferredScopeMatch ? preferredScopeMatch[1] : null,
    preferredInstallPath: preferredPathMatch ? preferredPathMatch[1] : null,
    installKind: kindMatch ? kindMatch[1] : null
  };
}

function parseTargetBlock(block) {
  return {
    id: extractStringField(block, 'id'),
    familyId: extractStringField(block, 'family_id'),
    displayName: extractStringField(block, 'display_name'),
    maturity: extractStringField(block, 'maturity'),
    proofTier: extractStringField(block, 'proof_tier'),
    surfaceClass: extractStringField(block, 'surface_class'),
    surfaceKind: extractStringField(block, 'surface_kind'),
    ...extractInstallSupport(block)
  };
}

function readCatalogSource() {
  for (const sourcePath of ['src/client_catalog/builtin.rs', 'src/client_catalog.rs']) {
    const absolutePath = path.join(repoRoot, sourcePath);
    if (!fs.existsSync(absolutePath)) {
      continue;
    }
    const source = readText(sourcePath);
    if (source.includes('pub const CLIENT_TARGETS: &[ClientTarget] = &[')) {
      return { source, sourcePath };
    }
  }
  throw new Error('failed to locate CLIENT_TARGETS catalog in src/client_catalog/builtin.rs or src/client_catalog.rs');
}

export function readClientCatalog() {
  const { source, sourcePath } = readCatalogSource();
  const catalogSection = extractCatalogSection(source, sourcePath);
  const blocks = catalogSection.match(TARGET_BLOCK_PATTERN) || [];
  const targets = blocks.map(parseTargetBlock).filter((target) => target.id);
  if (targets.length === 0) {
    throw new Error(`failed to parse any client targets from ${sourcePath}`);
  }
  return targets;
}

export function resolveCatalogSelection(selector = {}) {
  const { field, equals } = selector;
  if (!field) {
    throw new Error('catalog selection requires a field');
  }
  return readClientCatalog().filter((target) => String(target[field] ?? '') === String(equals ?? ''));
}

export function resolveProofFocusTargets(productTruth) {
  const selector = productTruth?.proofFocusSelector;
  if (!selector) {
    return [];
  }
  return resolveCatalogSelection(selector);
}

export function resolveInstallSupportTargets(productTruth) {
  const selector = productTruth?.installSupportSelector;
  if (!selector) {
    return readClientCatalog().filter((target) => target.installSupported);
  }
  return resolveCatalogSelection(selector);
}
