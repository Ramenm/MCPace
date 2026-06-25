#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { repoRoot } from './lib/project-metadata.mjs';

const htmlPath = path.join(repoRoot, 'src/dashboard/index.html');
const html = fs.readFileSync(htmlPath, 'utf8');

const checks = [
  {
    name: 'visibility resume does not force overview rebuild',
    pass: /else refreshDashboard\(\{ reason: "visible" \}\)/.test(html) && !/reason: "visible"[^\n]+force: true/.test(html),
  },
  {
    name: 'refresh overlap is suppressed',
    pass: /state\.refreshing && !options\.forceAbort/.test(html),
  },
  {
    name: 'failed refreshes use exponential backoff',
    pass: /MAX_REFRESH_FAILURE_BACKOFF_MS/.test(html) && /Math\.pow\(2, Math\.min\(state\.failureCount/.test(html),
  },
  {
    name: 'page freeze aborts in-flight refresh and defers work',
    pass: /document\.addEventListener\("freeze"/.test(html) && /state\.controller\) state\.controller\.abort/.test(html),
  },
  {
    name: 'page resume uses cached refresh path',
    pass: /document\.addEventListener\("resume"/.test(html) && /refreshDashboard\(\{ reason: "resume" \}\)/.test(html) && !/reason: "resume"[^\n]+force: true/.test(html),
  },
  {
    name: 'discarded or bfcache pages recover once without forced rebuild',
    pass: /document\.wasDiscarded/.test(html) && /window\.addEventListener\("pageshow"/.test(html) && /reason: "pageshow"/.test(html) && !/reason: "pageshow"[^\n]+force: true/.test(html),
  },
];
const failures = checks.filter((check) => !check.pass);
const report = {
  schema: 'mcpace.browserLifecycleProof.v1',
  generatedAt: new Date().toISOString(),
  ok: failures.length === 0,
  file: 'src/dashboard/index.html',
  checks,
};
if (process.argv.includes('--json')) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
else if (report.ok) process.stdout.write(`PASS browser lifecycle proof: ${checks.length}/${checks.length}\n`);
else process.stderr.write(failures.map((check) => `FAIL ${check.name}`).join('\n') + '\n');
process.exit(report.ok ? 0 : 1);
