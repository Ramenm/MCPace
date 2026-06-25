#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { repoRoot } from './lib/project-metadata.mjs';

const cli = path.join(repoRoot, 'node_modules', 'publint', 'src', 'cli.js');
const args = ['--max-old-space-size=128'];
if (fs.existsSync(cli)) {
  args.push(cli, 'packages/npm/cli');
} else {
  args.push('-e', "import('publint').catch(error=>{console.error(error);process.exit(1)})");
}

const result = spawnSync(process.execPath, args, {
  cwd: repoRoot,
  stdio: 'inherit',
  windowsHide: true,
});
if (result.error) {
  console.error(result.error.message || result.error);
  process.exit(1);
}
process.exit(result.status ?? 1);
