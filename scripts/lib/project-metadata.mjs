#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

export function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(repoRoot, relativePath), 'utf8'));
}

export function readText(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), 'utf8');
}

export function extractTomlPackageField(text, fieldName) {
  const pattern = new RegExp(`^${fieldName}\\s*=\\s*"([^"]+)"`, 'm');
  const match = String(text || '').match(pattern);
  return match ? match[1] : null;
}

export function extractTomlPackageName(text) {
  return extractTomlPackageField(text, 'name');
}

export function extractTomlVersion(text) {
  return extractTomlPackageField(text, 'version');
}

export function toKebabCase(value) {
  return (
    String(value)
      .trim()
      .toLowerCase()
      .replace(/^@/, '')
      .replace(/\//g, '-')
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .replace(/-workspace$/, '')
      .replace(/-cli$/, '') || 'mcpace'
  );
}

export function deriveProjectName() {
  const cargoName = extractTomlPackageName(readText('Cargo.toml'));
  if (cargoName) {
    return toKebabCase(cargoName);
  }

  const rootPkgName = readJson('package.json').name;
  if (rootPkgName) {
    return toKebabCase(rootPkgName);
  }

  const readme = readText('README.md');
  const heading = readme.match(/^#\s+(.+)$/m);
  return toKebabCase(heading ? heading[1] : 'mcpace');
}

export function deriveProjectVersion() {
  return extractTomlVersion(readText('Cargo.toml')) || readJson('package.json').version || '0.1.0';
}

export function readRootPackageJson() {
  return readJson('package.json');
}

export function readCliPackageJson() {
  return readJson(path.join('packages', 'npm', 'cli', 'package.json'));
}
