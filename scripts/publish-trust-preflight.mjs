#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { repoRoot } from './lib/project-metadata.mjs';

const workflowPath = path.join(repoRoot, '.github/workflows/publish-npm.yml');
const workflow = fs.readFileSync(workflowPath, 'utf8');
function publishTokenFallbackOk(text) {
  const tokenReference = /\b(?:NPM_TOKEN|NODE_AUTH_TOKEN|NPM_CONFIG_[A-Z0-9_]*TOKEN)\b/i;
  if (!tokenReference.test(text)) return true;
  const strippedAllowedBootstrapLines = text.replace(/^\s*NODE_AUTH_TOKEN:\s*\$\{\{\s*secrets\.NPM_TOKEN\s*\}\}\s*$/gm, '');
  return /environment:\s*npm-publish/.test(text) && !tokenReference.test(strippedAllowedBootstrapLines);
}

const checks = [
  {
    name: 'workflow uses GitHub OIDC id-token permission',
    pass: /id-token:\s*write/.test(workflow),
  },
  {
    name: 'workflow limits npm token fallback to protected initial bootstrap',
    pass: publishTokenFallbackOk(workflow),
  },
  {
    name: 'publish lane validates native package contract before publish',
    pass: /verify-npm-publish-contract\.mjs --enforce/.test(workflow),
  },
  {
    name: 'publish commands request provenance statements',
    pass: workflow.split(/\r?\n/).filter((line) => line.includes('npm publish') && !line.trim().startsWith('description:')).every((line) => line.includes('--provenance')),
  },
  {
    name: 'real publish is tag-protected while branch dispatch is dry-run only',
    pass: /if:\s*startsWith\(github\.ref, 'refs\/tags\/'\)(?:\s*\|\|\s*\(github\.event_name == 'workflow_dispatch' && inputs\.dry_run == true\))?/.test(workflow) && /environment:\s*npm-publish/.test(workflow),
  },
];
const failures = checks.filter((check) => !check.pass);
const report = {
  schema: 'mcpace.publishTrustPreflight.v1',
  generatedAt: new Date().toISOString(),
  ok: failures.length === 0,
  workflow: '.github/workflows/publish-npm.yml',
  checks,
};
if (process.argv.includes('--json')) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
else if (report.ok) process.stdout.write(`PASS npm trusted publishing preflight: ${checks.length}/${checks.length}\n`);
else process.stderr.write(failures.map((check) => `FAIL ${check.name}`).join('\n') + '\n');
process.exit(report.ok ? 0 : 1);
