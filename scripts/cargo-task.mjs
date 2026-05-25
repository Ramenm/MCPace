#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import process from 'node:process';
import { repoRoot } from './lib/project-metadata.mjs';
import { cleanChildEnv } from './lib/safe-child-env.mjs';

function isWindows(platform = process.platform) {
  return platform === 'win32';
}

export function cargoCommand(platform = process.platform) {
  return isWindows(platform) ? 'cargo.exe' : 'cargo';
}

export function readPinnedToolchain(root = repoRoot) {
  const toolchainPath = path.join(root, 'rust-toolchain.toml');
  if (!fs.existsSync(toolchainPath)) return '';
  const text = fs.readFileSync(toolchainPath, 'utf8');
  const match = text.match(/^\s*channel\s*=\s*"([^"]+)"\s*$/m);
  return match ? match[1] : '';
}

function formatMissingCargoMessage(command) {
  const pinned = readPinnedToolchain();
  const pinnedText = pinned ? ` pinned toolchain ${pinned}` : ' Rust toolchain';
  return [
    `cargo was not found while running: cargo ${command.join(' ')}`,
    `Install Rust${pinnedText} and make sure Cargo is on PATH, then retry.`,
    'Recommended install path: rustup. On Windows, install rustup-init.exe and the MSVC build tools when prompted.',
    'On Unix/WSL, install with rustup and restart the shell if ~/.cargo/bin was just added to PATH.',
  ].join('\n');
}

function cargoEnv(overrides = {}, platform = process.platform) {
  const normalized = { ...overrides };
  if (isWindows(platform) && Object.hasOwn(normalized, 'PATH') && !Object.hasOwn(normalized, 'Path')) {
    normalized.Path = normalized.PATH;
  }
  if (isWindows(platform) && Object.hasOwn(normalized, 'Path') && !Object.hasOwn(normalized, 'PATH')) {
    normalized.PATH = normalized.Path;
  }
  return cleanChildEnv(normalized, process.env);
}

export function runCargo(command, options = {}) {
  if (command.length === 0) throw new Error('cargo-task requires a cargo subcommand');
  const platform = options.platform || process.platform;
  const executable = cargoCommand(platform);
  const env = cargoEnv(options.env || {}, platform);
  const result = spawnSync(executable, command, {
    cwd: options.cwd || repoRoot,
    env,
    stdio: options.stdio || 'inherit',
    windowsHide: true,
  });

  if (result.error && result.error.code === 'ENOENT') {
    const error = new Error(formatMissingCargoMessage(command));
    error.code = 'MCPACE_CARGO_NOT_FOUND';
    throw error;
  }
  if (result.error) throw result.error;
  return result.status ?? 1;
}

function main() {
  try {
    process.exitCode = runCargo(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`${error?.message || error}\n`);
    process.exitCode = 1;
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  main();
}
